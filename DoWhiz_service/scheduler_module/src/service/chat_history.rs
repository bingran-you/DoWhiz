use std::collections::{HashMap, HashSet};
use std::env;
use std::path::Path;

use crate::slack_store::SlackStore;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use reqwest::blocking::Client;
use reqwest::header::HeaderMap as ReqwestHeaderMap;
use serde::{Deserialize, Serialize};
use std::time::Duration as StdDuration;

use super::state::AppState;
use super::{BoxError, ServiceConfig};

pub(crate) const CHAT_HISTORY_SCOPE_FILE_NAME: &str = ".chat_history_scope.json";

const CHAT_HISTORY_SCOPE_SIGNING_SECRET_ENV: &str = "CHAT_HISTORY_SCOPE_SIGNING_SECRET";
const CHAT_HISTORY_PROXY_BASE_URL_ENV: &str = "CHAT_HISTORY_PROXY_BASE_URL";
const DOWHIZ_API_URL_ENV: &str = "DOWHIZ_API_URL";
const FRONTEND_URL_ENV: &str = "FRONTEND_URL";
const POSTMARK_INBOUND_HOOK_URL_ENV: &str = "POSTMARK_INBOUND_HOOK_URL";
const SERVICE_URL_ENV: &str = "SERVICE_URL";
const RUN_TASK_EXECUTION_BACKEND_ENV: &str = "RUN_TASK_EXECUTION_BACKEND";
const CHAT_HISTORY_SCOPE_TTL_MINUTES_ENV: &str = "CHAT_HISTORY_SCOPE_TTL_MINUTES";
const CHAT_HISTORY_SLACK_MAX_HISTORY_PAGES_ENV: &str = "CHAT_HISTORY_SLACK_MAX_HISTORY_PAGES";
const CHAT_HISTORY_SLACK_MAX_THREAD_PAGES_ENV: &str = "CHAT_HISTORY_SLACK_MAX_THREAD_PAGES";
const CHAT_HISTORY_SLACK_MAX_CHANNELS_ENV: &str = "CHAT_HISTORY_SLACK_MAX_CHANNELS";
const CHAT_HISTORY_DISCORD_MAX_HISTORY_PAGES_PER_CHANNEL_ENV: &str =
    "CHAT_HISTORY_DISCORD_MAX_HISTORY_PAGES_PER_CHANNEL";
const CHAT_HISTORY_DISCORD_MAX_CHANNELS_ENV: &str = "CHAT_HISTORY_DISCORD_MAX_CHANNELS";
const CHAT_HISTORY_DISCORD_RATE_LIMIT_RETRIES_ENV: &str = "CHAT_HISTORY_DISCORD_RATE_LIMIT_RETRIES";
const CHAT_HISTORY_DISCORD_RATE_LIMIT_FALLBACK_MS_ENV: &str =
    "CHAT_HISTORY_DISCORD_RATE_LIMIT_FALLBACK_MS";

const DEFAULT_SCOPE_TTL_MINUTES: i64 = 12 * 60;
const DEFAULT_SEARCH_RESULT_LIMIT: usize = 20;
const MAX_SEARCH_RESULT_LIMIT: usize = 100;
const MAX_QUERY_CHARS: usize = 240;
const DEFAULT_SLACK_MAX_HISTORY_PAGES: usize = 100;
const DEFAULT_SLACK_MAX_THREAD_PAGES: usize = 20;
const DEFAULT_SLACK_MAX_CHANNELS: usize = 200;
const DISCORD_OFFICIAL_SEARCH_PAGE_LIMIT: usize = 25;
const DEFAULT_DISCORD_MAX_HISTORY_PAGES_PER_CHANNEL: usize = 40;
const DEFAULT_DISCORD_MAX_CHANNELS: usize = 200;
const DEFAULT_DISCORD_RATE_LIMIT_RETRIES: usize = 3;
const DEFAULT_DISCORD_RATE_LIMIT_FALLBACK_MS: u64 = 1_500;
const DISCORD_TEXT_CHANNEL_TYPES: &[u8] = &[0, 5, 10, 11, 12, 15];
const AZURE_ACI_EXECUTION_BACKEND: &str = "azure_aci";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ChatHistoryPlatform {
    Slack,
    Discord,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ChatHistoryScopeMode {
    CurrentConversation,
    CurrentWorkspace,
    CurrentGuild,
    DirectMessage,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ChatHistorySearchEngine {
    SlackConversationsApi,
    DiscordOfficialSearch,
    DiscordChannelScan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackScopeGrant {
    team_id: String,
    channel_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscordScopeGrant {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    guild_id: Option<u64>,
    channel_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatHistoryScopeGrant {
    version: u8,
    platform: ChatHistoryPlatform,
    scope_mode: ChatHistoryScopeMode,
    employee_id: String,
    iat: usize,
    exp: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slack: Option<SlackScopeGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discord: Option<DiscordScopeGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceChatHistoryScope {
    version: u8,
    platform: ChatHistoryPlatform,
    scope_mode: ChatHistoryScopeMode,
    description: String,
    generated_at: String,
    expires_at: String,
    search_endpoint: String,
    token: String,
    #[serde(flatten)]
    scope: WorkspaceChatHistoryScopeDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WorkspaceChatHistoryScopeDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    employee_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    team_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    guild_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChatHistorySearchRequest {
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) channel_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChatHistorySearchResponse {
    platform: ChatHistoryPlatform,
    scope_mode: ChatHistoryScopeMode,
    query: String,
    limit: usize,
    engine: ChatHistorySearchEngine,
    #[serde(default, skip_serializing_if = "is_false")]
    fallback_used: bool,
    searched_channels: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    results: Vec<ChatHistoryMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChatHistoryMatch {
    channel_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel_name: Option<String>,
    message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    author_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author_name: Option<String>,
    text: String,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChatHistoryErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ChatHistoryRequestError {
    status: StatusCode,
    message: String,
}

impl ChatHistoryRequestError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SlackHistoryResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    needed: Option<String>,
    #[serde(default)]
    provided: Option<String>,
    #[serde(default)]
    messages: Vec<SlackHistoryMessage>,
    #[serde(default)]
    response_metadata: Option<SlackResponseMetadata>,
}

#[derive(Debug, Deserialize)]
struct SlackConversationListResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    needed: Option<String>,
    #[serde(default)]
    provided: Option<String>,
    #[serde(default)]
    channels: Vec<SlackConversation>,
    #[serde(default)]
    response_metadata: Option<SlackResponseMetadata>,
}

fn format_slack_api_error(
    error: Option<&str>,
    needed: Option<&str>,
    provided: Option<&str>,
) -> String {
    let error = error
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown_error");
    let needed = needed.map(str::trim).filter(|value| !value.is_empty());
    let provided = provided.map(str::trim).filter(|value| !value.is_empty());

    match (needed, provided) {
        (Some(needed), Some(provided)) => {
            format!("{error} (needed scope: {needed}; provided: {provided})")
        }
        (Some(needed), None) => format!("{error} (needed scope: {needed})"),
        (None, Some(provided)) => format!("{error} (provided: {provided})"),
        (None, None) => error.to_string(),
    }
}

#[derive(Debug, Deserialize, Clone)]
struct SlackHistoryMessage {
    #[serde(default)]
    text: String,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    username: Option<String>,
    ts: String,
    #[serde(default)]
    thread_ts: Option<String>,
    #[serde(default)]
    reply_count: Option<u64>,
    #[serde(default)]
    files: Vec<SlackHistoryFile>,
}

#[derive(Debug, Deserialize, Clone)]
struct SlackConversation {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    is_private: bool,
    #[serde(default)]
    is_im: bool,
    #[serde(default)]
    is_mpim: bool,
}

impl SlackConversation {
    fn synthetic(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            is_private: false,
            is_im: false,
            is_mpim: false,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct SlackHistoryFile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackResponseMetadata {
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct DiscordGuildChannel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type")]
    kind: u8,
}

#[derive(Debug, Deserialize)]
struct DiscordActiveThreadsResponse {
    #[serde(default)]
    threads: Vec<DiscordGuildChannel>,
}

#[derive(Debug, Deserialize)]
struct DiscordHistoryMessage {
    id: String,
    #[serde(default)]
    content: String,
    timestamp: String,
    author: DiscordAuthor,
    #[serde(default)]
    attachments: Vec<DiscordAttachment>,
}

#[derive(Debug, Deserialize)]
struct DiscordAuthor {
    id: String,
    username: String,
    #[serde(default)]
    global_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscordAttachment {
    filename: String,
}

#[derive(Debug, Deserialize)]
struct DiscordOfficialSearchResponse {
    #[serde(default)]
    messages: Vec<Vec<DiscordOfficialSearchMessage>>,
    #[serde(default)]
    doing_deep_historical_index: bool,
    #[serde(default)]
    total_results: usize,
}

#[derive(Debug, Deserialize)]
struct DiscordOfficialSearchMessage {
    id: String,
    channel_id: String,
    #[serde(default)]
    content: String,
    timestamp: String,
    author: DiscordAuthor,
    #[serde(default)]
    attachments: Vec<DiscordAttachment>,
    #[serde(default)]
    thread: Option<DiscordThreadReference>,
    #[serde(default)]
    hit: bool,
}

#[derive(Debug, Deserialize)]
struct DiscordThreadReference {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DiscordSearchIndexNotReadyPayload {
    #[serde(default)]
    documents_indexed: Option<usize>,
    #[serde(default)]
    retry_after: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct DiscordRateLimitPayload {
    #[serde(default)]
    retry_after: Option<f64>,
}

#[derive(Debug)]
struct DiscordSearchOutcome {
    engine: ChatHistorySearchEngine,
    fallback_used: bool,
    searched_channels: usize,
    warnings: Vec<String>,
    results: Vec<ChatHistoryMatch>,
}

#[derive(Debug)]
struct SlackSearchOutcome {
    searched_channels: usize,
    warnings: Vec<String>,
    results: Vec<ChatHistoryMatch>,
}

pub(crate) fn write_slack_chat_history_scope_file(
    config: &ServiceConfig,
    workspace: &Path,
    message: &crate::channel::InboundMessage,
) -> Result<(), BoxError> {
    let team_id = message
        .metadata
        .slack_team_id
        .clone()
        .ok_or("missing slack team id for chat history scope")?;
    let channel_id = message
        .metadata
        .slack_channel_id
        .clone()
        .ok_or("missing slack channel id for chat history scope")?;
    let claims = build_scope_grant(
        config,
        ChatHistoryPlatform::Slack,
        ChatHistoryScopeMode::CurrentWorkspace,
        Some(SlackScopeGrant {
            team_id: team_id.clone(),
            channel_id: channel_id.clone(),
            thread_id: Some(message.thread_id.clone()),
        }),
        None,
    )?;
    let description = format!(
        "History search is limited to the current Slack workspace/team ({team_id}). It can scan readable Slack conversations in this workspace, including the origin conversation ({channel_id}). Cross-workspace access is blocked.",
    );
    let scope = WorkspaceChatHistoryScope {
        version: claims.version,
        platform: claims.platform,
        scope_mode: claims.scope_mode,
        description,
        generated_at: format_unix_ts(claims.iat)?,
        expires_at: format_unix_ts(claims.exp)?,
        search_endpoint: format!(
            "{}/internal/chat-history/search",
            chat_history_base_url(config)
        ),
        token: encode_scope_grant(config, &claims)?,
        scope: WorkspaceChatHistoryScopeDetails {
            employee_id: Some(config.employee_id.clone()),
            team_id: Some(team_id),
            channel_id: Some(channel_id),
            thread_id: Some(message.thread_id.clone()),
            ..WorkspaceChatHistoryScopeDetails::default()
        },
    };
    std::fs::write(
        workspace.join(CHAT_HISTORY_SCOPE_FILE_NAME),
        format!("{}\n", serde_json::to_string_pretty(&scope)?),
    )?;
    Ok(())
}

pub(crate) fn write_discord_chat_history_scope_file(
    config: &ServiceConfig,
    workspace: &Path,
    message: &crate::channel::InboundMessage,
) -> Result<(), BoxError> {
    let channel_id = message
        .metadata
        .discord_channel_id
        .ok_or("missing discord channel id for chat history scope")?;
    let guild_id = message.metadata.discord_guild_id;
    let scope_mode = if guild_id.is_some() {
        ChatHistoryScopeMode::CurrentGuild
    } else {
        ChatHistoryScopeMode::DirectMessage
    };
    let claims = build_scope_grant(
        config,
        ChatHistoryPlatform::Discord,
        scope_mode,
        None,
        Some(DiscordScopeGrant {
            guild_id,
            channel_id,
            thread_id: Some(message.thread_id.clone()),
        }),
    )?;
    let description = match guild_id {
        Some(guild_id) => format!(
            "History search is limited to the current Discord server ({guild_id}). Cross-server access is blocked."
        ),
        None => "History search is limited to the current Discord DM conversation.".to_string(),
    };
    let scope = WorkspaceChatHistoryScope {
        version: claims.version,
        platform: claims.platform,
        scope_mode: claims.scope_mode,
        description,
        generated_at: format_unix_ts(claims.iat)?,
        expires_at: format_unix_ts(claims.exp)?,
        search_endpoint: format!(
            "{}/internal/chat-history/search",
            chat_history_base_url(config)
        ),
        token: encode_scope_grant(config, &claims)?,
        scope: WorkspaceChatHistoryScopeDetails {
            employee_id: Some(config.employee_id.clone()),
            guild_id,
            channel_id: Some(channel_id.to_string()),
            thread_id: Some(message.thread_id.clone()),
            ..WorkspaceChatHistoryScopeDetails::default()
        },
    };
    std::fs::write(
        workspace.join(CHAT_HISTORY_SCOPE_FILE_NAME),
        format!("{}\n", serde_json::to_string_pretty(&scope)?),
    )?;
    Ok(())
}

pub(crate) async fn search_chat_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChatHistorySearchRequest>,
) -> impl IntoResponse {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string());

    let state_for_blocking = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let token = extract_bearer_token(authorization.as_deref())?;
        let claims = decode_scope_grant(&state_for_blocking.config, &token)?;
        if claims.employee_id != state_for_blocking.config.employee_id {
            return Err(ChatHistoryRequestError::new(
                StatusCode::FORBIDDEN,
                "chat history grant does not belong to this worker",
            ));
        }
        execute_chat_history_search(
            &state_for_blocking.config,
            &state_for_blocking.slack_store,
            claims,
            request,
        )
    })
    .await;

    match result {
        Ok(Ok(response)) => (StatusCode::OK, Json(response)).into_response(),
        Ok(Err(err)) => (
            err.status,
            Json(ChatHistoryErrorResponse { error: err.message }),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ChatHistoryErrorResponse {
                error: format!("chat history task join error: {err}"),
            }),
        )
            .into_response(),
    }
}

fn execute_chat_history_search(
    config: &ServiceConfig,
    slack_store: &SlackStore,
    claims: ChatHistoryScopeGrant,
    request: ChatHistorySearchRequest,
) -> Result<ChatHistorySearchResponse, ChatHistoryRequestError> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err(ChatHistoryRequestError::new(
            StatusCode::BAD_REQUEST,
            "query is required",
        ));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(ChatHistoryRequestError::new(
            StatusCode::BAD_REQUEST,
            format!("query is too long (max {MAX_QUERY_CHARS} chars)"),
        ));
    }
    let limit = request
        .limit
        .unwrap_or(DEFAULT_SEARCH_RESULT_LIMIT)
        .clamp(1, MAX_SEARCH_RESULT_LIMIT);

    match claims.platform {
        ChatHistoryPlatform::Slack => {
            let slack = claims.slack.ok_or_else(|| {
                ChatHistoryRequestError::new(
                    StatusCode::FORBIDDEN,
                    "slack chat history grant is missing slack scope",
                )
            })?;
            let installation = slack_store
                .get_installation_or_env(&slack.team_id)
                .map_err(|err| {
                    ChatHistoryRequestError::new(
                        StatusCode::BAD_GATEWAY,
                        format!("failed to resolve Slack installation: {err}"),
                    )
                })?;
            let outcome = search_slack_history(
                &installation.bot_token,
                &slack,
                request.channel_id.as_deref(),
                query,
                limit,
            )?;
            Ok(ChatHistorySearchResponse {
                platform: ChatHistoryPlatform::Slack,
                scope_mode: claims.scope_mode,
                query: query.to_string(),
                limit,
                engine: ChatHistorySearchEngine::SlackConversationsApi,
                fallback_used: false,
                searched_channels: outcome.searched_channels,
                warnings: outcome.warnings,
                results: outcome.results,
            })
        }
        ChatHistoryPlatform::Discord => {
            let discord = claims.discord.ok_or_else(|| {
                ChatHistoryRequestError::new(
                    StatusCode::FORBIDDEN,
                    "discord chat history grant is missing discord scope",
                )
            })?;
            let token = resolve_discord_bot_token(config).ok_or_else(|| {
                ChatHistoryRequestError::new(
                    StatusCode::BAD_GATEWAY,
                    "discord bot token is not configured",
                )
            })?;
            let outcome = search_discord_history(
                &token,
                &discord,
                request.channel_id.as_deref(),
                query,
                limit,
            )
            .map_err(|err| {
                ChatHistoryRequestError::new(
                    StatusCode::BAD_GATEWAY,
                    format!("Discord history search failed: {err}"),
                )
            })?;
            Ok(ChatHistorySearchResponse {
                platform: ChatHistoryPlatform::Discord,
                scope_mode: claims.scope_mode,
                query: query.to_string(),
                limit,
                engine: outcome.engine,
                fallback_used: outcome.fallback_used,
                searched_channels: outcome.searched_channels,
                warnings: outcome.warnings,
                results: outcome.results,
            })
        }
    }
}

fn build_scope_grant(
    config: &ServiceConfig,
    platform: ChatHistoryPlatform,
    scope_mode: ChatHistoryScopeMode,
    slack: Option<SlackScopeGrant>,
    discord: Option<DiscordScopeGrant>,
) -> Result<ChatHistoryScopeGrant, BoxError> {
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::minutes(scope_ttl_minutes());
    Ok(ChatHistoryScopeGrant {
        version: 1,
        platform,
        scope_mode,
        employee_id: config.employee_id.clone(),
        iat: issued_at.timestamp().max(0) as usize,
        exp: expires_at.timestamp().max(0) as usize,
        slack,
        discord,
    })
}

fn encode_scope_grant(
    config: &ServiceConfig,
    claims: &ChatHistoryScopeGrant,
) -> Result<String, BoxError> {
    let secret = chat_history_signing_secret(config)?;
    Ok(jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?)
}

fn decode_scope_grant(
    config: &ServiceConfig,
    token: &str,
) -> Result<ChatHistoryScopeGrant, ChatHistoryRequestError> {
    let secret = chat_history_signing_secret(config).map_err(|err| {
        ChatHistoryRequestError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("missing chat history signing secret: {err}"),
        )
    })?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.required_spec_claims.insert("exp".to_string());
    validation.required_spec_claims.insert("iat".to_string());
    let decoded = jsonwebtoken::decode::<ChatHistoryScopeGrant>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|err| {
        ChatHistoryRequestError::new(
            StatusCode::UNAUTHORIZED,
            format!("invalid chat history grant: {err}"),
        )
    })?;
    Ok(decoded.claims)
}

fn extract_bearer_token(value: Option<&str>) -> Result<String, ChatHistoryRequestError> {
    let header = value.ok_or_else(|| {
        ChatHistoryRequestError::new(StatusCode::UNAUTHORIZED, "missing Authorization header")
    })?;
    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ChatHistoryRequestError::new(
                StatusCode::UNAUTHORIZED,
                "Authorization header must be a Bearer token",
            )
        })?;
    Ok(token.to_string())
}

fn chat_history_signing_secret(config: &ServiceConfig) -> Result<String, BoxError> {
    if let Some(secret) = env_trimmed(CHAT_HISTORY_SCOPE_SIGNING_SECRET_ENV) {
        return Ok(secret);
    }
    if let Some(secret) = env_trimmed("SLACK_SIGNING_SECRET") {
        return Ok(secret);
    }
    if let Some(secret) = config
        .slack_client_secret
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(secret);
    }
    let employee_prefix = config.employee_profile.id.to_uppercase().replace('-', "_");
    if let Some(secret) = env_trimmed(&format!("{employee_prefix}_DISCORD_BOT_TOKEN")) {
        return Ok(secret);
    }
    if let Some(secret) = env_trimmed(&format!("{employee_prefix}_SLACK_BOT_TOKEN")) {
        return Ok(secret);
    }
    if let Some(secret) = config
        .discord_bot_token
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(secret);
    }
    if let Some(secret) = config
        .slack_bot_token
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(secret);
    }
    Err("CHAT_HISTORY_SCOPE_SIGNING_SECRET (or another platform secret fallback) is not set".into())
}

fn chat_history_base_url(config: &ServiceConfig) -> String {
    if let Some(url) = env_trimmed(CHAT_HISTORY_PROXY_BASE_URL_ENV) {
        return url.trim_end_matches('/').to_string();
    }
    if let Some(url) = env_trimmed(DOWHIZ_API_URL_ENV) {
        return url.trim_end_matches('/').to_string();
    }
    if let Some(url) = env_trimmed(SERVICE_URL_ENV) {
        return url.trim_end_matches('/').to_string();
    }
    if chat_history_requires_public_proxy() {
        if let Some(url) = env_trimmed(POSTMARK_INBOUND_HOOK_URL_ENV)
            .and_then(|value| derive_public_service_base_url(&value))
        {
            return url;
        }
        if let Some(url) =
            env_trimmed(FRONTEND_URL_ENV).and_then(|value| derive_public_service_base_url(&value))
        {
            return url;
        }
    }
    let host = normalize_base_host(&config.host);
    format!("http://{host}:{}", config.port)
}

fn chat_history_requires_public_proxy() -> bool {
    env_trimmed(RUN_TASK_EXECUTION_BACKEND_ENV)
        .map(|value| value.eq_ignore_ascii_case(AZURE_ACI_EXECUTION_BACKEND))
        .unwrap_or(false)
}

fn derive_public_service_base_url(candidate: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(candidate).ok()?;
    let raw_path = url.path().trim_end_matches('/');
    let normalized_path = if let Some(prefix) = raw_path.strip_suffix("/postmark/inbound") {
        let prefix = prefix.trim_end_matches('/');
        if prefix.is_empty() {
            "/service".to_string()
        } else {
            format!("{prefix}/service")
        }
    } else if raw_path.is_empty() || raw_path == "/" {
        "/service".to_string()
    } else if raw_path.ends_with("/service") {
        raw_path.to_string()
    } else {
        format!("{}/service", raw_path)
    };
    url.set_path(&normalized_path);
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string().trim_end_matches('/').to_string())
}

fn normalize_base_host(host: &str) -> String {
    let trimmed = host.trim();
    let normalized = match trimmed {
        "" | "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
        other => other,
    };
    if normalized.contains(':') && !normalized.starts_with('[') && !normalized.ends_with(']') {
        format!("[{normalized}]")
    } else {
        normalized.to_string()
    }
}

fn scope_ttl_minutes() -> i64 {
    env_trimmed(CHAT_HISTORY_SCOPE_TTL_MINUTES_ENV)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SCOPE_TTL_MINUTES)
}

fn env_trimmed(key: &str) -> Option<String> {
    env::var(key).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn format_unix_ts(seconds: usize) -> Result<String, BoxError> {
    let Some(dt) = DateTime::<Utc>::from_timestamp(seconds as i64, 0) else {
        return Err(format!("invalid unix timestamp: {seconds}").into());
    };
    Ok(dt.to_rfc3339())
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn history_client() -> Result<Client, BoxError> {
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

fn slack_max_history_pages() -> usize {
    env_trimmed(CHAT_HISTORY_SLACK_MAX_HISTORY_PAGES_ENV)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SLACK_MAX_HISTORY_PAGES)
}

fn slack_max_thread_pages() -> usize {
    env_trimmed(CHAT_HISTORY_SLACK_MAX_THREAD_PAGES_ENV)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SLACK_MAX_THREAD_PAGES)
}

fn slack_max_channels() -> usize {
    env_trimmed(CHAT_HISTORY_SLACK_MAX_CHANNELS_ENV)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SLACK_MAX_CHANNELS)
}

fn discord_max_history_pages_per_channel() -> usize {
    env_trimmed(CHAT_HISTORY_DISCORD_MAX_HISTORY_PAGES_PER_CHANNEL_ENV)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DISCORD_MAX_HISTORY_PAGES_PER_CHANNEL)
}

fn discord_max_channels() -> usize {
    env_trimmed(CHAT_HISTORY_DISCORD_MAX_CHANNELS_ENV)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DISCORD_MAX_CHANNELS)
}

fn discord_rate_limit_retries() -> usize {
    env_trimmed(CHAT_HISTORY_DISCORD_RATE_LIMIT_RETRIES_ENV)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_DISCORD_RATE_LIMIT_RETRIES)
}

fn discord_rate_limit_fallback_delay() -> StdDuration {
    let millis = env_trimmed(CHAT_HISTORY_DISCORD_RATE_LIMIT_FALLBACK_MS_ENV)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DISCORD_RATE_LIMIT_FALLBACK_MS);
    StdDuration::from_millis(millis)
}

fn push_optional_warning(warnings: &mut Option<&mut Vec<String>>, message: impl Into<String>) {
    if let Some(warnings) = warnings.as_mut() {
        warnings.push(message.into());
    }
}

fn slack_partial_or_error<T>(
    warnings: &mut Option<&mut Vec<String>>,
    partial: T,
    warning: String,
    error: String,
) -> Result<T, BoxError> {
    if warnings.is_some() {
        push_optional_warning(warnings, warning);
        Ok(partial)
    } else {
        Err(error.into())
    }
}

fn slack_conversation_name(conversation: &SlackConversation) -> Option<String> {
    let explicit_name = conversation
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if explicit_name.is_some() {
        return explicit_name;
    }
    if conversation.is_im {
        return Some(format!("dm-{}", conversation.id));
    }
    if conversation.is_mpim {
        return Some(format!("mpim-{}", conversation.id));
    }
    if conversation.is_private {
        return Some(format!("private-{}", conversation.id));
    }
    None
}

fn slack_conversation_label(channel_id: &str, channel_name: Option<&str>) -> String {
    channel_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value} ({channel_id})"))
        .unwrap_or_else(|| channel_id.to_string())
}

fn search_slack_history(
    bot_token: &str,
    scope: &SlackScopeGrant,
    requested_channel_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<SlackSearchOutcome, ChatHistoryRequestError> {
    let api_base =
        env_trimmed("SLACK_API_BASE_URL").unwrap_or_else(|| "https://slack.com/api".to_string());
    let client = history_client().map_err(|err| {
        ChatHistoryRequestError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to build Slack history client: {err}"),
        )
    })?;
    let query_norm = query.to_ascii_lowercase();
    let mut warnings = Vec::new();
    let requested_channel_id = requested_channel_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let channels = match resolve_slack_search_channels(
        &client,
        api_base.as_str(),
        bot_token,
        scope,
        requested_channel_id.as_deref(),
        &mut warnings,
    ) {
        Ok(channels) => channels,
        Err(err) if err.status == StatusCode::BAD_GATEWAY => {
            let fallback_channel_id = requested_channel_id
                .clone()
                .unwrap_or_else(|| scope.channel_id.clone());
            let fallback_label = if fallback_channel_id == scope.channel_id {
                "the origin Slack conversation"
            } else {
                "the requested Slack conversation"
            };
            warnings.push(format!(
                "{} Falling back to {fallback_label} ({fallback_channel_id}) only.",
                err.message
            ));
            vec![SlackConversation::synthetic(fallback_channel_id)]
        }
        Err(err) => return Err(err),
    };
    let searched_channels = channels.len();
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    for channel in channels {
        let channel_name = slack_conversation_name(&channel);
        let mut warning_sink = Some(&mut warnings);
        let channel_matches = search_slack_channel_history_with_client(
            &client,
            api_base.as_str(),
            bot_token,
            &channel.id,
            channel_name.as_deref(),
            &query_norm,
            limit,
            &mut warning_sink,
        )
        .map_err(|err| {
            ChatHistoryRequestError::new(
                StatusCode::BAD_GATEWAY,
                format!("Slack history search failed: {err}"),
            )
        })?;
        for entry in channel_matches {
            let key = format!("{}:{}", entry.channel_id, entry.message_id);
            if seen.insert(key) {
                results.push(entry);
            }
        }
    }

    sort_matches_desc(&mut results);
    results.truncate(limit);
    Ok(SlackSearchOutcome {
        searched_channels,
        warnings,
        results,
    })
}

fn resolve_slack_search_channels(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    scope: &SlackScopeGrant,
    requested_channel_id: Option<&str>,
    warnings: &mut Vec<String>,
) -> Result<Vec<SlackConversation>, ChatHistoryRequestError> {
    let requested = requested_channel_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut channels = fetch_slack_searchable_conversations(
        client,
        api_base,
        bot_token,
        &scope.team_id,
        requested.as_deref().or(Some(scope.channel_id.as_str())),
    )
    .map_err(|err| {
        ChatHistoryRequestError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to enumerate readable Slack conversations: {err}"),
        )
    })?;

    let origin = channels
        .iter()
        .position(|channel| channel.id == scope.channel_id)
        .map(|index| channels.remove(index))
        .unwrap_or_else(|| SlackConversation::synthetic(scope.channel_id.clone()));

    if let Some(requested) = requested {
        if requested == origin.id {
            return Ok(vec![origin]);
        }
        let selected = channels
            .into_iter()
            .find(|channel| channel.id == requested)
            .ok_or_else(|| {
                ChatHistoryRequestError::new(
                    StatusCode::FORBIDDEN,
                    format!(
                        "requested Slack conversation {requested} is outside the readable workspace scope"
                    ),
                )
            })?;
        return Ok(vec![selected]);
    }

    channels.insert(0, origin);
    if channels.len() > slack_max_channels() {
        warnings.push(format!(
            "Slack workspace has {} readable conversations; only the first {} were scanned, with the origin conversation kept in scope.",
            channels.len(),
            slack_max_channels()
        ));
        channels.truncate(slack_max_channels());
    }
    Ok(channels)
}

fn fetch_slack_searchable_conversations(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    team_id: &str,
    priority_channel_id: Option<&str>,
) -> Result<Vec<SlackConversation>, BoxError> {
    let mut channels = Vec::new();
    let mut cursor: Option<String> = None;
    let types = "public_channel,private_channel,mpim,im";

    loop {
        let mut request = client
            .get(format!(
                "{}/conversations.list",
                api_base.trim_end_matches('/')
            ))
            .bearer_auth(bot_token)
            .query(&[("limit", "200"), ("types", types)]);
        if !team_id.trim().is_empty() {
            request = request.query(&[("team_id", team_id)]);
        }
        if let Some(cursor_value) = cursor.as_deref() {
            request = request.query(&[("cursor", cursor_value)]);
        }

        let response = request.send()?;
        if !response.status().is_success() {
            return Err(format!("slack conversations.list returned {}", response.status()).into());
        }
        let payload: SlackConversationListResponse = response.json()?;
        if !payload.ok {
            let error = format_slack_api_error(
                payload.error.as_deref(),
                payload.needed.as_deref(),
                payload.provided.as_deref(),
            );
            return Err(format!("slack conversations.list returned error {}", error).into());
        }
        channels.extend(payload.channels);
        cursor = payload
            .response_metadata
            .and_then(|meta| meta.next_cursor)
            .filter(|value| !value.trim().is_empty());

        let reached_priority = priority_channel_id
            .map(|priority| channels.iter().any(|channel| channel.id == priority))
            .unwrap_or(true);
        if cursor.is_none() || (channels.len() >= slack_max_channels() && reached_priority) {
            break;
        }
    }

    Ok(channels)
}

#[cfg(test)]
fn search_slack_channel_history(
    bot_token: &str,
    channel_id: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<ChatHistoryMatch>, BoxError> {
    let api_base =
        env_trimmed("SLACK_API_BASE_URL").unwrap_or_else(|| "https://slack.com/api".to_string());
    let client = history_client()?;
    let query_norm = query.to_ascii_lowercase();
    let mut warning_sink: Option<&mut Vec<String>> = None;
    let mut results = search_slack_channel_history_with_client(
        &client,
        api_base.as_str(),
        bot_token,
        channel_id,
        None,
        &query_norm,
        limit,
        &mut warning_sink,
    )?;
    results.truncate(limit);
    Ok(results)
}

fn search_slack_channel_history_with_client(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    channel_id: &str,
    channel_name: Option<&str>,
    query_norm: &str,
    limit: usize,
    warnings: &mut Option<&mut Vec<String>>,
) -> Result<Vec<ChatHistoryMatch>, BoxError> {
    let label = slack_conversation_label(channel_id, channel_name);
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor: Option<String> = None;

    for _ in 0..slack_max_history_pages() {
        let mut request = client
            .get(format!(
                "{}/conversations.history",
                api_base.trim_end_matches('/')
            ))
            .bearer_auth(bot_token)
            .query(&[("channel", channel_id), ("limit", "200")]);
        if let Some(cursor_value) = cursor.as_deref() {
            request = request.query(&[("cursor", cursor_value)]);
        }
        let response = match request.send() {
            Ok(response) => response,
            Err(err) => {
                return slack_partial_or_error(
                    warnings,
                    results,
                    format!(
                        "Slack history scan for {label} stopped early because the request failed ({err})."
                    ),
                    format!("slack conversations.history request failed: {err}"),
                );
            }
        };
        if !response.status().is_success() {
            return slack_partial_or_error(
                warnings,
                results,
                format!(
                    "Slack history scan for {label} stopped early because Slack returned {}.",
                    response.status()
                ),
                format!("slack conversations.history returned {}", response.status()),
            );
        }
        let payload: SlackHistoryResponse = match response.json() {
            Ok(payload) => payload,
            Err(err) => {
                return slack_partial_or_error(
                    warnings,
                    results,
                    format!(
                        "Slack history scan for {label} stopped early because the response payload was unreadable ({err})."
                    ),
                    format!("slack conversations.history returned an unreadable payload: {err}"),
                );
            }
        };
        if !payload.ok {
            let error = format_slack_api_error(
                payload.error.as_deref(),
                payload.needed.as_deref(),
                payload.provided.as_deref(),
            );
            return slack_partial_or_error(
                warnings,
                results,
                format!(
                    "Slack history scan for {label} stopped early because Slack returned error {error}."
                ),
                format!("slack conversations.history returned error {error}"),
            );
        }

        for message in payload.messages {
            maybe_push_slack_match(
                &mut results,
                &mut seen,
                channel_id,
                channel_name,
                None,
                &message,
                query_norm,
            );
            if message.reply_count.unwrap_or(0) > 0 {
                let thread_ts = message.thread_ts.as_deref().unwrap_or(&message.ts);
                let replies = fetch_slack_thread_replies(
                    client, api_base, bot_token, channel_id, thread_ts, &label, warnings,
                )?;
                for reply in replies {
                    maybe_push_slack_match(
                        &mut results,
                        &mut seen,
                        channel_id,
                        channel_name,
                        Some(thread_ts),
                        &reply,
                        query_norm,
                    );
                }
            }
        }

        sort_matches_desc(&mut results);
        if results.len() >= limit {
            break;
        }

        cursor = payload
            .response_metadata
            .and_then(|meta| meta.next_cursor)
            .filter(|value| !value.trim().is_empty());
        if cursor.is_none() {
            break;
        }
    }

    Ok(results)
}

fn fetch_slack_thread_replies(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    channel_id: &str,
    thread_ts: &str,
    label: &str,
    warnings: &mut Option<&mut Vec<String>>,
) -> Result<Vec<SlackHistoryMessage>, BoxError> {
    let mut replies = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..slack_max_thread_pages() {
        let mut request = client
            .get(format!(
                "{}/conversations.replies",
                api_base.trim_end_matches('/')
            ))
            .bearer_auth(bot_token)
            .query(&[("channel", channel_id), ("ts", thread_ts), ("limit", "200")]);
        if let Some(cursor_value) = cursor.as_deref() {
            request = request.query(&[("cursor", cursor_value)]);
        }
        let response = match request.send() {
            Ok(response) => response,
            Err(err) => {
                return slack_partial_or_error(
                    warnings,
                    replies,
                    format!(
                        "Slack thread reply scan for {label} in thread {thread_ts} stopped early because the request failed ({err})."
                    ),
                    format!("slack conversations.replies request failed: {err}"),
                );
            }
        };
        if !response.status().is_success() {
            return slack_partial_or_error(
                warnings,
                replies,
                format!(
                    "Slack thread reply scan for {label} in thread {thread_ts} stopped early because Slack returned {}.",
                    response.status()
                ),
                format!("slack conversations.replies returned {}", response.status()),
            );
        }
        let payload: SlackHistoryResponse = match response.json() {
            Ok(payload) => payload,
            Err(err) => {
                return slack_partial_or_error(
                    warnings,
                    replies,
                    format!(
                        "Slack thread reply scan for {label} in thread {thread_ts} stopped early because the response payload was unreadable ({err})."
                    ),
                    format!("slack conversations.replies returned an unreadable payload: {err}"),
                );
            }
        };
        if !payload.ok {
            let error = format_slack_api_error(
                payload.error.as_deref(),
                payload.needed.as_deref(),
                payload.provided.as_deref(),
            );
            return slack_partial_or_error(
                warnings,
                replies,
                format!(
                    "Slack thread reply scan for {label} in thread {thread_ts} stopped early because Slack returned error {error}."
                ),
                format!("slack conversations.replies returned error {error}"),
            );
        }
        replies.extend(
            payload
                .messages
                .into_iter()
                .filter(|message| message.ts != thread_ts),
        );
        cursor = payload
            .response_metadata
            .and_then(|meta| meta.next_cursor)
            .filter(|value| !value.trim().is_empty());
        if cursor.is_none() {
            break;
        }
    }
    Ok(replies)
}

fn maybe_push_slack_match(
    results: &mut Vec<ChatHistoryMatch>,
    seen: &mut HashSet<String>,
    channel_id: &str,
    channel_name: Option<&str>,
    thread_id: Option<&str>,
    message: &SlackHistoryMessage,
    query_norm: &str,
) {
    let text = slack_message_text(message);
    if !message_matches_query(&text, query_norm) {
        return;
    }
    let key = format!("{channel_id}:{}", message.ts);
    if !seen.insert(key) {
        return;
    }
    results.push(ChatHistoryMatch {
        channel_id: channel_id.to_string(),
        channel_name: channel_name.map(|value| value.to_string()),
        message_id: message.ts.clone(),
        thread_id: thread_id
            .map(|value| value.to_string())
            .or_else(|| message.thread_ts.clone()),
        timestamp: slack_ts_to_rfc3339(&message.ts).unwrap_or_else(|| message.ts.clone()),
        author_id: message.user.clone(),
        author_name: message.username.clone(),
        text: truncate_match_text(&text),
        source: if thread_id.is_some() {
            "slack_thread_reply".to_string()
        } else {
            "slack_channel_message".to_string()
        },
    });
}

fn slack_message_text(message: &SlackHistoryMessage) -> String {
    let text = message.text.trim();
    if !text.is_empty() {
        return text.to_string();
    }
    let attachment_names = message
        .files
        .iter()
        .filter_map(|file| file.name.as_deref().or(file.title.as_deref()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if attachment_names.is_empty() {
        "(empty Slack message)".to_string()
    } else {
        format!("[attachments: {}]", attachment_names.join(", "))
    }
}

fn slack_ts_to_rfc3339(ts: &str) -> Option<String> {
    let seconds_text = ts.split('.').next()?;
    let seconds = seconds_text.parse::<i64>().ok()?;
    DateTime::<Utc>::from_timestamp(seconds, 0).map(|dt| dt.to_rfc3339())
}

fn search_discord_history(
    bot_token: &str,
    scope: &DiscordScopeGrant,
    requested_channel_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<DiscordSearchOutcome, BoxError> {
    let api_base = env_trimmed("DISCORD_API_BASE_URL")
        .unwrap_or_else(|| "https://discord.com/api/v10".to_string());
    let client = history_client()?;
    let mut warnings = Vec::new();

    let validated_channel_id = validate_requested_discord_channel_in_scope(
        &client,
        api_base.as_str(),
        bot_token,
        scope,
        requested_channel_id,
        &mut warnings,
    )?;

    if scope.guild_id.is_some() {
        if let Some((results, searched_channels)) = try_search_discord_history_with_official_search(
            &client,
            api_base.as_str(),
            bot_token,
            scope,
            validated_channel_id.as_deref(),
            query,
            limit,
            &mut warnings,
        )? {
            return Ok(DiscordSearchOutcome {
                engine: ChatHistorySearchEngine::DiscordOfficialSearch,
                fallback_used: false,
                searched_channels,
                warnings,
                results,
            });
        }
    }

    let (results, searched_channels, scan_warnings) =
        search_discord_history_via_channel_scan_with_client(
            &client,
            api_base.as_str(),
            bot_token,
            scope,
            validated_channel_id.as_deref().or(requested_channel_id),
            query,
            limit,
        )?;
    let fallback_used = scope.guild_id.is_some();
    warnings.extend(scan_warnings);
    Ok(DiscordSearchOutcome {
        engine: ChatHistorySearchEngine::DiscordChannelScan,
        fallback_used,
        searched_channels,
        warnings,
        results,
    })
}

fn validate_requested_discord_channel_in_scope(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    scope: &DiscordScopeGrant,
    requested_channel_id: Option<&str>,
    warnings: &mut Vec<String>,
) -> Result<Option<String>, BoxError> {
    let requested = requested_channel_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let Some(requested) = requested else {
        return Ok(None);
    };

    resolve_discord_search_channels(
        client,
        api_base,
        bot_token,
        scope,
        Some(&requested),
        warnings,
    )?;
    Ok(Some(requested))
}

fn try_search_discord_history_with_official_search(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    scope: &DiscordScopeGrant,
    requested_channel_id: Option<&str>,
    query: &str,
    limit: usize,
    warnings: &mut Vec<String>,
) -> Result<Option<(Vec<ChatHistoryMatch>, usize)>, BoxError> {
    let Some(guild_id) = scope.guild_id else {
        return Ok(None);
    };

    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let mut offset = 0usize;
    let mut deep_index_warning_emitted = false;

    while results.len() < limit {
        let page_limit = (limit - results.len()).min(DISCORD_OFFICIAL_SEARCH_PAGE_LIMIT);
        if page_limit == 0 {
            break;
        }

        let mut attempts = 0usize;
        let response = loop {
            let mut query_params = vec![
                ("content", query.to_string()),
                ("limit", page_limit.to_string()),
                ("offset", offset.to_string()),
                ("sort_by", "timestamp".to_string()),
                ("sort_order", "desc".to_string()),
            ];
            if let Some(channel_id) = requested_channel_id {
                query_params.push(("channel_id", channel_id.to_string()));
            }
            let response = client
                .get(format!(
                    "{}/guilds/{guild_id}/messages/search",
                    api_base.trim_end_matches('/')
                ))
                .header("Authorization", format!("Bot {bot_token}"))
                .query(&query_params)
                .send();

            let response = match response {
                Ok(response) => response,
                Err(err) => {
                    warnings.push(format!(
                        "Discord official guild search failed to reach Discord ({err}); falling back to scoped channel scan."
                    ));
                    return Ok(None);
                }
            };

            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let headers = response.headers().clone();
                let body = response.text().unwrap_or_default();
                if attempts < discord_rate_limit_retries() {
                    attempts += 1;
                    std::thread::sleep(discord_rate_limit_delay(&headers, &body));
                    continue;
                }
                warnings.push(
                    "Discord official guild search hit repeated rate limits; falling back to scoped channel scan."
                        .to_string(),
                );
                return Ok(None);
            }

            break response;
        };

        match response.status() {
            reqwest::StatusCode::ACCEPTED => {
                let body = response.text().unwrap_or_default();
                if let Ok(payload) =
                    serde_json::from_str::<DiscordSearchIndexNotReadyPayload>(&body)
                {
                    warnings.push(format!(
                        "Discord official guild search index is not ready yet (documents_indexed={}, retry_after={}s); falling back to scoped channel scan.",
                        payload.documents_indexed.unwrap_or(0),
                        payload.retry_after.unwrap_or(0.0)
                    ));
                } else {
                    warnings.push(
                        "Discord official guild search index is not ready yet; falling back to scoped channel scan."
                            .to_string(),
                    );
                }
                return Ok(None);
            }
            status if status.is_success() => {
                let payload: DiscordOfficialSearchResponse = match response.json() {
                    Ok(payload) => payload,
                    Err(err) => {
                        warnings.push(format!(
                            "Discord official guild search returned an unreadable payload ({err}); falling back to scoped channel scan."
                        ));
                        return Ok(None);
                    }
                };
                if payload.doing_deep_historical_index && !deep_index_warning_emitted {
                    warnings.push(
                        "Discord official guild search is still deep-indexing older messages; the result set may be incomplete."
                            .to_string(),
                    );
                    deep_index_warning_emitted = true;
                }

                let page_count = payload.messages.len();
                for group in payload.messages {
                    for message in group.into_iter().filter(|message| message.hit) {
                        let key = format!("{}:{}", message.channel_id, message.id);
                        if !seen.insert(key) {
                            continue;
                        }
                        let text =
                            discord_search_message_text(&message.content, &message.attachments);
                        results.push(ChatHistoryMatch {
                            channel_id: message.channel_id,
                            channel_name: None,
                            message_id: message.id,
                            thread_id: message.thread.map(|thread| thread.id),
                            timestamp: message.timestamp,
                            author_id: Some(message.author.id),
                            author_name: Some(
                                message
                                    .author
                                    .global_name
                                    .unwrap_or(message.author.username),
                            ),
                            text: truncate_match_text(&text),
                            source: "discord_official_search".to_string(),
                        });
                    }
                }
                sort_matches_desc(&mut results);
                results.truncate(limit);

                if page_count < page_limit || offset + page_count >= payload.total_results {
                    break;
                }
                offset += page_count;
            }
            status => {
                warnings.push(format!(
                    "Discord official guild search returned {status}; falling back to scoped channel scan."
                ));
                return Ok(None);
            }
        }
    }

    let searched_channels = requested_channel_id.map(|_| 1).unwrap_or(0);
    Ok(Some((results, searched_channels)))
}

#[cfg(test)]
fn search_discord_history_via_channel_scan(
    bot_token: &str,
    scope: &DiscordScopeGrant,
    requested_channel_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<(Vec<ChatHistoryMatch>, usize, Vec<String>), BoxError> {
    let api_base = env_trimmed("DISCORD_API_BASE_URL")
        .unwrap_or_else(|| "https://discord.com/api/v10".to_string());
    let client = history_client()?;
    search_discord_history_via_channel_scan_with_client(
        &client,
        api_base.as_str(),
        bot_token,
        scope,
        requested_channel_id,
        query,
        limit,
    )
}

fn search_discord_history_via_channel_scan_with_client(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    scope: &DiscordScopeGrant,
    requested_channel_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<(Vec<ChatHistoryMatch>, usize, Vec<String>), BoxError> {
    let query_norm = query.to_ascii_lowercase();
    let mut warnings = Vec::new();
    let channels = resolve_discord_search_channels(
        client,
        api_base,
        bot_token,
        scope,
        requested_channel_id,
        &mut warnings,
    )?;
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let searched_channels = channels.len();

    for channel in channels {
        let channel_matches = search_discord_channel_messages(
            client,
            api_base,
            bot_token,
            &channel.id,
            channel.name.as_deref(),
            &query_norm,
            &mut warnings,
        )?;
        for entry in channel_matches {
            let key = format!("{}:{}", entry.channel_id, entry.message_id);
            if seen.insert(key) {
                results.push(entry);
            }
        }
    }

    sort_matches_desc(&mut results);
    results.truncate(limit);
    Ok((results, searched_channels, warnings))
}

fn resolve_discord_search_channels(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    scope: &DiscordScopeGrant,
    requested_channel_id: Option<&str>,
    warnings: &mut Vec<String>,
) -> Result<Vec<DiscordGuildChannel>, BoxError> {
    if let Some(guild_id) = scope.guild_id {
        let mut channels = fetch_discord_guild_channels(client, api_base, bot_token, guild_id)?;
        let active_threads = fetch_discord_active_threads(client, api_base, bot_token, guild_id)
            .unwrap_or_else(|err| {
                warnings.push(format!(
                    "could not fetch active Discord threads in guild {guild_id}: {err}"
                ));
                Vec::new()
            });
        let mut channel_map = HashMap::new();
        for channel in channels.drain(..) {
            channel_map.insert(channel.id.clone(), channel);
        }
        for thread in active_threads {
            channel_map.insert(thread.id.clone(), thread);
        }
        let mut all_channels = channel_map.into_values().collect::<Vec<_>>();
        all_channels.sort_by(|left, right| left.id.cmp(&right.id));
        if let Some(requested) = requested_channel_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let selected = all_channels
                .into_iter()
                .find(|channel| channel.id == requested)
                .ok_or_else(|| {
                    format!(
                        "requested Discord channel {requested} is outside the allowed guild scope"
                    )
                })?;
            return Ok(vec![selected]);
        }
        if all_channels.len() > discord_max_channels() {
            warnings.push(format!(
                "Discord guild has {} searchable channels; only the first {} channels were scanned.",
                all_channels.len(),
                discord_max_channels()
            ));
            all_channels.truncate(discord_max_channels());
        }
        warnings.push(
            "Archived Discord threads are not scanned yet; active threads and regular text channels are included.".to_string(),
        );
        return Ok(all_channels);
    }

    let requested = requested_channel_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(requested_channel_id) = requested {
        if requested_channel_id != scope.channel_id.to_string() {
            return Err("Discord DM history cannot access another channel".into());
        }
    }
    Ok(vec![DiscordGuildChannel {
        id: scope.channel_id.to_string(),
        name: None,
        kind: 0,
    }])
}

fn fetch_discord_guild_channels(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    guild_id: u64,
) -> Result<Vec<DiscordGuildChannel>, BoxError> {
    let response = client
        .get(format!(
            "{}/guilds/{guild_id}/channels",
            api_base.trim_end_matches('/')
        ))
        .header("Authorization", format!("Bot {bot_token}"))
        .send()?;
    if !response.status().is_success() {
        return Err(format!("discord guild channels api returned {}", response.status()).into());
    }
    let channels: Vec<DiscordGuildChannel> = response.json()?;
    Ok(channels
        .into_iter()
        .filter(|channel| DISCORD_TEXT_CHANNEL_TYPES.contains(&channel.kind))
        .collect())
}

fn fetch_discord_active_threads(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    guild_id: u64,
) -> Result<Vec<DiscordGuildChannel>, BoxError> {
    let response = client
        .get(format!(
            "{}/guilds/{guild_id}/threads/active",
            api_base.trim_end_matches('/')
        ))
        .header("Authorization", format!("Bot {bot_token}"))
        .send()?;
    if !response.status().is_success() {
        return Err(format!("discord active threads api returned {}", response.status()).into());
    }
    let payload: DiscordActiveThreadsResponse = response.json()?;
    Ok(payload
        .threads
        .into_iter()
        .filter(|channel| DISCORD_TEXT_CHANNEL_TYPES.contains(&channel.kind))
        .collect())
}

fn search_discord_channel_messages(
    client: &Client,
    api_base: &str,
    bot_token: &str,
    channel_id: &str,
    channel_name: Option<&str>,
    query_norm: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<ChatHistoryMatch>, BoxError> {
    let mut results = Vec::new();
    let mut before: Option<String> = None;
    for _ in 0..discord_max_history_pages_per_channel() {
        let label = discord_channel_label(channel_id, channel_name);
        let mut rate_limit_attempt = 0usize;
        let messages: Vec<DiscordHistoryMessage> = loop {
            let mut request = client
                .get(format!(
                    "{}/channels/{channel_id}/messages",
                    api_base.trim_end_matches('/')
                ))
                .header("Authorization", format!("Bot {bot_token}"))
                .query(&[("limit", "100")]);
            if let Some(before_id) = before.as_deref() {
                request = request.query(&[("before", before_id)]);
            }

            let response = request.send()?;
            if response.status().is_success() {
                break response.json()?;
            }

            let status = response.status();
            if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND
            {
                warnings.push(format!(
                    "Skipped Discord channel {label} because the bot could not read its history ({status})."
                ));
                return Ok(Vec::new());
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let headers = response.headers().clone();
                let body = response.text().unwrap_or_default();
                if rate_limit_attempt < discord_rate_limit_retries() {
                    rate_limit_attempt += 1;
                    std::thread::sleep(discord_rate_limit_delay(&headers, &body));
                    continue;
                }
                warnings.push(format!(
                    "Skipped Discord channel {label} after repeated rate limits ({status})."
                ));
                return Ok(Vec::new());
            }
            return Err(format!(
                "discord channel history api for {channel_id} returned {}",
                status
            )
            .into());
        };
        if messages.is_empty() {
            break;
        }

        for message in &messages {
            let text = discord_message_text(message);
            if !message_matches_query(&text, query_norm) {
                continue;
            }
            results.push(ChatHistoryMatch {
                channel_id: channel_id.to_string(),
                channel_name: channel_name.map(|value| value.to_string()),
                message_id: message.id.clone(),
                thread_id: None,
                timestamp: message.timestamp.clone(),
                author_id: Some(message.author.id.clone()),
                author_name: Some(
                    message
                        .author
                        .global_name
                        .clone()
                        .unwrap_or_else(|| message.author.username.clone()),
                ),
                text: truncate_match_text(&text),
                source: "discord_message".to_string(),
            });
        }

        before = messages.last().map(|message| message.id.clone());
    }
    Ok(results)
}

fn discord_channel_label(channel_id: &str, channel_name: Option<&str>) -> String {
    channel_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value} ({channel_id})"))
        .unwrap_or_else(|| channel_id.to_string())
}

fn discord_rate_limit_delay(headers: &ReqwestHeaderMap, body: &str) -> StdDuration {
    for header_name in ["retry-after", "x-ratelimit-reset-after"] {
        if let Some(value) = headers
            .get(header_name)
            .and_then(|value| value.to_str().ok())
        {
            if let Some(delay) = parse_discord_rate_limit_seconds(value) {
                return delay;
            }
        }
    }
    if let Ok(payload) = serde_json::from_str::<DiscordRateLimitPayload>(body) {
        if let Some(retry_after) = payload.retry_after {
            if let Some(delay) = parse_discord_rate_limit_seconds(&retry_after.to_string()) {
                return delay;
            }
        }
    }
    discord_rate_limit_fallback_delay()
}

fn parse_discord_rate_limit_seconds(raw: &str) -> Option<StdDuration> {
    let seconds = raw.trim().parse::<f64>().ok()?;
    let delay = StdDuration::from_secs_f64(seconds.max(0.0));
    if delay.is_zero() {
        Some(StdDuration::from_millis(50))
    } else {
        Some(delay)
    }
}

fn discord_message_text(message: &DiscordHistoryMessage) -> String {
    discord_search_message_text(&message.content, &message.attachments)
}

fn discord_search_message_text(content: &str, attachments: &[DiscordAttachment]) -> String {
    let text = content.trim();
    if !text.is_empty() {
        return text.to_string();
    }
    let attachment_names = attachments
        .iter()
        .map(|attachment| attachment.filename.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if attachment_names.is_empty() {
        "(empty Discord message)".to_string()
    } else {
        format!("[attachments: {}]", attachment_names.join(", "))
    }
}

fn message_matches_query(text: &str, query_norm: &str) -> bool {
    text.to_ascii_lowercase().contains(query_norm)
}

fn truncate_match_text(text: &str) -> String {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let collected = chars.by_ref().take(1200).collect::<String>();
    if chars.next().is_some() {
        format!("{collected}...")
    } else {
        collected
    }
}

fn sort_matches_desc(matches: &mut [ChatHistoryMatch]) {
    matches.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
}

fn resolve_discord_bot_token(config: &ServiceConfig) -> Option<String> {
    let employee_prefix = config.employee_profile.id.to_uppercase().replace('-', "_");
    env_trimmed(&format!("{employee_prefix}_DISCORD_BOT_TOKEN"))
        .or_else(|| config.discord_bot_token.clone())
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    struct EnvGuard {
        key: String,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = env::var(key).ok();
            env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                env::set_var(&self.key, previous);
            } else {
                env::remove_var(&self.key);
            }
        }
    }

    fn test_config() -> ServiceConfig {
        let profile = crate::employee_config::EmployeeProfile {
            id: "little_bear".to_string(),
            display_name: Some("Little Bear".to_string()),
            runner: "codex".to_string(),
            model: None,
            addresses: Vec::new(),
            address_set: HashSet::new(),
            runtime_root: None,
            agents_path: None,
            claude_path: None,
            soul_path: None,
            skills_dir: None,
            discord_enabled: true,
            slack_enabled: true,
            bluebubbles_enabled: false,
            notion_user_id: None,
        };
        let employee_directory = crate::employee_config::EmployeeDirectory {
            default_employee_id: Some("little_bear".to_string()),
            service_addresses: HashSet::new(),
            employee_by_id: HashMap::from([(profile.id.clone(), profile.clone())]),
            employees: vec![profile.clone()],
        };
        ServiceConfig {
            host: "0.0.0.0".to_string(),
            port: 9001,
            employee_id: "little_bear".to_string(),
            employee_config_path: std::path::PathBuf::from("employee.toml"),
            employee_profile: profile,
            employee_directory,
            workspace_root: std::path::PathBuf::from("/tmp/workspaces"),
            scheduler_state_path: std::path::PathBuf::from("/tmp/scheduler_state"),
            processed_ids_path: std::path::PathBuf::from("/tmp/processed_ids"),
            ingestion_db_url: "postgres://example".to_string(),
            ingestion_poll_interval: std::time::Duration::from_secs(1),
            users_root: std::path::PathBuf::from("/tmp/users"),
            users_db_path: std::path::PathBuf::from("/tmp/users.db"),
            task_index_path: std::path::PathBuf::from("/tmp/task_index.db"),
            codex_model: "gpt-5.4".to_string(),
            codex_disabled: false,
            scheduler_poll_interval: std::time::Duration::from_secs(1),
            scheduler_max_concurrency: 1,
            scheduler_user_max_concurrency: 1,
            inbound_body_max_bytes: 1024,
            skills_source_dir: None,
            slack_bot_token: Some("xoxb-secret".to_string()),
            slack_bot_user_id: Some("U123".to_string()),
            slack_store_path: std::path::PathBuf::from("/tmp/slack.db"),
            slack_client_id: None,
            slack_client_secret: Some("slack-client-secret".to_string()),
            slack_redirect_uri: None,
            discord_bot_token: Some("discord-secret".to_string()),
            discord_bot_user_id: Some(123),
            google_docs_enabled: false,
            bluebubbles_url: None,
            bluebubbles_password: None,
            telegram_bot_token: None,
            whatsapp_access_token: None,
            whatsapp_phone_number_id: None,
            whatsapp_verify_token: None,
        }
    }

    #[test]
    fn scope_grants_round_trip() {
        let _lock = env_lock();
        let _secret = EnvGuard::set(CHAT_HISTORY_SCOPE_SIGNING_SECRET_ENV, "scope-secret");
        let config = test_config();
        let claims = build_scope_grant(
            &config,
            ChatHistoryPlatform::Slack,
            ChatHistoryScopeMode::CurrentWorkspace,
            Some(SlackScopeGrant {
                team_id: "T123".to_string(),
                channel_id: "C123".to_string(),
                thread_id: Some("1700.1".to_string()),
            }),
            None,
        )
        .expect("build scope");
        let token = encode_scope_grant(&config, &claims).expect("encode");
        let decoded = decode_scope_grant(&config, &token).expect("decode");
        assert_eq!(decoded.platform, ChatHistoryPlatform::Slack);
        assert_eq!(decoded.scope_mode, ChatHistoryScopeMode::CurrentWorkspace);
        let slack = decoded.slack.expect("slack scope");
        assert_eq!(slack.team_id, "T123");
        assert_eq!(slack.channel_id, "C123");
    }

    #[test]
    fn write_slack_scope_file_uses_workspace_scope() {
        let _lock = env_lock();
        let _secret = EnvGuard::set(CHAT_HISTORY_SCOPE_SIGNING_SECRET_ENV, "scope-secret");
        let workspace = tempfile::tempdir().expect("tempdir");
        let config = test_config();
        let message = crate::channel::InboundMessage {
            channel: crate::channel::Channel::Slack,
            sender: "U123".to_string(),
            sender_name: Some("User".to_string()),
            recipient: "C123".to_string(),
            subject: None,
            text_body: Some("hello".to_string()),
            html_body: None,
            thread_id: "1700.1".to_string(),
            message_id: Some("1700.2".to_string()),
            attachments: Vec::new(),
            reply_to: vec!["U123".to_string(), "C123".to_string()],
            raw_payload: Vec::new(),
            metadata: crate::channel::ChannelMetadata {
                slack_team_id: Some("T123".to_string()),
                slack_channel_id: Some("C123".to_string()),
                ..crate::channel::ChannelMetadata::default()
            },
        };
        write_slack_chat_history_scope_file(&config, workspace.path(), &message)
            .expect("write scope");
        let payload = std::fs::read_to_string(workspace.path().join(CHAT_HISTORY_SCOPE_FILE_NAME))
            .expect("scope file");
        assert!(payload.contains("\"scope_mode\": \"current_workspace\""));
        assert!(payload.contains("\"team_id\": \"T123\""));
        assert!(payload.contains("\"channel_id\": \"C123\""));
        assert!(payload.contains("current Slack workspace/team"));
    }

    #[test]
    fn write_discord_scope_file_uses_service_url_override() {
        let _lock = env_lock();
        let _secret = EnvGuard::set(CHAT_HISTORY_SCOPE_SIGNING_SECRET_ENV, "scope-secret");
        let _service_url = EnvGuard::set(SERVICE_URL_ENV, "https://worker.example.com");
        let workspace = tempfile::tempdir().expect("tempdir");
        let config = test_config();
        let message = crate::channel::InboundMessage {
            channel: crate::channel::Channel::Discord,
            sender: "123".to_string(),
            sender_name: Some("User".to_string()),
            recipient: "456".to_string(),
            subject: None,
            text_body: Some("hello".to_string()),
            html_body: None,
            thread_id: "789".to_string(),
            message_id: Some("789".to_string()),
            attachments: Vec::new(),
            reply_to: vec!["123".to_string()],
            raw_payload: Vec::new(),
            metadata: crate::channel::ChannelMetadata {
                discord_guild_id: Some(42),
                discord_channel_id: Some(84),
                ..crate::channel::ChannelMetadata::default()
            },
        };
        write_discord_chat_history_scope_file(&config, workspace.path(), &message)
            .expect("write scope");
        let payload = std::fs::read_to_string(workspace.path().join(CHAT_HISTORY_SCOPE_FILE_NAME))
            .expect("scope file");
        assert!(payload.contains("https://worker.example.com/internal/chat-history/search"));
        assert!(payload.contains("\"guild_id\": 42"));
    }

    #[test]
    fn write_discord_scope_file_uses_public_service_base_for_azure_aci() {
        let _lock = env_lock();
        let _secret = EnvGuard::set(CHAT_HISTORY_SCOPE_SIGNING_SECRET_ENV, "scope-secret");
        let _run_task_backend =
            EnvGuard::set(RUN_TASK_EXECUTION_BACKEND_ENV, AZURE_ACI_EXECUTION_BACKEND);
        let _proxy = EnvGuard::set(CHAT_HISTORY_PROXY_BASE_URL_ENV, "");
        let _dowhiz_api = EnvGuard::set(DOWHIZ_API_URL_ENV, "");
        let _service_url = EnvGuard::set(SERVICE_URL_ENV, "");
        let _frontend_url = EnvGuard::set(FRONTEND_URL_ENV, "");
        let _postmark_hook = EnvGuard::set(
            POSTMARK_INBOUND_HOOK_URL_ENV,
            "https://api.staging.dowhiz.com/postmark/inbound",
        );
        let workspace = tempfile::tempdir().expect("tempdir");
        let config = test_config();
        let message = crate::channel::InboundMessage {
            channel: crate::channel::Channel::Discord,
            sender: "123".to_string(),
            sender_name: Some("User".to_string()),
            recipient: "456".to_string(),
            subject: None,
            text_body: Some("hello".to_string()),
            html_body: None,
            thread_id: "789".to_string(),
            message_id: Some("789".to_string()),
            attachments: Vec::new(),
            reply_to: vec!["123".to_string()],
            raw_payload: Vec::new(),
            metadata: crate::channel::ChannelMetadata {
                discord_guild_id: Some(42),
                discord_channel_id: Some(84),
                ..crate::channel::ChannelMetadata::default()
            },
        };
        write_discord_chat_history_scope_file(&config, workspace.path(), &message)
            .expect("write scope");
        let payload = std::fs::read_to_string(workspace.path().join(CHAT_HISTORY_SCOPE_FILE_NAME))
            .expect("scope file");
        assert!(
            payload.contains("https://api.staging.dowhiz.com/service/internal/chat-history/search")
        );
    }

    #[test]
    fn write_discord_scope_file_keeps_local_base_without_remote_execution_backend() {
        let _lock = env_lock();
        let _secret = EnvGuard::set(CHAT_HISTORY_SCOPE_SIGNING_SECRET_ENV, "scope-secret");
        let _run_task_backend = EnvGuard::set(RUN_TASK_EXECUTION_BACKEND_ENV, "");
        let _proxy = EnvGuard::set(CHAT_HISTORY_PROXY_BASE_URL_ENV, "");
        let _dowhiz_api = EnvGuard::set(DOWHIZ_API_URL_ENV, "");
        let _service_url = EnvGuard::set(SERVICE_URL_ENV, "");
        let _frontend_url = EnvGuard::set(FRONTEND_URL_ENV, "https://api.staging.dowhiz.com/");
        let _postmark_hook = EnvGuard::set(
            POSTMARK_INBOUND_HOOK_URL_ENV,
            "https://api.staging.dowhiz.com/postmark/inbound",
        );
        let workspace = tempfile::tempdir().expect("tempdir");
        let config = test_config();
        let message = crate::channel::InboundMessage {
            channel: crate::channel::Channel::Discord,
            sender: "123".to_string(),
            sender_name: Some("User".to_string()),
            recipient: "456".to_string(),
            subject: None,
            text_body: Some("hello".to_string()),
            html_body: None,
            thread_id: "789".to_string(),
            message_id: Some("789".to_string()),
            attachments: Vec::new(),
            reply_to: vec!["123".to_string()],
            raw_payload: Vec::new(),
            metadata: crate::channel::ChannelMetadata {
                discord_guild_id: Some(42),
                discord_channel_id: Some(84),
                ..crate::channel::ChannelMetadata::default()
            },
        };
        write_discord_chat_history_scope_file(&config, workspace.path(), &message)
            .expect("write scope");
        let payload = std::fs::read_to_string(workspace.path().join(CHAT_HISTORY_SCOPE_FILE_NAME))
            .expect("scope file");
        assert!(payload.contains("http://127.0.0.1:9001/internal/chat-history/search"));
    }

    #[test]
    fn slack_history_search_reads_thread_replies() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("SLACK_API_BASE_URL", &server.url());

        let history_mock = server
            .mock("GET", "/conversations.history")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C123".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000001.000100","text":"Root message","reply_count":1},{"ts":"1700000000.000100","text":"Older note"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();
        let replies_mock = server
            .mock("GET", "/conversations.replies")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C123".into()),
                mockito::Matcher::UrlEncoded("ts".into(), "1700000001.000100".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000001.000100","text":"Root message"},{"ts":"1700000001.000200","text":"Needle in a thread"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();

        let results =
            search_slack_channel_history("xoxb-secret", "C123", "needle", 10).expect("search");
        history_mock.assert();
        replies_mock.assert();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].thread_id.as_deref(), Some("1700000001.000100"));
        assert!(results[0].text.contains("Needle"));
    }

    #[test]
    fn slack_workspace_search_scans_multiple_conversations() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("SLACK_API_BASE_URL", &server.url());

        let list_mock = server
            .mock("GET", "/conversations.list")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
                mockito::Matcher::UrlEncoded(
                    "types".into(),
                    "public_channel,private_channel,mpim,im".into(),
                ),
                mockito::Matcher::UrlEncoded("team_id".into(), "T123".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"channels":[{"id":"C999","name":"launch"},{"id":"C123","name":"general"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();
        let general_history = server
            .mock("GET", "/conversations.history")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C123".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000001.000100","text":"General update"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();
        let launch_history = server
            .mock("GET", "/conversations.history")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C999".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000002.000100","text":"Launch root","reply_count":1}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();
        let launch_replies = server
            .mock("GET", "/conversations.replies")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C999".into()),
                mockito::Matcher::UrlEncoded("ts".into(), "1700000002.000100".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000002.000100","text":"Launch root"},{"ts":"1700000002.000200","text":"Needle in launch thread"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();

        let scope = SlackScopeGrant {
            team_id: "T123".to_string(),
            channel_id: "C123".to_string(),
            thread_id: Some("1700.1".to_string()),
        };
        let outcome =
            search_slack_history("xoxb-secret", &scope, None, "needle", 10).expect("search");

        list_mock.assert();
        general_history.assert();
        launch_history.assert();
        launch_replies.assert();
        assert_eq!(outcome.searched_channels, 2);
        assert!(outcome.warnings.is_empty());
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].channel_id, "C999");
        assert_eq!(outcome.results[0].channel_name.as_deref(), Some("launch"));
        assert_eq!(
            outcome.results[0].thread_id.as_deref(),
            Some("1700000002.000100")
        );
    }

    #[test]
    fn slack_workspace_search_allows_requested_conversation() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("SLACK_API_BASE_URL", &server.url());

        let list_mock = server
            .mock("GET", "/conversations.list")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
                mockito::Matcher::UrlEncoded(
                    "types".into(),
                    "public_channel,private_channel,mpim,im".into(),
                ),
                mockito::Matcher::UrlEncoded("team_id".into(), "T123".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"channels":[{"id":"C999","name":"launch"},{"id":"C123","name":"general"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();
        let requested_history = server
            .mock("GET", "/conversations.history")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C999".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000003.000100","text":"Needle from requested channel"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .expect(1)
            .create();

        let scope = SlackScopeGrant {
            team_id: "T123".to_string(),
            channel_id: "C123".to_string(),
            thread_id: Some("1700.1".to_string()),
        };
        let outcome = search_slack_history("xoxb-secret", &scope, Some("C999"), "needle", 10)
            .expect("search");

        list_mock.assert();
        requested_history.assert();
        assert_eq!(outcome.searched_channels, 1);
        assert!(outcome.warnings.is_empty());
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].channel_id, "C999");
        assert_eq!(outcome.results[0].channel_name.as_deref(), Some("launch"));
    }

    #[test]
    fn slack_workspace_search_keeps_origin_conversation_when_capped() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("SLACK_API_BASE_URL", &server.url());
        let _max_channels = EnvGuard::set(CHAT_HISTORY_SLACK_MAX_CHANNELS_ENV, "1");

        let list_mock = server
            .mock("GET", "/conversations.list")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
                mockito::Matcher::UrlEncoded(
                    "types".into(),
                    "public_channel,private_channel,mpim,im".into(),
                ),
                mockito::Matcher::UrlEncoded("team_id".into(), "T123".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"channels":[{"id":"C999","name":"launch"},{"id":"C123","name":"general"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();
        let origin_history = server
            .mock("GET", "/conversations.history")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C123".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000004.000100","text":"Needle from origin"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();

        let scope = SlackScopeGrant {
            team_id: "T123".to_string(),
            channel_id: "C123".to_string(),
            thread_id: Some("1700.1".to_string()),
        };
        let outcome =
            search_slack_history("xoxb-secret", &scope, None, "needle", 10).expect("search");

        list_mock.assert();
        origin_history.assert();
        assert_eq!(outcome.searched_channels, 1);
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].channel_id, "C123");
        assert!(outcome.warnings.iter().any(|warning| {
            warning.contains("only the first 1 were scanned")
                && warning.contains("origin conversation kept in scope")
        }));
    }

    #[test]
    fn slack_workspace_search_falls_back_when_conversation_list_is_unavailable() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("SLACK_API_BASE_URL", &server.url());

        let list_mock = server
            .mock("GET", "/conversations.list")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
                mockito::Matcher::UrlEncoded(
                    "types".into(),
                    "public_channel,private_channel,mpim,im".into(),
                ),
                mockito::Matcher::UrlEncoded("team_id".into(), "T123".into()),
            ]))
            .with_status(200)
            .with_body(r#"{"ok":false,"error":"missing_scope"}"#)
            .create();
        let origin_history = server
            .mock("GET", "/conversations.history")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C123".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000005.000100","text":"Needle from fallback origin"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();

        let scope = SlackScopeGrant {
            team_id: "T123".to_string(),
            channel_id: "C123".to_string(),
            thread_id: Some("1700.1".to_string()),
        };
        let outcome =
            search_slack_history("xoxb-secret", &scope, None, "needle", 10).expect("search");

        list_mock.assert();
        origin_history.assert();
        assert_eq!(outcome.searched_channels, 1);
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].channel_id, "C123");
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("Falling back to the origin Slack conversation")));
    }

    #[test]
    fn slack_workspace_search_surfaces_needed_scope_details() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("SLACK_API_BASE_URL", &server.url());

        let list_mock = server
            .mock("GET", "/conversations.list")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
                mockito::Matcher::UrlEncoded(
                    "types".into(),
                    "public_channel,private_channel,mpim,im".into(),
                ),
                mockito::Matcher::UrlEncoded("team_id".into(), "T123".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":false,"error":"missing_scope","needed":"channels:read","provided":"channels:history,chat:write"}"#,
            )
            .create();
        let origin_history = server
            .mock("GET", "/conversations.history")
            .match_header("authorization", "Bearer xoxb-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("channel".into(), "C123".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"ok":true,"messages":[{"ts":"1700000005.000100","text":"Needle from fallback origin"}],"response_metadata":{"next_cursor":""}}"#,
            )
            .create();

        let scope = SlackScopeGrant {
            team_id: "T123".to_string(),
            channel_id: "C123".to_string(),
            thread_id: Some("1700.1".to_string()),
        };
        let outcome =
            search_slack_history("xoxb-secret", &scope, None, "needle", 10).expect("search");

        list_mock.assert();
        origin_history.assert();
        assert!(outcome.warnings.iter().any(|warning| {
            warning.contains("needed scope: channels:read")
                && warning.contains("provided: channels:history,chat:write")
        }));
    }

    #[test]
    fn discord_dm_scope_rejects_other_channel() {
        let scope = DiscordScopeGrant {
            guild_id: None,
            channel_id: 999,
            thread_id: Some("999".to_string()),
        };
        let mut warnings = Vec::new();
        let error = resolve_discord_search_channels(
            &history_client().expect("client"),
            "https://discord.com/api/v10",
            "discord-secret",
            &scope,
            Some("123"),
            &mut warnings,
        )
        .expect_err("expected scope violation");
        assert!(error.to_string().contains("cannot access another channel"));
    }

    #[test]
    fn discord_guild_search_prefers_official_search() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("DISCORD_API_BASE_URL", &server.url());

        let official = server
            .mock("GET", "/guilds/42/messages/search")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("content".into(), "needle".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "10".into()),
                mockito::Matcher::UrlEncoded("offset".into(), "0".into()),
                mockito::Matcher::UrlEncoded("sort_by".into(), "timestamp".into()),
                mockito::Matcher::UrlEncoded("sort_order".into(), "desc".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"messages":[[{"id":"200","channel_id":"222","content":"Needle from official search","timestamp":"2026-03-23T01:00:00Z","author":{"id":"user-1","username":"alice","global_name":"Alice"},"attachments":[],"hit":true}]],"doing_deep_historical_index":false,"total_results":1}"#,
            )
            .create();

        let scope = DiscordScopeGrant {
            guild_id: Some(42),
            channel_id: 222,
            thread_id: Some("thread-1".to_string()),
        };
        let outcome = search_discord_history("discord-secret", &scope, None, "needle", 10)
            .expect("official search should succeed");

        official.assert();
        assert_eq!(
            outcome.engine,
            ChatHistorySearchEngine::DiscordOfficialSearch
        );
        assert!(!outcome.fallback_used);
        assert_eq!(outcome.searched_channels, 0);
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].channel_id, "222");
        assert_eq!(outcome.results[0].source, "discord_official_search");
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn discord_guild_search_falls_back_when_official_index_is_not_ready() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("DISCORD_API_BASE_URL", &server.url());

        let official = server
            .mock("GET", "/guilds/42/messages/search")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("content".into(), "needle".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "10".into()),
                mockito::Matcher::UrlEncoded("offset".into(), "0".into()),
                mockito::Matcher::UrlEncoded("sort_by".into(), "timestamp".into()),
                mockito::Matcher::UrlEncoded("sort_order".into(), "desc".into()),
            ]))
            .with_status(202)
            .with_body(r#"{"message":"Index not ready","documents_indexed":123,"retry_after":3}"#)
            .create();
        let guild_channels = server
            .mock("GET", "/guilds/42/channels")
            .match_header("authorization", "Bot discord-secret")
            .with_status(200)
            .with_body(r#"[{"id":"222","name":"general","type":0}]"#)
            .create();
        let active_threads = server
            .mock("GET", "/guilds/42/threads/active")
            .match_header("authorization", "Bot discord-secret")
            .with_status(200)
            .with_body(r#"{"threads":[]}"#)
            .create();
        let general_page_one = server
            .mock("GET", "/channels/222/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
            .with_status(200)
            .with_body(
                r#"[{"id":"200","content":"Needle from fallback scan","timestamp":"2026-03-23T00:00:00Z","author":{"id":"user-1","username":"alice","global_name":"Alice"},"attachments":[]}]"#,
            )
            .create();
        let general_page_two = server
            .mock("GET", "/channels/222/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "100".into()),
                mockito::Matcher::UrlEncoded("before".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body("[]")
            .create();

        let scope = DiscordScopeGrant {
            guild_id: Some(42),
            channel_id: 222,
            thread_id: Some("thread-1".to_string()),
        };
        let outcome = search_discord_history("discord-secret", &scope, None, "needle", 10)
            .expect("scan fallback should succeed");

        official.assert();
        guild_channels.assert();
        active_threads.assert();
        general_page_one.assert();
        general_page_two.assert();
        assert_eq!(outcome.engine, ChatHistorySearchEngine::DiscordChannelScan);
        assert!(outcome.fallback_used);
        assert_eq!(outcome.searched_channels, 1);
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].source, "discord_message");
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| { warning.contains("official guild search index is not ready yet") }));
    }

    #[test]
    fn discord_guild_search_retries_rate_limited_official_search() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("DISCORD_API_BASE_URL", &server.url());
        let _retries = EnvGuard::set(CHAT_HISTORY_DISCORD_RATE_LIMIT_RETRIES_ENV, "1");
        let _fallback_ms = EnvGuard::set(CHAT_HISTORY_DISCORD_RATE_LIMIT_FALLBACK_MS_ENV, "0");

        let rate_limited = server
            .mock("GET", "/guilds/42/messages/search")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("content".into(), "needle".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "10".into()),
                mockito::Matcher::UrlEncoded("offset".into(), "0".into()),
                mockito::Matcher::UrlEncoded("sort_by".into(), "timestamp".into()),
                mockito::Matcher::UrlEncoded("sort_order".into(), "desc".into()),
            ]))
            .with_status(429)
            .with_header("retry-after", "0")
            .with_body(r#"{"message":"You are being rate limited.","retry_after":0.0}"#)
            .expect(1)
            .create();
        let official = server
            .mock("GET", "/guilds/42/messages/search")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("content".into(), "needle".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "10".into()),
                mockito::Matcher::UrlEncoded("offset".into(), "0".into()),
                mockito::Matcher::UrlEncoded("sort_by".into(), "timestamp".into()),
                mockito::Matcher::UrlEncoded("sort_order".into(), "desc".into()),
            ]))
            .with_status(200)
            .with_body(
                r#"{"messages":[[{"id":"200","channel_id":"222","content":"Needle from official search","timestamp":"2026-03-23T01:00:00Z","author":{"id":"user-1","username":"alice","global_name":"Alice"},"attachments":[],"hit":true}]],"doing_deep_historical_index":false,"total_results":1}"#,
            )
            .expect(1)
            .create();

        let scope = DiscordScopeGrant {
            guild_id: Some(42),
            channel_id: 222,
            thread_id: Some("thread-1".to_string()),
        };
        let outcome = search_discord_history("discord-secret", &scope, None, "needle", 10)
            .expect("official search should succeed after retry");

        rate_limited.assert();
        official.assert();
        assert_eq!(
            outcome.engine,
            ChatHistorySearchEngine::DiscordOfficialSearch
        );
        assert!(!outcome.fallback_used);
        assert_eq!(outcome.results.len(), 1);
        assert!(outcome
            .warnings
            .iter()
            .all(|warning| !warning.contains("falling back")));
    }

    #[test]
    fn discord_guild_search_skips_forbidden_channels() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("DISCORD_API_BASE_URL", &server.url());

        let guild_channels = server
            .mock("GET", "/guilds/42/channels")
            .match_header("authorization", "Bot discord-secret")
            .with_status(200)
            .with_body(
                r#"[{"id":"111","name":"restricted","type":0},{"id":"222","name":"general","type":0}]"#,
            )
            .create();
        let active_threads = server
            .mock("GET", "/guilds/42/threads/active")
            .match_header("authorization", "Bot discord-secret")
            .with_status(200)
            .with_body(r#"{"threads":[]}"#)
            .create();
        let restricted = server
            .mock("GET", "/channels/111/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
            .with_status(403)
            .create();
        let general_page_one = server
            .mock("GET", "/channels/222/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
            .with_status(200)
            .with_body(
                r#"[{"id":"200","content":"Needle from general","timestamp":"2026-03-23T00:00:00Z","author":{"id":"user-1","username":"alice","global_name":"Alice"},"attachments":[]}]"#,
            )
            .create();
        let general_page_two = server
            .mock("GET", "/channels/222/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "100".into()),
                mockito::Matcher::UrlEncoded("before".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body("[]")
            .create();

        let scope = DiscordScopeGrant {
            guild_id: Some(42),
            channel_id: 222,
            thread_id: Some("thread-1".to_string()),
        };
        let (results, searched_channels, warnings) =
            search_discord_history_via_channel_scan("discord-secret", &scope, None, "needle", 10)
                .expect("search should succeed");

        guild_channels.assert();
        active_threads.assert();
        restricted.assert();
        general_page_one.assert();
        general_page_two.assert();
        assert_eq!(searched_channels, 2);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].channel_id, "222");
        assert!(warnings.iter().any(|warning| {
            warning.contains("Skipped Discord channel") && warning.contains("111")
        }));
    }

    #[test]
    fn discord_guild_search_retries_rate_limited_channels() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("DISCORD_API_BASE_URL", &server.url());
        let _retries = EnvGuard::set(CHAT_HISTORY_DISCORD_RATE_LIMIT_RETRIES_ENV, "2");
        let _fallback_ms = EnvGuard::set(CHAT_HISTORY_DISCORD_RATE_LIMIT_FALLBACK_MS_ENV, "0");

        let guild_channels = server
            .mock("GET", "/guilds/42/channels")
            .match_header("authorization", "Bot discord-secret")
            .with_status(200)
            .with_body(r#"[{"id":"111","name":"general","type":0}]"#)
            .create();
        let active_threads = server
            .mock("GET", "/guilds/42/threads/active")
            .match_header("authorization", "Bot discord-secret")
            .with_status(200)
            .with_body(r#"{"threads":[]}"#)
            .create();
        let rate_limited = server
            .mock("GET", "/channels/111/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
            .with_status(429)
            .with_header("retry-after", "0")
            .with_body(r#"{"message":"You are being rate limited.","retry_after":0.0}"#)
            .expect(1)
            .create();
        let general_page_one = server
            .mock("GET", "/channels/111/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
            .with_status(200)
            .with_body(
                r#"[{"id":"200","content":"Needle from general","timestamp":"2026-03-23T00:00:00Z","author":{"id":"user-1","username":"alice","global_name":"Alice"},"attachments":[]}]"#,
            )
            .expect(1)
            .create();
        let general_page_two = server
            .mock("GET", "/channels/111/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "100".into()),
                mockito::Matcher::UrlEncoded("before".into(), "200".into()),
            ]))
            .with_status(200)
            .with_body("[]")
            .create();

        let scope = DiscordScopeGrant {
            guild_id: Some(42),
            channel_id: 111,
            thread_id: Some("thread-1".to_string()),
        };
        let (results, searched_channels, warnings) =
            search_discord_history_via_channel_scan("discord-secret", &scope, None, "needle", 10)
                .expect("search should succeed");

        guild_channels.assert();
        active_threads.assert();
        rate_limited.assert();
        general_page_one.assert();
        general_page_two.assert();
        assert_eq!(searched_channels, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].channel_id, "111");
        assert!(warnings
            .iter()
            .all(|warning| !warning.contains("rate limits")));
    }

    #[test]
    fn discord_guild_search_skips_repeatedly_rate_limited_channels() {
        let _lock = env_lock();
        let mut server = mockito::Server::new();
        let _api = EnvGuard::set("DISCORD_API_BASE_URL", &server.url());
        let _retries = EnvGuard::set(CHAT_HISTORY_DISCORD_RATE_LIMIT_RETRIES_ENV, "1");
        let _fallback_ms = EnvGuard::set(CHAT_HISTORY_DISCORD_RATE_LIMIT_FALLBACK_MS_ENV, "0");

        let guild_channels = server
            .mock("GET", "/guilds/42/channels")
            .match_header("authorization", "Bot discord-secret")
            .with_status(200)
            .with_body(
                r#"[{"id":"111","name":"busy","type":0},{"id":"222","name":"general","type":0}]"#,
            )
            .create();
        let active_threads = server
            .mock("GET", "/guilds/42/threads/active")
            .match_header("authorization", "Bot discord-secret")
            .with_status(200)
            .with_body(r#"{"threads":[]}"#)
            .create();
        let rate_limited_once = server
            .mock("GET", "/channels/111/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
            .with_status(429)
            .with_header("retry-after", "0")
            .with_body(r#"{"message":"You are being rate limited.","retry_after":0.0}"#)
            .expect(1)
            .create();
        let rate_limited_twice = server
            .mock("GET", "/channels/111/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
            .with_status(429)
            .with_header("retry-after", "0")
            .with_body(r#"{"message":"You are being rate limited.","retry_after":0.0}"#)
            .expect(1)
            .create();
        let general_page_one = server
            .mock("GET", "/channels/222/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
            .with_status(200)
            .with_body(
                r#"[{"id":"300","content":"Needle from fallback channel","timestamp":"2026-03-23T00:00:00Z","author":{"id":"user-2","username":"bob","global_name":"Bob"},"attachments":[]}]"#,
            )
            .create();
        let general_page_two = server
            .mock("GET", "/channels/222/messages")
            .match_header("authorization", "Bot discord-secret")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "100".into()),
                mockito::Matcher::UrlEncoded("before".into(), "300".into()),
            ]))
            .with_status(200)
            .with_body("[]")
            .create();

        let scope = DiscordScopeGrant {
            guild_id: Some(42),
            channel_id: 222,
            thread_id: Some("thread-1".to_string()),
        };
        let (results, searched_channels, warnings) =
            search_discord_history_via_channel_scan("discord-secret", &scope, None, "needle", 10)
                .expect("search should succeed");

        guild_channels.assert();
        active_threads.assert();
        rate_limited_once.assert();
        rate_limited_twice.assert();
        general_page_one.assert();
        general_page_two.assert();
        assert_eq!(searched_channels, 2);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].channel_id, "222");
        assert!(warnings.iter().any(|warning| {
            warning.contains("Skipped Discord channel busy (111)")
                && warning.contains("repeated rate limits")
        }));
    }
}
