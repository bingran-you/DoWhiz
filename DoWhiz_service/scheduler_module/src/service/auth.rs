use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::task;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::account_store::{
    AccountStore, AccountStoreError, AnalyticsEventInsert, ChannelInstallOnboardingState,
};
use crate::blob_store::BlobStore;
use crate::google_auth::GoogleAuthConfig;
use crate::index_store::IndexStore;
use crate::notion_store::{NotionCredential, NotionStore};
use crate::scheduler::{
    append_task_execution_event, insert_scheduled_task, is_user_visible_routine_task,
    load_scheduled_task, persist_scheduled_task, prepare_task_for_resume,
    try_load_routines_with_status, try_load_task_executions, try_load_task_with_status,
    try_load_tasks_with_status_shared, RoutineSummary, Schedule, ScheduledTask,
    TaskExecutionSummary, TaskKind,
};
use crate::slack_store::{SlackInstallation, SlackStore};
use crate::thread_state::{
    default_thread_state_path, load_thread_state, write_thread_state, ThreadState,
};
use crate::user_store::UserStore;
use crate::{load_tasks_with_status, TaskStatusSummary};

use super::launch_execution::{generate_launch_execution_response, LaunchExecutionRequest};
use super::onboarding::{
    run_install_onboarding, AccountStoreInstallOnboardingStateStore,
    DiscordInstallOnboardingClient, InstallOnboardingConfig, InstallOnboardingRequest,
    InstallOnboardingRunResult, InstallOnboardingTrigger, InstallPlatform,
    SlackInstallOnboardingClient,
};
use super::startup_workspace::{
    derive_provider_capabilities, derive_provider_connections, evaluate_workspace_recommendations,
    generate_startup_intake_chat_response, LinkedIdentifierSnapshot, ProactivityLevel,
    ProviderCapabilityInputs, RecommendationFeedbackKind, RecommendationFeedbackSnapshot,
    StartupIntakeChatRequest, WorkspaceProviderRuntimeState,
    WorkspaceRecommendationFeedbackRequest, WorkspaceRecommendationPreferences,
    WorkspaceRecommendationPreferencesUpdateRequest, WorkspaceRecommendationRequest,
};

/// State for auth routes
#[derive(Clone)]
pub struct AuthState {
    pub account_store: Arc<AccountStore>,
    pub blob_store: Option<Arc<BlobStore>>,
    pub slack_store: Arc<SlackStore>,
    pub supabase_url: String,
    // Discord OAuth config
    pub discord_client_id: Option<String>,
    pub discord_client_secret: Option<String>,
    pub discord_redirect_uri: Option<String>,
    pub discord_bot_token: Option<String>,
    // Slack OAuth config
    pub slack_client_id: Option<String>,
    pub slack_client_secret: Option<String>,
    pub slack_redirect_uri: Option<String>,
    // GitHub OAuth config
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub github_redirect_uri: Option<String>,
    // Notion OAuth config
    pub notion_client_id: Option<String>,
    pub notion_client_secret: Option<String>,
    pub notion_redirect_uri: Option<String>,
    // Lark OAuth config
    pub lark_client_id: Option<String>,
    pub lark_client_secret: Option<String>,
    pub lark_redirect_uri: Option<String>,
    // WeCom OAuth config
    pub wechat_corp_id: Option<String>,
    pub wechat_corp_secret: Option<String>,
    pub wechat_agent_id: Option<String>,
    pub wechat_redirect_uri: Option<String>,
    // Frontend URL for redirects after OAuth
    pub frontend_url: String,
    pub install_onboarding_config: InstallOnboardingConfig,
    // User store and paths for task lookups
    pub user_store: Option<Arc<UserStore>>,
    pub users_root: Option<std::path::PathBuf>,
}

/// JWT Claims from Supabase token
#[derive(Debug, Deserialize)]
struct JwtClaims {
    sub: Uuid,  // User ID
    exp: usize, // Expiration time
    #[serde(default)]
    aud: Option<String>, // Audience (optional)
    #[serde(default)]
    email: Option<String>, // User's email from Supabase
}

/// Authenticated user info extracted from token
pub struct AuthUser {
    pub id: Uuid,
    pub email: Option<String>,
}

/// Cached JWT secret for local verification
fn get_jwt_secret() -> Option<String> {
    std::env::var("SUPABASE_JWT_SECRET").ok()
}

/// Extract and validate Supabase JWT locally, returns the auth user ID and email
/// This avoids an HTTP round-trip to Supabase on every request.
pub async fn validate_supabase_token(
    supabase_url: &str,
    token: &str,
) -> Result<AuthUser, (StatusCode, String)> {
    // Try local JWT verification first (fast path)
    if let Some(secret) = get_jwt_secret() {
        let key = DecodingKey::from_secret(secret.as_bytes());
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_aud = false; // Supabase doesn't always set aud

        match decode::<JwtClaims>(token, &key, &validation) {
            Ok(token_data) => {
                return Ok(AuthUser {
                    id: token_data.claims.sub,
                    email: token_data.claims.email,
                });
            }
            Err(e) => {
                warn!("Local JWT validation failed: {}", e);
                // Fall through to remote validation
            }
        }
    }

    // Fallback: validate via Supabase API (slow path)
    // This handles cases where JWT_SECRET isn't configured or token format differs
    let anon_key = std::env::var("SUPABASE_ANON_KEY").unwrap_or_default();
    info!(
        "validate_supabase_token: using remote validation, anon_key_set={}, url={}",
        !anon_key.is_empty(),
        supabase_url
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/auth/v1/user", supabase_url))
        .header("Authorization", format!("Bearer {}", token))
        .header("apikey", anon_key)
        .send()
        .await
        .map_err(|e| {
            error!("Failed to validate token with Supabase: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                "Failed to validate token".to_string(),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!("Supabase auth validation failed: {} - {}", status, body);
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid or expired token".to_string(),
        ));
    }

    #[derive(Deserialize)]
    struct SupabaseUser {
        id: Uuid,
        email: Option<String>,
    }

    let user: SupabaseUser = resp.json().await.map_err(|e| {
        error!("Failed to parse Supabase user response: {}", e);
        (
            StatusCode::BAD_GATEWAY,
            "Invalid response from auth service".to_string(),
        )
    })?;

    Ok(AuthUser {
        id: user.id,
        email: user.email,
    })
}

/// Extract Bearer token from Authorization header
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn track_auth_event(
    store: &Arc<AccountStore>,
    event_name: &str,
    account_id: Option<Uuid>,
    auth_user_id: Option<Uuid>,
    event_key: Option<String>,
    route_path: Option<&str>,
    properties: serde_json::Value,
) {
    let environment = std::env::var("DEPLOY_TARGET")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "production".to_string());
    let insert = AnalyticsEventInsert {
        event_name: event_name.to_string(),
        source: "server".to_string(),
        event_timestamp: Utc::now(),
        account_id,
        auth_user_id,
        anonymous_id: None,
        session_id: None,
        workspace_id: account_id.map(|id| id.to_string()),
        org_id: None,
        plan_type: None,
        environment: Some(environment),
        app_version: None,
        page_path: None,
        route_path: route_path.map(|value| value.to_string()),
        referrer: None,
        utm_source: None,
        utm_medium: None,
        utm_campaign: None,
        utm_term: None,
        utm_content: None,
        device_type: None,
        browser: None,
        os: None,
        event_key,
        properties,
    };
    store.record_analytics_event_detached(insert, "auth");
}

fn install_onboarding_identifier_type(platform: InstallPlatform) -> &'static str {
    match platform {
        InstallPlatform::Slack => "slack",
        InstallPlatform::Discord => "discord",
    }
}

fn load_linked_owner_identifier(
    store: &AccountStore,
    account_id: Uuid,
    platform: InstallPlatform,
) -> Result<Option<String>, AccountStoreError> {
    let identifier_type = install_onboarding_identifier_type(platform);
    Ok(store
        .list_identifiers(account_id)?
        .into_iter()
        .find(|identifier| {
            identifier.verified
                && identifier
                    .identifier_type
                    .eq_ignore_ascii_case(identifier_type)
        })
        .map(|identifier| identifier.identifier))
}

fn linked_owner_identifier_source(linked_owner_identifier: &Option<String>) -> Option<String> {
    linked_owner_identifier
        .as_ref()
        .map(|_| "linked_account_owner".to_string())
}

fn oauth_callback_event_nonce(code: &str) -> String {
    let digest = Sha256::digest(code.as_bytes());
    hex::encode(digest)[..16].to_string()
}

fn build_install_onboarding_event_key(
    event_kind: &str,
    account_id: Uuid,
    workspace_id: &str,
    event_nonce: &str,
) -> String {
    format!("{event_kind}:{account_id}:{workspace_id}:{event_nonce}")
}

fn build_slack_install_onboarding_request(
    account_id: Uuid,
    auth_user_id: Uuid,
    installation: &SlackInstallation,
    linked_owner_identifier: Option<String>,
    event_nonce: &str,
) -> InstallOnboardingRequest {
    InstallOnboardingRequest {
        account_id,
        auth_user_id: Some(auth_user_id),
        platform: InstallPlatform::Slack,
        workspace_id: installation.team_id.clone(),
        workspace_name: installation.team_name.clone(),
        installer_identifier: None,
        installer_identifier_source: None,
        linked_owner_identifier_source: linked_owner_identifier_source(&linked_owner_identifier),
        linked_owner_identifier,
        public_channel_hint: None,
        event_key: Some(build_install_onboarding_event_key(
            "slack_bot_installed",
            account_id,
            &installation.team_id,
            event_nonce,
        )),
        route_path: Some("/auth/slack/bot-callback".to_string()),
        trigger: InstallOnboardingTrigger::InstallSuccess,
        force: false,
    }
}

fn build_discord_install_onboarding_request(
    account_id: Uuid,
    auth_user_id: Uuid,
    guild_id: &str,
    guild_name: Option<String>,
    linked_owner_identifier: Option<String>,
    event_nonce: &str,
) -> InstallOnboardingRequest {
    InstallOnboardingRequest {
        account_id,
        auth_user_id: Some(auth_user_id),
        platform: InstallPlatform::Discord,
        workspace_id: guild_id.to_string(),
        workspace_name: guild_name,
        installer_identifier: None,
        installer_identifier_source: None,
        linked_owner_identifier_source: linked_owner_identifier_source(&linked_owner_identifier),
        linked_owner_identifier,
        public_channel_hint: None,
        event_key: Some(build_install_onboarding_event_key(
            "discord_bot_installed",
            account_id,
            guild_id,
            event_nonce,
        )),
        route_path: Some("/auth/discord/bot-callback".to_string()),
        trigger: InstallOnboardingTrigger::InstallSuccess,
        force: false,
    }
}

fn build_reconnect_install_onboarding_request(
    account_id: Uuid,
    auth_user_id: Uuid,
    platform: InstallPlatform,
    state: &ChannelInstallOnboardingState,
    linked_owner_identifier: Option<String>,
    event_nonce: &str,
) -> InstallOnboardingRequest {
    let route_path = match platform {
        InstallPlatform::Slack => "/auth/slack/callback",
        InstallPlatform::Discord => "/auth/discord/callback",
    };

    InstallOnboardingRequest {
        account_id,
        auth_user_id: Some(auth_user_id),
        platform,
        workspace_id: state.workspace_id.clone(),
        workspace_name: state.workspace_name.clone(),
        installer_identifier: state.installer_identifier.clone(),
        installer_identifier_source: state.installer_identifier_source.clone(),
        linked_owner_identifier_source: linked_owner_identifier_source(&linked_owner_identifier),
        linked_owner_identifier,
        public_channel_hint: state.public_channel_id.clone(),
        event_key: Some(build_install_onboarding_event_key(
            &format!("{}_reconnected", platform.as_str()),
            account_id,
            &state.workspace_id,
            event_nonce,
        )),
        route_path: Some(route_path.to_string()),
        trigger: InstallOnboardingTrigger::ReconnectSuccess,
        force: false,
    }
}

fn build_manual_resend_install_onboarding_request(
    account_id: Uuid,
    auth_user_id: Uuid,
    platform: InstallPlatform,
    state: &ChannelInstallOnboardingState,
    linked_owner_identifier: Option<String>,
    force: bool,
) -> InstallOnboardingRequest {
    InstallOnboardingRequest {
        account_id,
        auth_user_id: Some(auth_user_id),
        platform,
        workspace_id: state.workspace_id.clone(),
        workspace_name: state.workspace_name.clone(),
        installer_identifier: state.installer_identifier.clone(),
        installer_identifier_source: state.installer_identifier_source.clone(),
        linked_owner_identifier_source: linked_owner_identifier_source(&linked_owner_identifier),
        linked_owner_identifier,
        public_channel_hint: state.public_channel_id.clone(),
        event_key: Some(format!(
            "manual_resend:{}:{}:{}",
            platform.as_str(),
            state.workspace_id,
            Uuid::new_v4()
        )),
        route_path: Some("/api/channel-install-onboarding/resend".to_string()),
        trigger: InstallOnboardingTrigger::ManualResend,
        force,
    }
}

fn execute_slack_install_onboarding(
    state: &AuthState,
    account_id: Uuid,
    auth_user_id: Uuid,
    installation: SlackInstallation,
    event_nonce: &str,
) -> Result<InstallOnboardingRunResult, String> {
    let linked_owner_identifier =
        load_linked_owner_identifier(&state.account_store, account_id, InstallPlatform::Slack)
            .map_err(|err| format!("failed to resolve linked Slack owner: {err}"))?;
    let request = build_slack_install_onboarding_request(
        account_id,
        auth_user_id,
        &installation,
        linked_owner_identifier,
        event_nonce,
    );
    let state_store = AccountStoreInstallOnboardingStateStore::new(state.account_store.clone());
    let client = SlackInstallOnboardingClient::new(installation);
    run_install_onboarding(
        &state.install_onboarding_config,
        &state_store,
        &state.account_store,
        &client,
        request,
    )
}

fn execute_discord_install_onboarding(
    state: &AuthState,
    account_id: Uuid,
    auth_user_id: Uuid,
    guild_id: String,
    guild_name: Option<String>,
    event_nonce: &str,
) -> Result<InstallOnboardingRunResult, String> {
    let bot_token = state
        .discord_bot_token
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "discord bot token not configured".to_string())?;
    let linked_owner_identifier =
        load_linked_owner_identifier(&state.account_store, account_id, InstallPlatform::Discord)
            .map_err(|err| format!("failed to resolve linked Discord owner: {err}"))?;
    let request = build_discord_install_onboarding_request(
        account_id,
        auth_user_id,
        &guild_id,
        guild_name.clone(),
        linked_owner_identifier,
        event_nonce,
    );
    let state_store = AccountStoreInstallOnboardingStateStore::new(state.account_store.clone());
    let client = DiscordInstallOnboardingClient::new(guild_id, guild_name, bot_token);
    run_install_onboarding(
        &state.install_onboarding_config,
        &state_store,
        &state.account_store,
        &client,
        request,
    )
}

fn execute_platform_reconnect_onboarding(
    state: &AuthState,
    account_id: Uuid,
    auth_user_id: Uuid,
    platform: InstallPlatform,
    event_nonce: &str,
) -> Result<usize, String> {
    let onboarding_states = state
        .account_store
        .list_channel_install_onboarding_states(account_id, platform.as_str())
        .map_err(|err| format!("failed to list onboarding states: {err}"))?;

    if onboarding_states.is_empty() {
        return Ok(0);
    }

    let linked_owner_identifier =
        load_linked_owner_identifier(&state.account_store, account_id, platform)
            .map_err(|err| format!("failed to resolve linked owner: {err}"))?;
    let state_store = AccountStoreInstallOnboardingStateStore::new(state.account_store.clone());
    let mut attempted = 0usize;

    for onboarding_state in onboarding_states {
        let request = build_reconnect_install_onboarding_request(
            account_id,
            auth_user_id,
            platform,
            &onboarding_state,
            linked_owner_identifier.clone(),
            event_nonce,
        );

        let run_result = match platform {
            InstallPlatform::Slack => {
                let installation = match state
                    .slack_store
                    .get_installation_or_env(&onboarding_state.workspace_id)
                {
                    Ok(installation) => installation,
                    Err(err) => {
                        warn!(
                            "skipping Slack reconnect onboarding for account {} workspace {}: {}",
                            account_id, onboarding_state.workspace_id, err
                        );
                        continue;
                    }
                };
                let client = SlackInstallOnboardingClient::new(installation);
                run_install_onboarding(
                    &state.install_onboarding_config,
                    &state_store,
                    &state.account_store,
                    &client,
                    request,
                )
            }
            InstallPlatform::Discord => {
                let bot_token = match state
                    .discord_bot_token
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                {
                    Some(token) => token,
                    None => {
                        warn!(
                            "skipping Discord reconnect onboarding for account {} guild {}: discord bot token not configured",
                            account_id, onboarding_state.workspace_id
                        );
                        continue;
                    }
                };
                let client = DiscordInstallOnboardingClient::new(
                    onboarding_state.workspace_id.clone(),
                    onboarding_state.workspace_name.clone(),
                    bot_token,
                );
                run_install_onboarding(
                    &state.install_onboarding_config,
                    &state_store,
                    &state.account_store,
                    &client,
                    request,
                )
            }
        };

        match run_result {
            Ok(result) => {
                attempted += 1;
                info!(
                    "Reconnect onboarding processed for account {} {} {} (public={}, dm={})",
                    account_id,
                    platform.as_str(),
                    onboarding_state.workspace_id,
                    result.public_status.as_str(),
                    result.dm_status.as_str()
                );
            }
            Err(err) => warn!(
                "Reconnect onboarding failed for account {} {} {}: {}",
                account_id,
                platform.as_str(),
                onboarding_state.workspace_id,
                err
            ),
        }
    }

    Ok(attempted)
}

fn execute_manual_install_onboarding_resend(
    state: &AuthState,
    account_id: Uuid,
    auth_user_id: Uuid,
    platform: InstallPlatform,
    workspace_id: &str,
    force: bool,
) -> Result<InstallOnboardingRunResult, (StatusCode, String)> {
    let onboarding_state = state
        .account_store
        .get_channel_install_onboarding_state(account_id, platform.as_str(), workspace_id)
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to load onboarding state: {err}"),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!(
                    "no onboarding state found for {} workspace {}",
                    platform.as_str(),
                    workspace_id
                ),
            )
        })?;

    let linked_owner_identifier =
        load_linked_owner_identifier(&state.account_store, account_id, platform).map_err(
            |err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to resolve linked owner identifier: {err}"),
                )
            },
        )?;
    let request = build_manual_resend_install_onboarding_request(
        account_id,
        auth_user_id,
        platform,
        &onboarding_state,
        linked_owner_identifier,
        force,
    );
    let state_store = AccountStoreInstallOnboardingStateStore::new(state.account_store.clone());

    match platform {
        InstallPlatform::Slack => {
            let installation = state
                .slack_store
                .get_installation_or_env(&onboarding_state.workspace_id)
                .map_err(|err| {
                    (
                        StatusCode::BAD_GATEWAY,
                        format!("failed to resolve Slack installation: {err}"),
                    )
                })?;
            let client = SlackInstallOnboardingClient::new(installation);
            run_install_onboarding(
                &state.install_onboarding_config,
                &state_store,
                &state.account_store,
                &client,
                request,
            )
            .map_err(|err| (StatusCode::BAD_GATEWAY, err))
        }
        InstallPlatform::Discord => {
            let bot_token = state
                .discord_bot_token
                .clone()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "discord bot token not configured".to_string(),
                    )
                })?;
            let client = DiscordInstallOnboardingClient::new(
                onboarding_state.workspace_id.clone(),
                onboarding_state.workspace_name.clone(),
                bot_token,
            );
            run_install_onboarding(
                &state.install_onboarding_config,
                &state_store,
                &state.account_store,
                &client,
                request,
            )
            .map_err(|err| (StatusCode::BAD_GATEWAY, err))
        }
    }
}

fn json_error_response(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": message
        })),
    )
        .into_response()
}

async fn load_account_for_auth_user(
    state: &AuthState,
    auth_user_id: Uuid,
) -> Result<crate::account_store::Account, Response> {
    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        })?;

    match account_result {
        Ok(Some(account)) => Ok(account),
        Err(e) => {
            error!("Failed to get account: {}", e);
            Err(json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error",
            ))
        }
        Ok(None) => Err(json_error_response(
            StatusCode::NOT_FOUND,
            "Account not found. Please sign up first.",
        )),
    }
}

async fn load_account_identifiers(
    state: &AuthState,
    account_id: Uuid,
) -> Result<Vec<crate::account_store::AccountIdentifier>, Response> {
    let store = state.account_store.clone();
    let identifiers_result = task::spawn_blocking(move || store.list_identifiers(account_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        })?;

    identifiers_result.map_err(|e| {
        error!("Failed to list identifiers: {}", e);
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
    })
}

fn legacy_routine_lookup_identifiers(
    identifiers: &[crate::account_store::AccountIdentifier],
) -> Vec<(String, String)> {
    identifiers
        .iter()
        .filter(|identifier| identifier.verified)
        .map(|identifier| {
            (
                identifier.identifier_type.clone(),
                identifier.identifier.clone(),
            )
        })
        .collect()
}

fn provider_capabilities_from_state(
    state: &AuthState,
) -> crate::service::startup_workspace::ProviderCapabilitySnapshot {
    derive_provider_capabilities(&ProviderCapabilityInputs {
        github_oauth_ready: oauth_ready(
            &state.github_client_id,
            &state.github_client_secret,
            &state.github_redirect_uri,
        ),
        google_docs_runtime_ready: env_flag_enabled("GOOGLE_DOCS_ENABLED")
            || GoogleAuthConfig::from_env().is_valid(),
        email_outbound_ready: env_has_value("POSTMARK_SERVER_TOKEN"),
        slack_oauth_ready: oauth_ready(
            &state.slack_client_id,
            &state.slack_client_secret,
            &state.slack_redirect_uri,
        ),
        slack_bot_ready: env_has_value("SLACK_BOT_TOKEN"),
        discord_oauth_ready: oauth_ready(
            &state.discord_client_id,
            &state.discord_client_secret,
            &state.discord_redirect_uri,
        ),
        discord_bot_ready: env_has_value("DISCORD_BOT_TOKEN"),
    })
}

fn provider_runtime_from_identifiers(
    state: &AuthState,
    identifiers: &[crate::account_store::AccountIdentifier],
) -> WorkspaceProviderRuntimeState {
    let identifier_snapshots = identifiers
        .iter()
        .map(|identifier| LinkedIdentifierSnapshot {
            identifier_type: identifier.identifier_type.clone(),
            identifier: identifier.identifier.clone(),
            verified: identifier.verified,
        })
        .collect::<Vec<_>>();

    WorkspaceProviderRuntimeState {
        has_account: true,
        capabilities: provider_capabilities_from_state(state),
        connected: derive_provider_connections(&identifier_snapshots),
    }
}

async fn try_load_unified_account_tasks(
    state: &AuthState,
    account_id: Uuid,
) -> Vec<TaskStatusSummary> {
    let task_paths = match load_unified_account_task_paths(state, account_id).await {
        Ok(paths) => paths,
        Err(_) => return Vec::new(),
    };

    let mut tasks = Vec::new();
    for task_path in task_paths {
        tasks = merge_task_summaries(tasks, load_tasks_with_status(&task_path));
    }
    sort_task_summaries(&mut tasks);
    tasks
}

fn sort_task_summaries(tasks: &mut [TaskStatusSummary]) {
    tasks.sort_by(|left, right| {
        parse_rfc3339_utc(Some(right.created_at.as_str()))
            .cmp(&parse_rfc3339_utc(Some(left.created_at.as_str())))
            .then_with(|| task_latest_activity_at(right).cmp(&task_latest_activity_at(left)))
    });
}

fn merge_task_summaries(
    mut base: Vec<TaskStatusSummary>,
    incoming: Vec<TaskStatusSummary>,
) -> Vec<TaskStatusSummary> {
    for candidate in incoming {
        if let Some(existing_idx) = base.iter().position(|task| task.id == candidate.id) {
            if should_prefer_task_summary(&candidate, &base[existing_idx]) {
                base[existing_idx] = candidate;
            }
        } else {
            base.push(candidate);
        }
    }
    base
}

fn should_prefer_task_summary(candidate: &TaskStatusSummary, existing: &TaskStatusSummary) -> bool {
    let candidate_rank = task_status_rank(candidate.status.as_str());
    let existing_rank = task_status_rank(existing.status.as_str());
    if candidate_rank != existing_rank {
        return candidate_rank > existing_rank;
    }

    let candidate_activity = task_latest_activity_at(candidate);
    let existing_activity = task_latest_activity_at(existing);
    if candidate_activity != existing_activity {
        return candidate_activity > existing_activity;
    }

    let candidate_error = candidate.error_message.as_deref().unwrap_or("").trim();
    let existing_error = existing.error_message.as_deref().unwrap_or("").trim();
    if !candidate_error.is_empty() && existing_error.is_empty() {
        return true;
    }

    false
}

fn task_status_rank(status: &str) -> i32 {
    match status {
        "failed" | "success" | "cancelled" | "expired" | "superseded" => 5,
        "cancellation_requested" => 4,
        "running" | "retry_scheduled" => 3,
        "queued" => 2,
        "scheduled" | "paused" => 1,
        _ => 0,
    }
}

fn task_latest_activity_at(task: &TaskStatusSummary) -> Option<DateTime<Utc>> {
    parse_rfc3339_utc(task.status_changed_at.as_deref())
        .or_else(|| parse_rfc3339_utc(task.execution_started_at.as_deref()))
        .or_else(|| parse_rfc3339_utc(task.last_run.as_deref()))
        .or_else(|| parse_rfc3339_utc(task.run_at.as_deref()))
        .or_else(|| parse_rfc3339_utc(task.next_run.as_deref()))
        .or_else(|| parse_rfc3339_utc(Some(task.created_at.as_str())))
}

fn load_task_statuses_or_response(
    tasks_db_path: &std::path::Path,
    scope_label: &str,
) -> Result<Vec<TaskStatusSummary>, Response> {
    try_load_tasks_with_status_shared(tasks_db_path).map_err(|err| {
        error!(
            "Failed to load {scope_label} tasks from {}: {}",
            tasks_db_path.display(),
            err
        );
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load tasks")
    })
}

fn load_routine_summaries_or_response(
    tasks_db_path: &std::path::Path,
    scope_label: &str,
) -> Result<Vec<RoutineSummary>, Response> {
    try_load_routines_with_status(tasks_db_path).map_err(|err| {
        error!(
            "Failed to load {scope_label} routines from {}: {}",
            tasks_db_path.display(),
            err
        );
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load routines")
    })
}

async fn load_authenticated_account_from_headers(
    state: &AuthState,
    headers: &HeaderMap,
) -> Result<crate::account_store::Account, Response> {
    let token = extract_bearer_token(headers).ok_or_else(|| {
        json_error_response(StatusCode::UNAUTHORIZED, "Missing Authorization header")
    })?;
    let auth_user = validate_supabase_token(&state.supabase_url, &token)
        .await
        .map_err(|(status, msg)| json_error_response(status, &msg))?;
    load_account_for_auth_user(state, auth_user.id).await
}

async fn load_unified_account_task_paths(
    state: &AuthState,
    account_id: Uuid,
) -> Result<Vec<PathBuf>, Response> {
    let (Some(user_store), Some(users_root)) = (&state.user_store, &state.users_root) else {
        return Err(json_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Task storage not configured",
        ));
    };

    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    let account_tasks_db_path = users_root
        .join(account_id.to_string())
        .join("state")
        .join("tasks.db");
    push_unique_task_path(&mut paths, &mut seen, account_tasks_db_path);

    let identifiers = load_account_identifiers(state, account_id).await?;
    // Older scheduled child routines can exist only in legacy per-user storage even when
    // the account-level dashboard mirror is missing. Check every verified identifier so
    // existing routines remain visible and routine mutations can reach the live task copy.
    for (identifier_type, identifier) in legacy_routine_lookup_identifiers(&identifiers) {
        let user_store_clone = user_store.clone();
        let user_result = task::spawn_blocking(move || {
            user_store_clone.get_user_by_identifier(&identifier_type, &identifier)
        })
        .await;

        if let Ok(Ok(Some(user_record))) = user_result {
            let user_paths = user_store.user_paths(users_root, &user_record.user_id);
            push_unique_task_path(&mut paths, &mut seen, user_paths.tasks_db_path);
        }
    }

    Ok(paths)
}

fn push_unique_task_path(paths: &mut Vec<PathBuf>, seen: &mut HashSet<String>, path: PathBuf) {
    let key = path.to_string_lossy().to_string();
    if seen.insert(key) {
        paths.push(path);
    }
}

fn merge_routine_summaries(
    mut base: Vec<RoutineSummary>,
    incoming: Vec<RoutineSummary>,
) -> Vec<RoutineSummary> {
    for candidate in incoming {
        if let Some(existing_idx) = base.iter().position(|routine| routine.id == candidate.id) {
            if should_prefer_routine_summary(&candidate, &base[existing_idx]) {
                base[existing_idx] = candidate;
            }
        } else {
            base.push(candidate);
        }
    }
    base
}

fn should_prefer_routine_summary(candidate: &RoutineSummary, existing: &RoutineSummary) -> bool {
    let candidate_rank = routine_status_rank(candidate.execution_status.as_deref());
    let existing_rank = routine_status_rank(existing.execution_status.as_deref());
    if candidate_rank != existing_rank {
        return candidate_rank > existing_rank;
    }

    let candidate_activity = routine_latest_activity_at(candidate);
    let existing_activity = routine_latest_activity_at(existing);
    if candidate_activity != existing_activity {
        return candidate_activity > existing_activity;
    }

    let candidate_error = candidate.error_message.as_deref().unwrap_or("").trim();
    let existing_error = existing.error_message.as_deref().unwrap_or("").trim();
    if !candidate_error.is_empty() && existing_error.is_empty() {
        return true;
    }

    false
}

fn routine_status_rank(status: Option<&str>) -> i32 {
    match status {
        Some("success") | Some("failed") | Some("superseded") => 3,
        Some("running") => 2,
        Some(_) => 1,
        None => 0,
    }
}

fn partition_routines(mut routines: Vec<RoutineSummary>) -> RoutinesResponse {
    let mut active = Vec::new();
    let mut history = Vec::new();

    for routine in routines.drain(..) {
        if routine.enabled {
            active.push(routine);
        } else {
            history.push(routine);
        }
    }

    active.sort_by(|left, right| {
        let left_next = routine_next_occurrence_at(left);
        let right_next = routine_next_occurrence_at(right);
        left_next
            .cmp(&right_next)
            .then_with(|| routine_created_at(right).cmp(&routine_created_at(left)))
    });

    history.sort_by(|left, right| {
        routine_latest_activity_at(right).cmp(&routine_latest_activity_at(left))
    });
    history.truncate(50);

    RoutinesResponse { active, history }
}

fn routine_next_occurrence_at(routine: &RoutineSummary) -> Option<DateTime<Utc>> {
    parse_rfc3339_utc(routine.next_run.as_deref())
        .or_else(|| parse_rfc3339_utc(routine.run_at.as_deref()))
}

fn routine_latest_activity_at(routine: &RoutineSummary) -> Option<DateTime<Utc>> {
    parse_rfc3339_utc(routine.last_run.as_deref())
        .or_else(|| parse_rfc3339_utc(routine.run_at.as_deref()))
        .or_else(|| parse_rfc3339_utc(Some(routine.created_at.as_str())))
}

fn routine_created_at(routine: &RoutineSummary) -> Option<DateTime<Utc>> {
    parse_rfc3339_utc(Some(routine.created_at.as_str()))
}

fn parse_rfc3339_utc(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

async fn try_load_unified_account_routines(
    state: &AuthState,
    account_id: Uuid,
) -> Result<RoutinesResponse, Response> {
    let task_paths = load_unified_account_task_paths(state, account_id).await?;
    let mut routines = Vec::new();
    for task_path in task_paths {
        // Run sync MongoDB I/O on blocking thread to avoid blocking async runtime
        let path = task_path.clone();
        let loaded = task::spawn_blocking(move || try_load_routines_with_status(&path))
            .await
            .map_err(|e| {
                error!("spawn_blocking panicked loading routines: {}", e);
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load routines")
            })?
            .map_err(|err| {
                error!(
                    "Failed to load account-scoped routines from {}: {}",
                    task_path.display(),
                    err
                );
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load routines")
            })?;
        routines = merge_routine_summaries(routines, loaded);
    }
    Ok(partition_routines(routines))
}

#[derive(Debug, Clone, Copy)]
enum RoutineMutationAction {
    Pause,
    Resume,
    Delete,
}

fn mutate_routine_task(
    task: &ScheduledTask,
    action: RoutineMutationAction,
    now: DateTime<Utc>,
) -> Result<ScheduledTask, String> {
    match action {
        RoutineMutationAction::Pause | RoutineMutationAction::Delete => {
            let mut updated = task.clone();
            updated.enabled = false;
            Ok(updated)
        }
        RoutineMutationAction::Resume => {
            prepare_task_for_resume(task, now).map_err(|err| err.to_string())
        }
    }
}

async fn mutate_unified_account_routine(
    state: &AuthState,
    account_id: Uuid,
    task_id: &str,
    action: RoutineMutationAction,
) -> Result<bool, Response> {
    let task_paths = load_unified_account_task_paths(state, account_id).await?;
    let task_id = task_id.to_string();
    let action_name = match action {
        RoutineMutationAction::Pause => "pause",
        RoutineMutationAction::Resume => "resume",
        RoutineMutationAction::Delete => "delete",
    }
    .to_string();
    let task_id_for_log = task_id.clone();
    let action_name_for_log = action_name.clone();

    task::spawn_blocking(move || {
        mutate_unified_account_routine_blocking(&task_paths, &task_id, action, &action_name)
    })
    .await
    .map_err(|err| {
        error!(
            "spawn_blocking panicked while applying routine action {} to {}: {}",
            action_name_for_log, task_id_for_log, err
        );
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    })?
}

fn mutate_unified_account_routine_blocking(
    task_paths: &[PathBuf],
    task_id: &str,
    action: RoutineMutationAction,
    action_name: &str,
) -> Result<bool, Response> {
    let now = Utc::now();
    let mut updates = Vec::new();

    for task_path in task_paths {
        let task = load_scheduled_task(task_path, task_id).map_err(|err| {
            error!(
                "failed to load task {} from {} for routine {}: {}",
                task_id,
                task_path.display(),
                action_name,
                err
            );
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load routine")
        })?;

        let Some(task) = task else {
            continue;
        };

        if !is_user_visible_routine_task(&task, now) {
            continue;
        }

        let updated = mutate_routine_task(&task, action, now).map_err(|message| {
            json_error_response(
                StatusCode::CONFLICT,
                &format!("Routine cannot be {}d safely: {}", action_name, message),
            )
        })?;
        updates.push((task_path.clone(), updated));
    }

    if updates.is_empty() {
        return Ok(false);
    }

    for (task_path, updated_task) in updates {
        persist_scheduled_task(&task_path, &updated_task).map_err(|err| {
            error!(
                "failed to persist routine {} to {} during {}: {}",
                updated_task.id,
                task_path.display(),
                action_name,
                err
            );
            json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update routine",
            )
        })?;
    }

    Ok(true)
}

// ============================================================================
// Signup
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SignupResponse {
    pub account_id: Uuid,
    pub auth_user_id: Uuid,
    pub created: bool,
    pub organization_id: Option<Uuid>,
    pub organization_name: Option<String>,
}

/// POST /auth/signup
/// Creates a DoWhiz account for the authenticated Supabase user.
/// Requires: Authorization: Bearer <supabase_access_token>
pub async fn signup(State(state): State<AuthState>, headers: HeaderMap) -> impl IntoResponse {
    let auth_method = headers
        .get("x-dowhiz-auth-method")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };
    let auth_user_id = auth_user.id;
    let auth_email = auth_user.email.clone();

    // Check if account already exists (run on blocking thread)
    let store = state.account_store.clone();
    let existing = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let existing = match existing {
        Ok(Ok(existing)) => existing,
        Ok(Err(e)) => {
            error!("Failed to check existing account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    if let Some(existing) = existing {
        info!("Account already exists for auth_user_id={}", auth_user_id);
        track_auth_event(
            &state.account_store,
            "signup_completed",
            Some(existing.id),
            Some(existing.auth_user_id),
            Some(format!("signup:{}", existing.id)),
            Some("/auth/signup"),
            serde_json::json!({
                "created": false,
                "auth_method": auth_method,
            }),
        );
        track_auth_event(
            &state.account_store,
            "first_authenticated_session",
            Some(existing.id),
            Some(existing.auth_user_id),
            Some(format!("first_authenticated_session:{}", existing.id)),
            Some("/auth/signup"),
            serde_json::json!({
                "created": false,
                "auth_method": auth_method,
            }),
        );
        track_auth_event(
            &state.account_store,
            "workspace_created",
            Some(existing.id),
            Some(existing.auth_user_id),
            Some(format!("workspace:{}", existing.id)),
            Some("/auth/signup"),
            serde_json::json!({
                "workspace_type": "account_workspace",
                "created": false,
            }),
        );

        // Fetch organization name if account has one
        let organization_name = if let Some(org_id) = existing.organization_id {
            let store = state.account_store.clone();
            match task::spawn_blocking(move || store.get_organization_by_id(org_id)).await {
                Ok(Ok(Some(org))) => Some(org.name),
                _ => None,
            }
        } else {
            None
        };

        return (
            StatusCode::OK,
            Json(SignupResponse {
                account_id: existing.id,
                auth_user_id: existing.auth_user_id,
                created: false,
                organization_id: existing.organization_id,
                organization_name,
            }),
        )
            .into_response();
    }

    // Create new account (run on blocking thread)
    let store = state.account_store.clone();
    let result = task::spawn_blocking(move || store.create_account(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    match result {
        Ok(Ok(account)) => {
            info!(
                "Created account {} for auth_user_id={}",
                account.id, auth_user_id
            );
            track_auth_event(
                &state.account_store,
                "signup_completed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!("signup:{}", account.id)),
                Some("/auth/signup"),
                serde_json::json!({
                    "created": true,
                    "auth_method": auth_method,
                }),
            );
            track_auth_event(
                &state.account_store,
                "first_authenticated_session",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!("first_authenticated_session:{}", account.id)),
                Some("/auth/signup"),
                serde_json::json!({
                    "created": true,
                    "auth_method": auth_method,
                }),
            );
            track_auth_event(
                &state.account_store,
                "workspace_created",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!("workspace:{}", account.id)),
                Some("/auth/signup"),
                serde_json::json!({
                    "workspace_type": "account_workspace",
                    "created": true,
                }),
            );

            // Auto-link the auth email as an identifier
            if let Some(email) = auth_email {
                let store = state.account_store.clone();
                let account_id = account.id;
                let email_clone = email.clone();
                let link_result = task::spawn_blocking(move || {
                    store.create_identifier(account_id, "email", &email_clone)
                })
                .await;

                match link_result {
                    Ok(Ok(_)) => {
                        info!("Auto-linked email {} to account {}", email, account.id);
                    }
                    Ok(Err(AccountStoreError::IdentifierTaken)) => {
                        warn!("Email {} already linked to another account", email);
                    }
                    Ok(Err(e)) => {
                        warn!("Failed to auto-link email {}: {}", email, e);
                    }
                    Err(e) => {
                        warn!("spawn_blocking panicked during email link: {}", e);
                    }
                }
            }

            (
                StatusCode::CREATED,
                Json(SignupResponse {
                    account_id: account.id,
                    auth_user_id: account.auth_user_id,
                    created: true,
                    organization_id: None,
                    organization_name: None,
                }),
            )
                .into_response()
        }
        Ok(Err(e)) => {
            error!("Failed to create account: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to create account"
                })),
            )
                .into_response()
        }
        Err(resp) => resp.into_response(),
    }
}

// ============================================================================
// Get Account
// ============================================================================

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub account_id: Uuid,
    pub auth_user_id: Uuid,
    pub identifiers: Vec<IdentifierResponse>,
    pub tokens_to_hours: Option<f64>,
    pub organization_id: Option<Uuid>,
    pub organization_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IdentifierResponse {
    pub identifier_type: String,
    pub identifier: String,
    pub verified: bool,
}

/// GET /auth/account
/// Returns the current user's account and linked identifiers.
pub async fn get_account(State(state): State<AuthState>, headers: HeaderMap) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };

    // Get account (run on blocking thread)
    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Account not found. Please sign up first."
                })),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    // List identifiers (run on blocking thread)
    let account_id = account.id;
    let store = state.account_store.clone();
    let identifiers_result = task::spawn_blocking(move || store.list_identifiers(account_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let identifiers = match identifiers_result {
        Ok(Ok(ids)) => ids,
        Ok(Err(e)) => {
            error!("Failed to list identifiers: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    // Fetch organization name if account has one
    let organization_name = if let Some(org_id) = account.organization_id {
        let store = state.account_store.clone();
        match task::spawn_blocking(move || store.get_organization_by_id(org_id)).await {
            Ok(Ok(Some(org))) => Some(org.name),
            _ => None,
        }
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(AccountResponse {
            account_id: account.id,
            auth_user_id: account.auth_user_id,
            identifiers: identifiers
                .into_iter()
                .map(|i| IdentifierResponse {
                    identifier_type: i.identifier_type,
                    identifier: i.identifier,
                    verified: i.verified,
                })
                .collect(),
            tokens_to_hours: account.tokens_to_hours,
            organization_id: account.organization_id,
            organization_name,
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct SetOrganizationRequest {
    pub organization_name: String,
    /// Optional cron expression for TPM sync schedule (default: "0 0 9 * * MON-FRI")
    pub cron_expr: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ListOrganizationsQuery {
    pub search: Option<String>,
}

/// POST /auth/organization - Create a new organization
pub async fn create_organization(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<CreateOrganizationRequest>,
) -> impl IntoResponse {
    // Require authentication
    if let Err(response) = load_authenticated_account_from_headers(&state, &headers).await {
        return response;
    }

    let store = state.account_store.clone();
    let name = payload.name.clone();

    let result = task::spawn_blocking(move || store.create_organization(&name))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    match result {
        Ok(Ok(org)) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": org.id,
                "name": org.name,
                "notion_database_id": org.notion_database_id,
                "notion_workspace_id": org.notion_workspace_id,
                "created_at": org.created_at,
            })),
        )
            .into_response(),
        Ok(Err(crate::account_store::AccountStoreError::AlreadyExists(msg))) => {
            json_error_response(StatusCode::CONFLICT, &msg)
        }
        Ok(Err(e)) => {
            error!("Failed to create organization: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

/// GET /auth/organizations - List organizations, optionally filtered by search term
pub async fn list_organizations(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Query(query): Query<ListOrganizationsQuery>,
) -> impl IntoResponse {
    // Require authentication
    if let Err(response) = load_authenticated_account_from_headers(&state, &headers).await {
        return response;
    }

    let store = state.account_store.clone();
    let search = query.search.clone();

    let result = task::spawn_blocking(move || store.list_organizations(search.as_deref()))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    match result {
        Ok(Ok(orgs)) => {
            let items: Vec<serde_json::Value> = orgs
                .iter()
                .map(|org| {
                    serde_json::json!({
                        "id": org.id,
                        "name": org.name,
                        "notion_database_id": org.notion_database_id,
                        "notion_workspace_id": org.notion_workspace_id,
                        "leader_account_id": org.leader_account_id,
                        "discord_guild_id": org.discord_guild_id,
                        "slack_team_id": org.slack_team_id,
                        "created_at": org.created_at,
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "organizations": items })),
            )
                .into_response()
        }
        Ok(Err(e)) => {
            error!("Failed to list organizations: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

/// GET /auth/organization/:name/member-count - Get the number of members in an organization
pub async fn get_organization_member_count(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(org_name): Path<String>,
) -> impl IntoResponse {
    // Require authentication
    if let Err(response) = load_authenticated_account_from_headers(&state, &headers).await {
        return response;
    }

    let store = state.account_store.clone();
    let name = org_name.clone();

    let result = task::spawn_blocking(move || store.get_organization_member_count(&name))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    match result {
        Ok(Ok(count)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "organization_name": org_name,
                "member_count": count,
            })),
        )
            .into_response(),
        Ok(Err(AccountStoreError::NotFound)) => json_error_response(
            StatusCode::NOT_FOUND,
            &format!("Organization '{}' not found", org_name),
        ),
        Ok(Err(e)) => {
            error!("Failed to get organization member count: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrganizationDatabaseRequest {
    pub database_id: String,
    /// Notion workspace ID where the database lives (required for multi-workspace setups)
    pub workspace_id: Option<String>,
}

/// PUT /auth/organization/:name/database - Update organization's Notion database ID
///
/// Allows connecting an existing Notion database to an organization for TPM workflows.
/// The user must be a member of the organization to update its database.
pub async fn update_organization_database(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(org_name): Path<String>,
    Json(payload): Json<UpdateOrganizationDatabaseRequest>,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(acc) => acc,
        Err(response) => return response,
    };

    // Verify user belongs to an organization
    let Some(account_org_id) = account.organization_id else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "You must be a member of an organization to update its database",
        );
    };

    // Fetch the organization to verify it exists and user belongs to it
    let store = state.account_store.clone();
    let org_name_clone = org_name.clone();
    let org_result = task::spawn_blocking(move || store.get_organization_by_name(&org_name_clone))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    let org = match org_result {
        Ok(Ok(Some(org))) => org,
        Ok(Ok(None)) => {
            return json_error_response(
                StatusCode::NOT_FOUND,
                &format!("Organization '{}' not found", org_name),
            );
        }
        Ok(Err(e)) => {
            error!("Failed to get organization: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    // Verify user belongs to this organization
    if account_org_id != org.id {
        return json_error_response(
            StatusCode::FORBIDDEN,
            "You are not a member of this organization",
        );
    }

    // Auto-detect workspace_id if not provided
    let database_id = payload.database_id.clone();
    let workspace_id = if let Some(ws_id) = payload.workspace_id.clone() {
        Some(ws_id)
    } else {
        // Try to auto-detect by testing each credential
        let account_id = account.id;
        let db_id = database_id.clone();
        match task::spawn_blocking(move || detect_workspace_for_database(account_id, &db_id)).await
        {
            Ok(Some(ws_id)) => {
                info!(
                    "Auto-detected workspace_id={} for database_id={}",
                    ws_id, database_id
                );
                Some(ws_id)
            }
            Ok(None) => {
                warn!(
                    "Could not auto-detect workspace for database_id={}, no credential had access",
                    database_id
                );
                None
            }
            Err(e) => {
                error!("Failed to auto-detect workspace: {}", e);
                None
            }
        }
    };

    // Update the organization's notion_database_id and workspace_id
    let store = state.account_store.clone();
    let update_result = task::spawn_blocking(move || {
        store.update_organization_notion_config(&org_name, &database_id, workspace_id.as_deref())
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked: {}", e);
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    });

    match update_result {
        Ok(Ok(updated_org)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "organization_name": updated_org.name,
                "notion_database_id": updated_org.notion_database_id,
                "notion_workspace_id": updated_org.notion_workspace_id,
            })),
        )
            .into_response(),
        Ok(Err(e)) => {
            error!("Failed to update organization database: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrganizationLeaderRequest {
    pub leader_account_id: Uuid,
}

/// PUT /auth/organization/:name/leader - Set the organization's leader account
///
/// The leader's Notion credentials are used for all TPM operations in the org.
/// The user must be a member of the organization to update its leader.
pub async fn update_organization_leader(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(org_name): Path<String>,
    Json(payload): Json<UpdateOrganizationLeaderRequest>,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(acc) => acc,
        Err(response) => return response,
    };

    // Verify user belongs to an organization
    let Some(account_org_id) = account.organization_id else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "You must be a member of an organization to update its leader",
        );
    };

    // Fetch the organization to verify it exists and user belongs to it
    let store = state.account_store.clone();
    let org_name_clone = org_name.clone();
    let org_result = task::spawn_blocking(move || store.get_organization_by_name(&org_name_clone))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    let org = match org_result {
        Ok(Ok(Some(org))) => org,
        Ok(Ok(None)) => {
            return json_error_response(
                StatusCode::NOT_FOUND,
                &format!("Organization '{}' not found", org_name),
            );
        }
        Ok(Err(e)) => {
            error!("Failed to get organization: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    // Verify user belongs to this organization
    if account_org_id != org.id {
        return json_error_response(
            StatusCode::FORBIDDEN,
            "You are not a member of this organization",
        );
    }

    // Verify the leader account exists and is a member of this organization
    let store = state.account_store.clone();
    let leader_id = payload.leader_account_id;
    let leader_check = task::spawn_blocking(move || store.get_account(leader_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    match leader_check {
        Ok(Ok(Some(leader_account))) => {
            if leader_account.organization_id != Some(org.id) {
                return json_error_response(
                    StatusCode::BAD_REQUEST,
                    "Leader account must be a member of this organization",
                );
            }
        }
        Ok(Ok(None)) => {
            return json_error_response(StatusCode::BAD_REQUEST, "Leader account does not exist");
        }
        Ok(Err(e)) => {
            error!("Failed to get leader account: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    }

    // Update the leader
    let store = state.account_store.clone();
    let leader_id = payload.leader_account_id;
    let org_name_for_update = org_name.clone();
    let update_result = task::spawn_blocking(move || {
        store.set_organization_leader(&org_name_for_update, leader_id)
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked: {}", e);
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    });

    match update_result {
        Ok(Ok(updated_org)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "organization_name": updated_org.name,
                "leader_account_id": updated_org.leader_account_id,
            })),
        )
            .into_response(),
        Ok(Err(e)) => {
            error!("Failed to update organization leader: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrganizationDiscordRequest {
    pub guild_id: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrganizationSlackRequest {
    pub team_id: String,
}

/// PUT /auth/organization/:name/discord - Set the organization's Discord guild ID
///
/// Used for TPM bug scanning in Discord channels during scheduled syncs.
/// The user must be a member of the organization to update its Discord config.
pub async fn update_organization_discord(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(org_name): Path<String>,
    Json(payload): Json<UpdateOrganizationDiscordRequest>,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(acc) => acc,
        Err(response) => return response,
    };

    let Some(account_org_id) = account.organization_id else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "You must be a member of an organization to update its Discord config",
        );
    };

    let store = state.account_store.clone();
    let org_name_clone = org_name.clone();
    let org_result = task::spawn_blocking(move || store.get_organization_by_name(&org_name_clone))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    let org = match org_result {
        Ok(Ok(Some(org))) => org,
        Ok(Ok(None)) => {
            return json_error_response(
                StatusCode::NOT_FOUND,
                &format!("Organization '{}' not found", org_name),
            );
        }
        Ok(Err(e)) => {
            error!("Failed to get organization: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    if account_org_id != org.id {
        return json_error_response(
            StatusCode::FORBIDDEN,
            "You are not a member of this organization",
        );
    }

    let store = state.account_store.clone();
    let guild_id = payload.guild_id.clone();
    let update_result =
        task::spawn_blocking(move || store.update_organization_discord_guild(&org_name, &guild_id))
            .await
            .map_err(|e| {
                error!("spawn_blocking panicked: {}", e);
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
            });

    match update_result {
        Ok(Ok(updated_org)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "organization_name": updated_org.name,
                "discord_guild_id": updated_org.discord_guild_id,
            })),
        )
            .into_response(),
        Ok(Err(e)) => {
            error!("Failed to update organization Discord config: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

/// PUT /auth/organization/:name/slack - Set the organization's Slack team ID
///
/// Used for TPM notifications in Slack channels.
/// The user must be a member of the organization to update its Slack config.
pub async fn update_organization_slack(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(org_name): Path<String>,
    Json(payload): Json<UpdateOrganizationSlackRequest>,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(acc) => acc,
        Err(response) => return response,
    };

    let Some(account_org_id) = account.organization_id else {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "You must be a member of an organization to update its Slack config",
        );
    };

    let store = state.account_store.clone();
    let org_name_clone = org_name.clone();
    let org_result = task::spawn_blocking(move || store.get_organization_by_name(&org_name_clone))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    let org = match org_result {
        Ok(Ok(Some(org))) => org,
        Ok(Ok(None)) => {
            return json_error_response(
                StatusCode::NOT_FOUND,
                &format!("Organization '{}' not found", org_name),
            );
        }
        Ok(Err(e)) => {
            error!("Failed to get organization: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    if account_org_id != org.id {
        return json_error_response(
            StatusCode::FORBIDDEN,
            "You are not a member of this organization",
        );
    }

    let store = state.account_store.clone();
    let team_id = payload.team_id.clone();
    let update_result =
        task::spawn_blocking(move || store.update_organization_slack_team(&org_name, &team_id))
            .await
            .map_err(|e| {
                error!("spawn_blocking panicked: {}", e);
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
            });

    match update_result {
        Ok(Ok(updated_org)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "organization_name": updated_org.name,
                "slack_team_id": updated_org.slack_team_id,
            })),
        )
            .into_response(),
        Ok(Err(e)) => {
            error!("Failed to update organization Slack config: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

/// Auto-detect which workspace a Notion database belongs to.
///
/// Tries each of the user's Notion credentials to see which one can access the database.
/// Returns the workspace_id of the first credential that succeeds.
fn detect_workspace_for_database(account_id: Uuid, database_id: &str) -> Option<String> {
    let notion_store = match NotionStore::new() {
        Ok(store) => store,
        Err(e) => {
            error!("Failed to create NotionStore: {}", e);
            return None;
        }
    };

    let credentials = match notion_store.get_credentials_for_account(account_id) {
        Ok(creds) => creds,
        Err(e) => {
            error!(
                "Failed to get Notion credentials for account {}: {}",
                account_id, e
            );
            return None;
        }
    };

    if credentials.is_empty() {
        warn!("No Notion credentials found for account {}", account_id);
        return None;
    }

    info!(
        "Trying {} Notion credentials to find workspace for database {}",
        credentials.len(),
        database_id
    );

    // Try each credential to see which one can access the database
    for cred in &credentials {
        let url = format!("https://api.notion.com/v1/databases/{}", database_id);
        let client = reqwest::blocking::Client::new();

        match client
            .get(&url)
            .header("Authorization", format!("Bearer {}", cred.access_token))
            .header("Notion-Version", "2022-06-28")
            .send()
        {
            Ok(resp) if resp.status().is_success() => {
                info!(
                    "Found database {} in workspace {} ({})",
                    database_id,
                    cred.workspace_id,
                    cred.workspace_name.as_deref().unwrap_or("unnamed")
                );
                return Some(cred.workspace_id.clone());
            }
            Ok(resp) => {
                info!(
                    "Workspace {} cannot access database {}: HTTP {}",
                    cred.workspace_id,
                    database_id,
                    resp.status()
                );
            }
            Err(e) => {
                warn!(
                    "Failed to check database {} with workspace {}: {}",
                    database_id, cred.workspace_id, e
                );
            }
        }
    }

    None
}

/// POST /api/tpm/setup-cron - Set up TPM cron job for an organization
///
/// Called when the first member joins an organization. Triggers `tpm_cli setup-tpm-cron`
/// to create a daily sync cron job for the TPM workflow.
pub async fn setup_tpm_cron(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<SetOrganizationRequest>,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(acc) => acc,
        Err(response) => return response,
    };

    let org_name = payload.organization_name.clone();
    let cron_expr = payload.cron_expr.clone();

    // Verify user belongs to this organization
    if account.organization_id.is_none() {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "You must be a member of an organization to set up TPM cron",
        );
    }

    // Fetch the organization to verify name matches
    let store = state.account_store.clone();
    let org_name_clone = org_name.clone();
    let org_result = task::spawn_blocking(move || store.get_organization_by_name(&org_name_clone))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    let org = match org_result {
        Ok(Ok(Some(org))) => org,
        Ok(Ok(None)) => {
            return json_error_response(
                StatusCode::NOT_FOUND,
                &format!("Organization '{}' not found", org_name),
            );
        }
        Ok(Err(e)) => {
            error!("Failed to fetch organization: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    // Verify user is in this organization
    if account.organization_id != Some(org.id) {
        return json_error_response(
            StatusCode::FORBIDDEN,
            &format!("You are not a member of organization '{}'", org_name),
        );
    }

    // Call setup_tpm_cron directly
    let account_id = account.id;
    let org_name_for_cron = org_name.clone();
    let store_clone = state.account_store.clone();

    let user_store = match &state.user_store {
        Some(store) => store.clone(),
        None => {
            return json_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "User store not configured",
            );
        }
    };

    let cron_result = task::spawn_blocking(move || {
        let index_store = IndexStore::new("/tmp/task_index.db")
            .map_err(|e| crate::tpm_cron::TpmCronError::IndexStoreSync(e.to_string()))?;
        crate::tpm_cron::setup_tpm_cron(
            &store_clone,
            &user_store,
            &index_store,
            account_id,
            &org_name_for_cron,
            cron_expr.as_deref(),
        )
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked during setup_tpm_cron: {}", e);
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    });

    match cron_result {
        Ok(Ok(result)) => {
            info!("setup_tpm_cron: succeeded, task_id={}", result.task_id);
            (StatusCode::OK, Json(result)).into_response()
        }
        Ok(Err(e)) => {
            error!("setup_tpm_cron: failed: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
        }
        Err(response) => response,
    }
}

/// POST /api/tpm/trigger-sync - Trigger an immediate TPM sync for an organization
///
/// Creates a one-shot task that runs immediately to sync TPM workflows.
pub async fn trigger_tpm_sync_endpoint(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<SetOrganizationRequest>,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(acc) => acc,
        Err(response) => return response,
    };

    let org_name = payload.organization_name.clone();

    // Verify user belongs to an organization
    if account.organization_id.is_none() {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "You must be a member of an organization to trigger TPM sync",
        );
    }

    // Fetch the organization to verify name matches
    let store = state.account_store.clone();
    let org_name_clone = org_name.clone();
    let org_result = task::spawn_blocking(move || store.get_organization_by_name(&org_name_clone))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    let org = match org_result {
        Ok(Ok(Some(org))) => org,
        Ok(Ok(None)) => {
            return json_error_response(
                StatusCode::NOT_FOUND,
                &format!("Organization '{}' not found", org_name),
            );
        }
        Ok(Err(e)) => {
            error!("Failed to fetch organization: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    // Verify user is in this organization
    if account.organization_id != Some(org.id) {
        return json_error_response(
            StatusCode::FORBIDDEN,
            &format!("You are not a member of organization '{}'", org_name),
        );
    }

    // Call trigger_tpm_sync
    let account_id = account.id;
    let org_name_for_sync = org_name.clone();
    let store_clone = state.account_store.clone();

    let user_store = match &state.user_store {
        Some(store) => store.clone(),
        None => {
            return json_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "User store not configured",
            );
        }
    };

    let sync_result = task::spawn_blocking(move || {
        let index_store = IndexStore::new("/tmp/task_index.db")
            .map_err(|e| crate::tpm_cron::TpmCronError::IndexStoreSync(e.to_string()))?;
        crate::tpm_cron::trigger_tpm_sync(
            &store_clone,
            &user_store,
            &index_store,
            account_id,
            &org_name_for_sync,
        )
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked during trigger_tpm_sync: {}", e);
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    });

    match sync_result {
        Ok(Ok(result)) => {
            info!("trigger_tpm_sync: succeeded, task_id={}", result.task_id);
            (StatusCode::OK, Json(result)).into_response()
        }
        Ok(Err(e)) => {
            error!("trigger_tpm_sync: failed: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
        }
        Err(response) => response,
    }
}

/// PUT /auth/account/organization - Set the account's organization
pub async fn set_account_organization(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<SetOrganizationRequest>,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(acc) => acc,
        Err(response) => return response,
    };

    let store = state.account_store.clone();
    let account_id = account.id;
    let org_name = payload.organization_name.clone();

    let result =
        task::spawn_blocking(move || store.set_account_organization(account_id, &org_name))
            .await
            .map_err(|e| {
                error!("spawn_blocking panicked: {}", e);
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
            });

    match result {
        Ok(Ok(updated_account)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "account_id": updated_account.id,
                "organization_id": updated_account.organization_id,
                "organization_name": payload.organization_name,
            })),
        )
            .into_response(),
        Ok(Err(crate::account_store::AccountStoreError::NotFound)) => json_error_response(
            StatusCode::NOT_FOUND,
            &format!("Organization '{}' not found", payload.organization_name),
        ),
        Ok(Err(e)) => {
            error!("Failed to set account organization: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

/// DELETE /auth/account/organization - Remove the account from its organization
pub async fn clear_account_organization(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(acc) => acc,
        Err(response) => return response,
    };

    let store = state.account_store.clone();
    let account_id = account.id;

    let result = task::spawn_blocking(move || store.clear_account_organization(account_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        });

    match result {
        Ok(Ok(updated_account)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "account_id": updated_account.id,
                "organization_id": null,
            })),
        )
            .into_response(),
        Ok(Err(e)) => {
            error!("Failed to clear account organization: {}", e);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
        }
        Err(response) => response,
    }
}

#[derive(Debug, Serialize)]
pub struct WorkspaceProviderStateResponse {
    pub runtime: WorkspaceProviderRuntimeState,
    pub identifiers: Vec<IdentifierResponse>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceRecommendationFeedbackResponse {
    pub recorded: bool,
}

/// GET /api/workspace/provider-state
/// Returns runtime provider capabilities and connected provider state
/// from verified account identifiers.
pub async fn get_workspace_provider_state(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };

    let capabilities = derive_provider_capabilities(&ProviderCapabilityInputs {
        github_oauth_ready: oauth_ready(
            &state.github_client_id,
            &state.github_client_secret,
            &state.github_redirect_uri,
        ),
        google_docs_runtime_ready: env_flag_enabled("GOOGLE_DOCS_ENABLED")
            || GoogleAuthConfig::from_env().is_valid(),
        email_outbound_ready: env_has_value("POSTMARK_SERVER_TOKEN"),
        slack_oauth_ready: oauth_ready(
            &state.slack_client_id,
            &state.slack_client_secret,
            &state.slack_redirect_uri,
        ),
        slack_bot_ready: env_has_value("SLACK_BOT_TOKEN"),
        discord_oauth_ready: oauth_ready(
            &state.discord_client_id,
            &state.discord_client_secret,
            &state.discord_redirect_uri,
        ),
        discord_bot_ready: env_has_value("DISCORD_BOT_TOKEN"),
    });

    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return (
                StatusCode::OK,
                Json(WorkspaceProviderStateResponse {
                    runtime: WorkspaceProviderRuntimeState {
                        has_account: false,
                        capabilities,
                        connected: Default::default(),
                    },
                    identifiers: Vec::new(),
                }),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    let account_id = account.id;
    let store = state.account_store.clone();
    let identifiers_result = task::spawn_blocking(move || store.list_identifiers(account_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let identifiers = match identifiers_result {
        Ok(Ok(ids)) => ids,
        Ok(Err(e)) => {
            error!("Failed to list identifiers: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    let identifier_snapshots = identifiers
        .iter()
        .map(|identifier| LinkedIdentifierSnapshot {
            identifier_type: identifier.identifier_type.clone(),
            identifier: identifier.identifier.clone(),
            verified: identifier.verified,
        })
        .collect::<Vec<_>>();

    let connected = derive_provider_connections(&identifier_snapshots);

    (
        StatusCode::OK,
        Json(WorkspaceProviderStateResponse {
            runtime: WorkspaceProviderRuntimeState {
                has_account: true,
                capabilities,
                connected,
            },
            identifiers: identifiers
                .into_iter()
                .map(|i| IdentifierResponse {
                    identifier_type: i.identifier_type,
                    identifier: i.identifier,
                    verified: i.verified,
                })
                .collect(),
        }),
    )
        .into_response()
}

/// GET /api/workspace/recommendation-preferences
/// Returns the persisted proactivity level for the authenticated account.
pub async fn get_workspace_recommendation_preferences(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return json_error_response(StatusCode::UNAUTHORIZED, "Missing Authorization header")
        }
    };

    let auth_user = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user,
        Err((status, msg)) => return json_error_response(status, &msg),
    };

    let account = match load_account_for_auth_user(&state, auth_user.id).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    let store = state.account_store.clone();
    let account_id = account.id;
    let preference_result =
        task::spawn_blocking(move || store.get_recommendation_preference(account_id))
            .await
            .map_err(|e| {
                error!("spawn_blocking panicked: {}", e);
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
            });

    let preference = match preference_result {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => {
            error!("Failed to load recommendation preference: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    let proactivity_level = preference
        .as_ref()
        .map(|value| ProactivityLevel::from_storage_value(&value.proactivity_level))
        .unwrap_or_default();

    (
        StatusCode::OK,
        Json(WorkspaceRecommendationPreferences {
            proactivity_level,
            effective_proactivity_level: proactivity_level,
        }),
    )
        .into_response()
}

/// POST /api/workspace/recommendation
/// Returns the highest-confidence proactive recommendation for the authenticated account.
pub async fn get_workspace_recommendation(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(request): Json<WorkspaceRecommendationRequest>,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return json_error_response(StatusCode::UNAUTHORIZED, "Missing Authorization header")
        }
    };

    let auth_user = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user,
        Err((status, msg)) => return json_error_response(status, &msg),
    };

    let account = match load_account_for_auth_user(&state, auth_user.id).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    let blueprint = request.blueprint.normalize();
    if let Err(error) = blueprint.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": error.to_string()
            })),
        )
            .into_response();
    }

    let identifiers = match load_account_identifiers(&state, account.id).await {
        Ok(identifiers) => identifiers,
        Err(response) => return response,
    };

    let provider_runtime = provider_runtime_from_identifiers(&state, &identifiers);
    let recent_tasks = try_load_unified_account_tasks(&state, account.id).await;

    let store = state.account_store.clone();
    let account_id = account.id;
    let preference_result =
        task::spawn_blocking(move || store.get_recommendation_preference(account_id))
            .await
            .map_err(|e| {
                error!("spawn_blocking panicked: {}", e);
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
            });

    let preference = match preference_result {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => {
            error!("Failed to load recommendation preference: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    let store = state.account_store.clone();
    let account_id = account.id;
    let feedback_result =
        task::spawn_blocking(move || store.list_recent_recommendation_feedback(account_id, 50))
            .await
            .map_err(|e| {
                error!("spawn_blocking panicked: {}", e);
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
            });

    let feedback_snapshots = match feedback_result {
        Ok(Ok(records)) => records
            .into_iter()
            .filter_map(|record| {
                RecommendationFeedbackKind::from_storage_value(&record.feedback).map(|feedback| {
                    RecommendationFeedbackSnapshot {
                        recommendation_key: record.recommendation_key,
                        state_signature: record.state_signature,
                        feedback,
                        created_at: record.created_at,
                    }
                })
            })
            .collect::<Vec<_>>(),
        Ok(Err(e)) => {
            error!("Failed to load recommendation feedback: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    };

    let proactivity_level = preference
        .as_ref()
        .map(|value| ProactivityLevel::from_storage_value(&value.proactivity_level))
        .unwrap_or_default();

    let response = evaluate_workspace_recommendations(
        crate::service::startup_workspace::WorkspaceRecommendationContext {
            account_created_at: account.created_at,
            blueprint: &blueprint,
            provider_runtime: &provider_runtime,
            recent_tasks: &recent_tasks,
            proactivity_level,
            recent_feedback: &feedback_snapshots,
            now: Utc::now(),
        },
    );

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /api/workspace/recommendation-feedback
/// Records user feedback for recommendation cooldown and analytics.
pub async fn record_workspace_recommendation_feedback(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<WorkspaceRecommendationFeedbackRequest>,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return json_error_response(StatusCode::UNAUTHORIZED, "Missing Authorization header")
        }
    };

    let auth_user = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user,
        Err((status, msg)) => return json_error_response(status, &msg),
    };

    if payload.recommendation_key.trim().is_empty() || payload.state_signature.trim().is_empty() {
        return json_error_response(
            StatusCode::BAD_REQUEST,
            "recommendation_key and state_signature are required",
        );
    }

    let account = match load_account_for_auth_user(&state, auth_user.id).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    let recommendation_key = payload.recommendation_key.trim().to_string();
    let state_signature = payload.state_signature.trim().to_string();
    let feedback = payload.feedback.as_storage_value().to_string();
    let store = state.account_store.clone();
    let account_id = account.id;
    let feedback_result = task::spawn_blocking(move || {
        store.record_recommendation_feedback(
            account_id,
            &recommendation_key,
            &state_signature,
            &feedback,
            &serde_json::json!({}),
        )
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked: {}", e);
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    });

    match feedback_result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            error!("Failed to store recommendation feedback: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    }

    track_auth_event(
        &state.account_store,
        &format!("recommendation_{}", payload.feedback.as_storage_value()),
        Some(account.id),
        Some(auth_user.id),
        Some(format!(
            "recommendation_feedback:{}:{}:{}",
            account.id,
            payload.feedback.as_storage_value(),
            payload.recommendation_key
        )),
        Some("/api/workspace/recommendation-feedback"),
        serde_json::json!({
            "recommendation_key": payload.recommendation_key,
            "state_signature": payload.state_signature,
            "feedback": payload.feedback.as_storage_value()
        }),
    );

    (
        StatusCode::OK,
        Json(WorkspaceRecommendationFeedbackResponse { recorded: true }),
    )
        .into_response()
}

/// POST /api/workspace/recommendation-preferences
/// Updates the persisted proactivity level for the authenticated account.
pub async fn update_workspace_recommendation_preferences(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<WorkspaceRecommendationPreferencesUpdateRequest>,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return json_error_response(StatusCode::UNAUTHORIZED, "Missing Authorization header")
        }
    };

    let auth_user = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user,
        Err((status, msg)) => return json_error_response(status, &msg),
    };

    let account = match load_account_for_auth_user(&state, auth_user.id).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    let proactivity_level = payload.proactivity_level;
    let store = state.account_store.clone();
    let account_id = account.id;
    let preference_result = task::spawn_blocking(move || {
        store.upsert_recommendation_preference(account_id, proactivity_level.as_storage_value())
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked: {}", e);
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    });

    match preference_result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            error!("Failed to update recommendation preference: {}", e);
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Database error");
        }
        Err(response) => return response,
    }

    track_auth_event(
        &state.account_store,
        "recommendation_preference_updated",
        Some(account.id),
        Some(auth_user.id),
        Some(format!(
            "recommendation_preference_updated:{}:{}",
            account.id,
            payload.proactivity_level.as_storage_value()
        )),
        Some("/api/workspace/recommendation-preferences"),
        serde_json::json!({
            "proactivity_level": payload.proactivity_level.as_storage_value()
        }),
    );

    (
        StatusCode::OK,
        Json(WorkspaceRecommendationPreferences {
            proactivity_level: payload.proactivity_level,
            effective_proactivity_level: payload.proactivity_level,
        }),
    )
        .into_response()
}

/// POST /api/startup-workspace/intake-chat
/// LLM-driven conversational intake that returns a structured draft JSON.
pub async fn startup_workspace_intake_chat(
    Json(request): Json<StartupIntakeChatRequest>,
) -> impl IntoResponse {
    if request.messages.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "messages must include at least one conversation message"
            })),
        )
            .into_response();
    }

    match generate_startup_intake_chat_response(request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error_message) => {
            let status = if error_message.contains("No conversation messages")
                || error_message.contains("messages")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::BAD_GATEWAY
            };

            (
                status,
                Json(serde_json::json!({
                    "error": error_message
                })),
            )
                .into_response()
        }
    }
}

/// POST /api/launch-execution/analyze
/// Analyze one pasted launch thread into a narrow execution plan and readiness brief.
pub async fn analyze_launch_execution(
    Json(request): Json<LaunchExecutionRequest>,
) -> impl IntoResponse {
    match generate_launch_execution_response(request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error_message) => {
            let status = if error_message.contains("context_text")
                || error_message.contains("Unsupported source_type")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::BAD_GATEWAY
            };

            (
                status,
                Json(serde_json::json!({
                    "error": error_message
                })),
            )
                .into_response()
        }
    }
}

fn oauth_ready(
    client_id: &Option<String>,
    client_secret: &Option<String>,
    redirect_uri: &Option<String>,
) -> bool {
    has_value(client_id.as_deref())
        && has_value(client_secret.as_deref())
        && has_value(redirect_uri.as_deref())
}

fn env_has_value(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn env_flag_enabled(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .as_deref()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn has_value(value: Option<&str>) -> bool {
    value.map(|value| !value.trim().is_empty()).unwrap_or(false)
}

// ============================================================================
// Link Identifier
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct LinkRequest {
    pub identifier_type: String,
    pub identifier: String,
}

#[derive(Debug, Serialize)]
pub struct LinkResponse {
    pub identifier_type: String,
    pub identifier: String,
    pub verified: bool,
    pub message: String,
}

/// POST /auth/link
/// Start linking a channel identifier to the account.
/// For now, creates an unverified link. Verification can be added later.
pub async fn link_identifier(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<LinkRequest>,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };

    // Get account (run on blocking thread)
    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Account not found. Please sign up first."
                })),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    track_auth_event(
        &state.account_store,
        "channel_connect_started",
        Some(account.id),
        Some(account.auth_user_id),
        Some(format!(
            "channel_start:{}:{}:{}",
            account.id, req.identifier_type, req.identifier
        )),
        Some("/auth/link"),
        serde_json::json!({
            "identifier_type": req.identifier_type.clone(),
            "identifier": req.identifier.clone(),
        }),
    );

    // For email type, create a verification token and send email
    if req.identifier_type == "email" {
        let account_id = account.id;
        let email = req.identifier.clone();
        let store = state.account_store.clone();
        let frontend_url = state.frontend_url.clone();

        // Create verification token
        let token_result =
            task::spawn_blocking(move || store.create_email_verification_token(account_id, &email))
                .await
                .map_err(|e| {
                    error!("spawn_blocking panicked: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": "Internal error" })),
                    )
                });

        let verification_token = match token_result {
            Ok(Ok(token)) => token,
            Ok(Err(e)) => {
                error!("Failed to create verification token: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "Failed to create verification token"
                    })),
                )
                    .into_response();
            }
            Err(resp) => return resp.into_response(),
        };

        // Send verification email
        let verify_url = format!(
            "{}/auth/index.html?verify_email={}",
            frontend_url, verification_token.token
        );

        if let Err(e) = send_verification_email(&verification_token.email, &verify_url).await {
            error!("Failed to send verification email: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to send verification email"
                })),
            )
                .into_response();
        }

        info!(
            "Sent verification email to {} for account {}",
            verification_token.email, account.id
        );
        track_auth_event(
            &state.account_store,
            "channel_connect_pending",
            Some(account.id),
            Some(account.auth_user_id),
            Some(format!(
                "channel_pending:{}:{}:{}",
                account.id, req.identifier_type, req.identifier
            )),
            Some("/auth/link"),
            serde_json::json!({
                "identifier_type": req.identifier_type.clone(),
                "identifier": req.identifier.clone(),
                "verification_required": true,
            }),
        );

        return (
            StatusCode::OK,
            Json(LinkResponse {
                identifier_type: req.identifier_type,
                identifier: req.identifier,
                verified: false,
                message: "Verification email sent. Please check your inbox.".to_string(),
            }),
        )
            .into_response();
    }

    // For other types (discord, slack, phone, etc.), create identifier directly
    let account_id = account.id;
    let identifier_type = req.identifier_type.clone();
    let identifier = req.identifier.clone();
    let store = state.account_store.clone();
    let create_result = task::spawn_blocking(move || {
        store.create_identifier(account_id, &identifier_type, &identifier)
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Internal error" })),
        )
    });

    match create_result {
        Ok(Ok(identifier)) => {
            info!(
                "Linked identifier {}:{} to account {}",
                req.identifier_type, req.identifier, account.id
            );
            track_auth_event(
                &state.account_store,
                "channel_connect_succeeded",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect:{}:{}:{}",
                    account.id, identifier.identifier_type, identifier.identifier
                )),
                Some("/auth/link"),
                serde_json::json!({
                    "identifier_type": identifier.identifier_type.clone(),
                    "identifier": identifier.identifier.clone(),
                    "provider": identifier.identifier_type.clone(),
                }),
            );
            track_auth_event(
                &state.account_store,
                "first_channel_or_tool_connected",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!("first_channel:{}", account.id)),
                Some("/auth/link"),
                serde_json::json!({
                    "identifier_type": identifier.identifier_type.clone(),
                    "provider": identifier.identifier_type.clone(),
                }),
            );
            (
                StatusCode::CREATED,
                Json(LinkResponse {
                    identifier_type: identifier.identifier_type,
                    identifier: identifier.identifier,
                    verified: identifier.verified,
                    message: "Identifier linked.".to_string(),
                }),
            )
                .into_response()
        }
        Ok(Err(AccountStoreError::IdentifierTaken)) => {
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:{}:{}",
                    account.id, req.identifier_type, req.identifier
                )),
                Some("/auth/link"),
                serde_json::json!({
                    "identifier_type": req.identifier_type.clone(),
                    "identifier": req.identifier.clone(),
                    "error_reason": "identifier_taken",
                }),
            );
            (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "This identifier is already linked to another account"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => {
            error!("Failed to link identifier: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:{}:{}",
                    account.id, req.identifier_type, req.identifier
                )),
                Some("/auth/link"),
                serde_json::json!({
                    "identifier_type": req.identifier_type.clone(),
                    "identifier": req.identifier.clone(),
                    "error_reason": "database_error",
                }),
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to link identifier"
                })),
            )
                .into_response()
        }
        Err(resp) => resp.into_response(),
    }
}

// ============================================================================
// Verify Identifier
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub identifier_type: String,
    pub identifier: String,
    pub code: String,
}

/// POST /auth/verify
/// Verify an identifier with a verification code.
/// For now, accepts any code and marks as verified (placeholder).
pub async fn verify_identifier(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<VerifyRequest>,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };

    // Get account (run on blocking thread)
    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Account not found"
                })),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    // TODO: Actually validate the verification code
    // For now, just mark as verified
    let account_id = account.id;
    let identifier_type = req.identifier_type.clone();
    let identifier = req.identifier.clone();
    let store = state.account_store.clone();
    let verify_result = task::spawn_blocking(move || {
        store.verify_identifier(account_id, &identifier_type, &identifier)
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Internal error" })),
        )
    });

    match verify_result {
        Ok(Ok(())) => {
            info!(
                "Verified identifier {}:{} for account {}",
                req.identifier_type, req.identifier, account.id
            );
            track_auth_event(
                &state.account_store,
                "channel_connect_succeeded",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect:{}:{}:{}",
                    account.id, req.identifier_type, req.identifier
                )),
                Some("/auth/verify"),
                serde_json::json!({
                    "identifier_type": req.identifier_type.clone(),
                    "identifier": req.identifier.clone(),
                    "verification_method": "code",
                }),
            );
            track_auth_event(
                &state.account_store,
                "first_channel_or_tool_connected",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!("first_channel:{}", account.id)),
                Some("/auth/verify"),
                serde_json::json!({
                    "identifier_type": req.identifier_type.clone(),
                    "provider": req.identifier_type.clone(),
                }),
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "verified": true,
                    "message": "Identifier verified successfully"
                })),
            )
                .into_response()
        }
        Ok(Err(AccountStoreError::NotFound)) => {
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:{}:{}",
                    account.id, req.identifier_type, req.identifier
                )),
                Some("/auth/verify"),
                serde_json::json!({
                    "identifier_type": req.identifier_type.clone(),
                    "identifier": req.identifier.clone(),
                    "error_reason": "identifier_not_found",
                }),
            );
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Identifier not found"
                })),
            )
                .into_response()
        }
        Ok(Err(e)) => {
            error!("Failed to verify identifier: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:{}:{}",
                    account.id, req.identifier_type, req.identifier
                )),
                Some("/auth/verify"),
                serde_json::json!({
                    "identifier_type": req.identifier_type.clone(),
                    "identifier": req.identifier.clone(),
                    "error_reason": "database_error",
                }),
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to verify identifier"
                })),
            )
                .into_response()
        }
        Err(resp) => resp.into_response(),
    }
}

// ============================================================================
// Unlink Identifier
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct UnlinkRequest {
    pub identifier_type: String,
    pub identifier: String,
}

/// DELETE /auth/unlink
/// Remove a linked identifier from the account.
pub async fn unlink_identifier(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<UnlinkRequest>,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };

    // Get account (run on blocking thread)
    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Account not found"
                })),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    // Delete identifier (run on blocking thread)
    let account_id = account.id;
    let identifier_type = req.identifier_type.clone();
    let identifier = req.identifier.clone();
    let store = state.account_store.clone();
    let delete_result = task::spawn_blocking(move || {
        store.delete_identifier(account_id, &identifier_type, &identifier)
    })
    .await
    .map_err(|e| {
        error!("spawn_blocking panicked: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Internal error" })),
        )
    });

    match delete_result {
        Ok(Ok(())) => {
            info!(
                "Unlinked identifier {}:{} from account {}",
                req.identifier_type, req.identifier, account.id
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "message": "Identifier unlinked successfully"
                })),
            )
                .into_response()
        }
        Ok(Err(AccountStoreError::NotFound)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Identifier not found"
            })),
        )
            .into_response(),
        Ok(Err(e)) => {
            error!("Failed to unlink identifier: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to unlink identifier"
                })),
            )
                .into_response()
        }
        Err(resp) => resp.into_response(),
    }
}

// ============================================================================
// Delete Account
// ============================================================================

/// DELETE /auth/account
/// Delete the current user's DoWhiz account and all linked identifiers.
pub async fn delete_account(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };

    // Get account (run on blocking thread)
    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Account not found"
                })),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    // Delete account (run on blocking thread)
    let account_id = account.id;
    let store = state.account_store.clone();
    let delete_result = task::spawn_blocking(move || store.delete_account(account_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    match delete_result {
        Ok(Ok(())) => {
            info!(
                "Deleted account {} for auth_user_id={}",
                account_id, auth_user_id
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "message": "Account deleted successfully"
                })),
            )
                .into_response()
        }
        Ok(Err(AccountStoreError::NotFound)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Account not found"
            })),
        )
            .into_response(),
        Ok(Err(e)) => {
            error!("Failed to delete account: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to delete account"
                })),
            )
                .into_response()
        }
        Err(resp) => resp.into_response(),
    }
}

// ============================================================================
// Get Memo
// ============================================================================

#[derive(Debug, Serialize)]
pub struct MemoResponse {
    pub account_id: Uuid,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct MemoUpdateRequest {
    pub content: String,
}

/// GET /auth/memo
/// Returns the memo.md content for the current user's account.
pub async fn get_memo(State(state): State<AuthState>, headers: HeaderMap) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };

    // Get account (run on blocking thread)
    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Account not found. Please sign up first."
                })),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    // Check if blob store is available
    let blob_store = match &state.blob_store {
        Some(store) => store.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Memo storage not configured"
                })),
            )
                .into_response();
        }
    };

    // Read memo from blob storage
    let account_id = account.id;
    match blob_store.read_memo(account_id).await {
        Ok(content) => {
            info!("Retrieved memo for account {}", account_id);
            (
                StatusCode::OK,
                Json(MemoResponse {
                    account_id,
                    content,
                }),
            )
                .into_response()
        }
        Err(e) => {
            error!("Failed to read memo for account {}: {}", account_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to read memo"
                })),
            )
                .into_response()
        }
    }
}

/// POST /auth/memo
/// Updates the memo.md content for the current user's account.
pub async fn update_memo(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<MemoUpdateRequest>,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err((status, msg)) => {
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }
    };

    // Get account (run on blocking thread)
    let store = state.account_store.clone();
    let account_result = task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
        });

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Account not found. Please sign up first."
                })),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Database error"
                })),
            )
                .into_response();
        }
        Err(resp) => return resp.into_response(),
    };

    // Check if blob store is available
    let blob_store = match &state.blob_store {
        Some(store) => store.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Memo storage not configured"
                })),
            )
                .into_response();
        }
    };

    // Write memo directly to blob storage
    let account_id = account.id;
    match blob_store.write_memo(account_id, &payload.content).await {
        Ok(()) => {
            info!("Updated memo for account {}", account_id);
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "account_id": account_id
                })),
            )
                .into_response()
        }
        Err(e) => {
            error!("Failed to write memo for account {}: {}", account_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to save memo"
                })),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Discord OAuth
// ============================================================================

/// Query params for Discord OAuth callback
#[derive(Debug, Deserialize)]
pub struct DiscordCallbackQuery {
    pub code: String,
    pub state: String,
}

/// Query params for Discord bot installation callback
#[derive(Debug, Deserialize)]
pub struct DiscordBotCallbackQuery {
    pub code: String,
    pub state: String,
    pub guild_id: Option<String>,
    pub permissions: Option<String>,
}

/// Discord bot OAuth response (for bot installation)
#[derive(Debug, Deserialize)]
struct DiscordBotOAuthResponse {
    pub guild: Option<DiscordGuild>,
}

#[derive(Debug, Deserialize)]
struct DiscordGuild {
    pub id: String,
    pub name: Option<String>,
}

/// Discord token response
#[derive(Debug, Deserialize)]
struct DiscordTokenResponse {
    access_token: String,
    token_type: String,
}

/// Discord user response
#[derive(Debug, Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
}

/// GET /auth/discord
/// Initiates Discord OAuth flow - redirects to Discord's authorization page.
pub async fn discord_oauth_start(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check if Discord OAuth is configured
    let (client_id, redirect_uri) = match (&state.discord_client_id, &state.discord_redirect_uri) {
        (Some(id), Some(uri)) => (id.clone(), uri.clone()),
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Discord OAuth not configured"
                })),
            )
                .into_response();
        }
    };

    // Extract and validate Supabase token
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    // Validate the token to ensure user is authenticated
    if let Err((status, msg)) = validate_supabase_token(&state.supabase_url, &token).await {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // Encode the Supabase token in state so we can identify the user on callback
    let encoded_state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token.as_bytes());

    // Build Discord OAuth URL
    let discord_auth_url = format!(
        "https://discord.com/api/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&scope=identify&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        encoded_state
    );

    // Return the URL for the frontend to redirect to
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "redirect_url": discord_auth_url
        })),
    )
        .into_response()
}

/// GET /auth/discord/callback
/// Handles Discord OAuth callback - exchanges code for token, gets user info, links account.
pub async fn discord_oauth_callback(
    State(state): State<AuthState>,
    Query(params): Query<DiscordCallbackQuery>,
) -> impl IntoResponse {
    // Helper to build redirect URLs to the frontend
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    // Check if Discord OAuth is configured
    let (client_id, client_secret, redirect_uri) = match (
        &state.discord_client_id,
        &state.discord_client_secret,
        &state.discord_redirect_uri,
    ) {
        (Some(id), Some(secret), Some(uri)) => (id.clone(), secret.clone(), uri.clone()),
        _ => {
            return redirect_to("/auth/index.html?discord=error&reason=not_configured");
        }
    };

    // Decode state to get the Supabase token
    let token = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&params.state) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                return redirect_to("/auth/index.html?discord=error&reason=invalid_state");
            }
        },
        Err(_) => {
            return redirect_to("/auth/index.html?discord=error&reason=invalid_state");
        }
    };

    // Validate Supabase token and get user
    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err(_) => {
            return redirect_to("/auth/index.html?discord=error&reason=invalid_token");
        }
    };

    // Exchange code for Discord access token
    let client = reqwest::Client::new();
    let token_res = client
        .post("https://discord.com/api/oauth2/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", params.code.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await;

    let discord_token = match token_res {
        Ok(res) if res.status().is_success() => match res.json::<DiscordTokenResponse>().await {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to parse Discord token response: {}", e);
                return redirect_to("/auth/index.html?discord=error&reason=token_parse_error");
            }
        },
        Ok(res) => {
            error!("Discord token exchange failed: {}", res.status());
            return redirect_to("/auth/index.html?discord=error&reason=token_exchange_failed");
        }
        Err(e) => {
            error!("Discord token request failed: {}", e);
            return redirect_to("/auth/index.html?discord=error&reason=token_request_failed");
        }
    };

    // Get Discord user info
    let user_res = client
        .get("https://discord.com/api/users/@me")
        .header(
            "Authorization",
            format!(
                "{} {}",
                discord_token.token_type, discord_token.access_token
            ),
        )
        .send()
        .await;

    let discord_user = match user_res {
        Ok(res) if res.status().is_success() => match res.json::<DiscordUser>().await {
            Ok(u) => u,
            Err(e) => {
                error!("Failed to parse Discord user response: {}", e);
                return redirect_to("/auth/index.html?discord=error&reason=user_parse_error");
            }
        },
        Ok(res) => {
            error!("Discord user request failed: {}", res.status());
            return redirect_to("/auth/index.html?discord=error&reason=user_request_failed");
        }
        Err(e) => {
            error!("Discord user request failed: {}", e);
            return redirect_to("/auth/index.html?discord=error&reason=user_request_failed");
        }
    };

    info!(
        "Discord OAuth successful for user {} (Discord: {} / {})",
        auth_user_id, discord_user.id, discord_user.username
    );

    // Get user's account
    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id)).await;

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return redirect_to("/auth/index.html?discord=error&reason=account_not_found");
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return redirect_to("/auth/index.html?discord=error&reason=db_error");
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return redirect_to("/auth/index.html?discord=error&reason=internal_error");
        }
    };

    // Link Discord ID to account
    let store = state.account_store.clone();
    let discord_id = discord_user.id.clone();
    let link_result =
        task::spawn_blocking(move || store.create_identifier(account.id, "discord", &discord_id))
            .await;

    match link_result {
        Ok(Ok(_identifier)) => {
            info!(
                "Linked Discord {} to account {}",
                discord_user.id, account.id
            );
            track_auth_event(
                &state.account_store,
                "channel_connect_succeeded",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect:{}:discord:{}",
                    account.id, discord_user.id
                )),
                Some("/auth/discord/callback"),
                serde_json::json!({
                    "identifier_type": "discord",
                    "identifier": discord_user.id,
                    "provider": "discord",
                }),
            );
            track_auth_event(
                &state.account_store,
                "first_channel_or_tool_connected",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!("first_channel:{}", account.id)),
                Some("/auth/discord/callback"),
                serde_json::json!({
                    "identifier_type": "discord",
                    "provider": "discord",
                }),
            );
            let reconnect_state = state.clone();
            let reconnect_event_nonce = oauth_callback_event_nonce(&params.code);
            match task::spawn_blocking(move || {
                execute_platform_reconnect_onboarding(
                    &reconnect_state,
                    account.id,
                    account.auth_user_id,
                    InstallPlatform::Discord,
                    &reconnect_event_nonce,
                )
            })
            .await
            {
                Ok(Ok(attempted)) => {
                    if attempted > 0 {
                        info!(
                            "Replayed Discord onboarding for account {} across {} guild(s) after reconnect",
                            account.id, attempted
                        );
                    }
                }
                Ok(Err(err)) => {
                    warn!(
                        "Discord reconnect onboarding failed for account {}: {}",
                        account.id, err
                    );
                }
                Err(err) => {
                    warn!(
                        "Discord reconnect onboarding task join failed for account {}: {}",
                        account.id, err
                    );
                }
            }
            redirect_to("/auth/index.html?discord=success")
        }
        Ok(Err(AccountStoreError::IdentifierTaken)) => {
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:discord:{}",
                    account.id, discord_user.id
                )),
                Some("/auth/discord/callback"),
                serde_json::json!({
                    "identifier_type": "discord",
                    "identifier": discord_user.id,
                    "error_reason": "identifier_taken",
                }),
            );
            redirect_to("/auth/index.html?discord=error&reason=already_linked")
        }
        Ok(Err(e)) => {
            error!("Failed to link Discord: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:discord:{}",
                    account.id, discord_user.id
                )),
                Some("/auth/discord/callback"),
                serde_json::json!({
                    "identifier_type": "discord",
                    "identifier": discord_user.id,
                    "error_reason": "link_failed",
                }),
            );
            redirect_to("/auth/index.html?discord=error&reason=link_failed")
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:discord:{}",
                    account.id, discord_user.id
                )),
                Some("/auth/discord/callback"),
                serde_json::json!({
                    "identifier_type": "discord",
                    "identifier": discord_user.id,
                    "error_reason": "internal_error",
                }),
            );
            redirect_to("/auth/index.html?discord=error&reason=internal_error")
        }
    }
}

/// GET /auth/discord/bot-callback
/// Handles Discord bot installation callback - records bot installation event and redirects.
pub async fn discord_bot_callback(
    State(state): State<AuthState>,
    Query(params): Query<DiscordBotCallbackQuery>,
) -> impl IntoResponse {
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    // Decode state to get the Supabase token
    let token = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&params.state) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                return redirect_to("/auth/index.html?discord_bot=error&reason=invalid_state");
            }
        },
        Err(_) => {
            return redirect_to("/auth/index.html?discord_bot=error&reason=invalid_state");
        }
    };

    // Validate Supabase token and get user
    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err(_) => {
            return redirect_to("/auth/index.html?discord_bot=error&reason=invalid_token");
        }
    };

    // Look up the account for this auth user
    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id)).await;

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return redirect_to("/auth/index.html?discord_bot=error&reason=account_not_found");
        }
        Ok(Err(e)) => {
            error!("Failed to lookup account: {}", e);
            return redirect_to("/auth/index.html?discord_bot=error&reason=internal_error");
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return redirect_to("/auth/index.html?discord_bot=error&reason=internal_error");
        }
    };

    // Get guild_id - either from params directly or by exchanging code
    let (guild_id, guild_name) = if let Some(gid) = params.guild_id.clone() {
        (gid, None)
    } else {
        // Check if Discord OAuth is configured
        let (client_id, client_secret, redirect_uri) = match (
            &state.discord_client_id,
            &state.discord_client_secret,
            &state.discord_redirect_uri,
        ) {
            (Some(id), Some(secret), Some(_)) => {
                let uri = format!(
                    "{}/auth/discord/bot-callback",
                    std::env::var("DOWHIZ_API_URL").unwrap_or_else(|_| {
                        "https://api.production1.dowhiz.com/service".to_string()
                    })
                );
                (id.clone(), secret.clone(), uri)
            }
            _ => {
                return redirect_to("/auth/index.html?discord_bot=error&reason=not_configured");
            }
        };

        // Exchange code for access token to get guild info
        let client = reqwest::Client::new();
        let token_res = client
            .post("https://discord.com/api/oauth2/token")
            .form(&[
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("code", params.code.as_str()),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect_uri.as_str()),
            ])
            .send()
            .await;

        match token_res {
            Ok(res) if res.status().is_success() => match res
                .json::<DiscordBotOAuthResponse>()
                .await
            {
                Ok(data) => {
                    if let Some(guild) = data.guild {
                        (guild.id, guild.name)
                    } else {
                        ("unknown".to_string(), None)
                    }
                }
                Err(e) => {
                    error!("Failed to parse Discord response: {}", e);
                    return redirect_to("/auth/index.html?discord_bot=error&reason=parse_error");
                }
            },
            Ok(res) => {
                error!("Discord token exchange failed: {}", res.status());
                return redirect_to(
                    "/auth/index.html?discord_bot=error&reason=token_exchange_failed",
                );
            }
            Err(e) => {
                error!("Discord token request failed: {}", e);
                return redirect_to("/auth/index.html?discord_bot=error&reason=request_failed");
            }
        }
    };

    // Record the bot installation event
    track_auth_event(
        &state.account_store,
        "discord_bot_installed",
        Some(account.id),
        Some(account.auth_user_id),
        Some(format!("discord_bot_installed:{}:{}", account.id, guild_id)),
        Some("/auth/discord/bot-callback"),
        serde_json::json!({
            "guild_id": guild_id.clone(),
            "guild_name": guild_name.clone(),
            "permissions": params.permissions,
        }),
    );

    let onboarding_state = state.clone();
    let onboarding_guild_id = guild_id.clone();
    let onboarding_guild_name = guild_name.clone();
    let onboarding_event_nonce = oauth_callback_event_nonce(&params.code);
    let onboarding_result = task::spawn_blocking(move || {
        execute_discord_install_onboarding(
            &onboarding_state,
            account.id,
            account.auth_user_id,
            onboarding_guild_id,
            onboarding_guild_name,
            &onboarding_event_nonce,
        )
    })
    .await;

    match onboarding_result {
        Ok(Ok(result)) => {
            info!(
                "Discord install onboarding finished for account {} guild {} (public={}, dm={})",
                account.id,
                guild_id,
                result.public_status.as_str(),
                result.dm_status.as_str()
            );
        }
        Ok(Err(err)) => {
            warn!(
                "Discord install onboarding failed for account {} guild {}: {}",
                account.id, guild_id, err
            );
        }
        Err(err) => {
            warn!(
                "Discord install onboarding task join failed for account {} guild {}: {}",
                account.id, guild_id, err
            );
        }
    }

    info!(
        "Discord bot installed for account {} in guild {}",
        account.id, guild_id
    );

    // Include the token in fragment so frontend can restore the session
    let encoded_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token.as_bytes());
    Redirect::to(&format!(
        "{}/auth/index.html?discord_bot=success#access_token={}",
        frontend_url, encoded_token
    ))
    .into_response()
}

// ============================================================================
// Slack OAuth
// ============================================================================

/// Query params for Slack OAuth callback
#[derive(Debug, Deserialize)]
pub struct SlackCallbackQuery {
    pub code: String,
    pub state: String,
}

/// Query params for Slack bot installation callback
#[derive(Debug, Deserialize)]
pub struct SlackBotCallbackQuery {
    pub code: String,
    pub state: String,
}

/// Slack bot OAuth response (for bot installation)
#[derive(Debug, Deserialize)]
struct SlackBotOAuthResponse {
    ok: bool,
    error: Option<String>,
    team: Option<SlackTeam>,
    access_token: Option<String>,
    bot_user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackTeam {
    id: String,
    name: Option<String>,
}

/// Slack OAuth token response
#[derive(Debug, Deserialize)]
struct SlackTokenResponse {
    ok: bool,
    error: Option<String>,
    authed_user: Option<SlackAuthedUser>,
}

#[derive(Debug, Deserialize)]
struct SlackAuthedUser {
    id: String,
    access_token: Option<String>,
}

#[derive(Debug)]
pub(crate) struct VerifiedSlackBotIdentity {
    pub team_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackAuthTestResponse {
    ok: bool,
    error: Option<String>,
    team_id: Option<String>,
    user_id: Option<String>,
}

pub(crate) async fn verify_slack_bot_access(
    bot_token: &str,
) -> Result<VerifiedSlackBotIdentity, String> {
    let response = reqwest::Client::new()
        .get("https://slack.com/api/auth.test")
        .header("Authorization", format!("Bearer {}", bot_token))
        .send()
        .await
        .map_err(|err| format!("request failed: {err}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("status {} body {}", status, body));
    }

    let payload = response
        .json::<SlackAuthTestResponse>()
        .await
        .map_err(|err| format!("invalid response: {err}"))?;

    if !payload.ok {
        return Err(payload
            .error
            .unwrap_or_else(|| "unknown auth.test error".to_string()));
    }

    Ok(VerifiedSlackBotIdentity {
        team_id: payload.team_id,
        user_id: payload.user_id,
    })
}

/// GET /auth/slack
/// Initiates Slack OAuth flow - redirects to Slack's authorization page.
pub async fn slack_oauth_start(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check if Slack OAuth is configured
    let (client_id, redirect_uri) = match (&state.slack_client_id, &state.slack_redirect_uri) {
        (Some(id), Some(uri)) => (id.clone(), uri.clone()),
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Slack OAuth not configured"
                })),
            )
                .into_response();
        }
    };

    // Extract and validate Supabase token
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    // Validate the token to ensure user is authenticated
    if let Err((status, msg)) = validate_supabase_token(&state.supabase_url, &token).await {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // Encode the Supabase token in state so we can identify the user on callback
    let encoded_state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token.as_bytes());

    // Build Slack OAuth URL - using user_scope for user identity
    let slack_auth_url = format!(
        "https://slack.com/oauth/v2/authorize?client_id={}&user_scope=identity.basic&redirect_uri={}&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        encoded_state
    );

    // Return the URL for the frontend to redirect to
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "redirect_url": slack_auth_url
        })),
    )
        .into_response()
}

/// GET /auth/slack/callback
/// Handles Slack OAuth callback - exchanges code for token, gets user info, links account.
pub async fn slack_oauth_callback(
    State(state): State<AuthState>,
    Query(params): Query<SlackCallbackQuery>,
) -> impl IntoResponse {
    // Helper to build redirect URLs to the frontend
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    // Check if Slack OAuth is configured
    let (client_id, client_secret, redirect_uri) = match (
        &state.slack_client_id,
        &state.slack_client_secret,
        &state.slack_redirect_uri,
    ) {
        (Some(id), Some(secret), Some(uri)) => (id.clone(), secret.clone(), uri.clone()),
        _ => {
            return redirect_to("/auth/index.html?slack=error&reason=not_configured");
        }
    };

    // Decode state to get the Supabase token
    let token = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&params.state) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                return redirect_to("/auth/index.html?slack=error&reason=invalid_state");
            }
        },
        Err(_) => {
            return redirect_to("/auth/index.html?slack=error&reason=invalid_state");
        }
    };

    // Validate Supabase token and get user
    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err(_) => {
            return redirect_to("/auth/index.html?slack=error&reason=invalid_token");
        }
    };

    // Exchange code for Slack access token
    let client = reqwest::Client::new();
    let token_res = client
        .post("https://slack.com/api/oauth.v2.access")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", params.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await;

    let slack_response = match token_res {
        Ok(res) => match res.json::<SlackTokenResponse>().await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse Slack token response: {}", e);
                return redirect_to("/auth/index.html?slack=error&reason=token_parse_error");
            }
        },
        Err(e) => {
            error!("Slack token request failed: {}", e);
            return redirect_to("/auth/index.html?slack=error&reason=token_request_failed");
        }
    };

    // Check if Slack returned an error
    if !slack_response.ok {
        let error_msg = slack_response
            .error
            .unwrap_or_else(|| "unknown".to_string());
        error!("Slack OAuth error: {}", error_msg);
        return redirect_to(&format!(
            "/auth/index.html?slack=error&reason={}",
            urlencoding::encode(&error_msg)
        ));
    }

    // Get the user ID from the response
    let slack_user = match slack_response.authed_user {
        Some(user) => user,
        None => {
            error!("Slack response missing authed_user");
            return redirect_to("/auth/index.html?slack=error&reason=missing_user");
        }
    };

    info!(
        "Slack OAuth successful for user {} (Slack ID: {})",
        auth_user_id, slack_user.id
    );

    // Get user's account
    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id)).await;

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return redirect_to("/auth/index.html?slack=error&reason=account_not_found");
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return redirect_to("/auth/index.html?slack=error&reason=db_error");
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return redirect_to("/auth/index.html?slack=error&reason=internal_error");
        }
    };

    // Link Slack ID to account
    let store = state.account_store.clone();
    let slack_id = slack_user.id.clone();
    let link_result =
        task::spawn_blocking(move || store.create_identifier(account.id, "slack", &slack_id)).await;

    match link_result {
        Ok(Ok(_identifier)) => {
            info!("Linked Slack {} to account {}", slack_user.id, account.id);
            track_auth_event(
                &state.account_store,
                "channel_connect_succeeded",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect:{}:slack:{}",
                    account.id, slack_user.id
                )),
                Some("/auth/slack/callback"),
                serde_json::json!({
                    "identifier_type": "slack",
                    "identifier": slack_user.id,
                    "provider": "slack",
                }),
            );
            track_auth_event(
                &state.account_store,
                "first_channel_or_tool_connected",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!("first_channel:{}", account.id)),
                Some("/auth/slack/callback"),
                serde_json::json!({
                    "identifier_type": "slack",
                    "provider": "slack",
                }),
            );
            let reconnect_state = state.clone();
            let reconnect_event_nonce = oauth_callback_event_nonce(&params.code);
            match task::spawn_blocking(move || {
                execute_platform_reconnect_onboarding(
                    &reconnect_state,
                    account.id,
                    account.auth_user_id,
                    InstallPlatform::Slack,
                    &reconnect_event_nonce,
                )
            })
            .await
            {
                Ok(Ok(attempted)) => {
                    if attempted > 0 {
                        info!(
                            "Replayed Slack onboarding for account {} across {} workspace(s) after reconnect",
                            account.id, attempted
                        );
                    }
                }
                Ok(Err(err)) => {
                    warn!(
                        "Slack reconnect onboarding failed for account {}: {}",
                        account.id, err
                    );
                }
                Err(err) => {
                    warn!(
                        "Slack reconnect onboarding task join failed for account {}: {}",
                        account.id, err
                    );
                }
            }
            redirect_to("/auth/index.html?slack=success")
        }
        Ok(Err(AccountStoreError::IdentifierTaken)) => {
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:slack:{}",
                    account.id, slack_user.id
                )),
                Some("/auth/slack/callback"),
                serde_json::json!({
                    "identifier_type": "slack",
                    "identifier": slack_user.id,
                    "error_reason": "identifier_taken",
                }),
            );
            redirect_to("/auth/index.html?slack=error&reason=already_linked")
        }
        Ok(Err(e)) => {
            error!("Failed to link Slack: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:slack:{}",
                    account.id, slack_user.id
                )),
                Some("/auth/slack/callback"),
                serde_json::json!({
                    "identifier_type": "slack",
                    "identifier": slack_user.id,
                    "error_reason": "link_failed",
                }),
            );
            redirect_to("/auth/index.html?slack=error&reason=link_failed")
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:slack:{}",
                    account.id, slack_user.id
                )),
                Some("/auth/slack/callback"),
                serde_json::json!({
                    "identifier_type": "slack",
                    "identifier": slack_user.id,
                    "error_reason": "internal_error",
                }),
            );
            redirect_to("/auth/index.html?slack=error&reason=internal_error")
        }
    }
}

/// GET /auth/slack/bot-callback
/// Handles Slack bot installation callback - exchanges code for team info, records event.
pub async fn slack_bot_callback(
    State(state): State<AuthState>,
    Query(params): Query<SlackBotCallbackQuery>,
) -> impl IntoResponse {
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    // Check if Slack OAuth is configured
    let (client_id, client_secret) = match (&state.slack_client_id, &state.slack_client_secret) {
        (Some(id), Some(secret)) => (id.clone(), secret.clone()),
        _ => {
            return redirect_to("/auth/index.html?slack_bot=error&reason=not_configured");
        }
    };

    // Decode state to get the Supabase token
    let token = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&params.state) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                return redirect_to("/auth/index.html?slack_bot=error&reason=invalid_state");
            }
        },
        Err(_) => {
            return redirect_to("/auth/index.html?slack_bot=error&reason=invalid_state");
        }
    };

    // Validate Supabase token and get user
    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err(_) => {
            return redirect_to("/auth/index.html?slack_bot=error&reason=invalid_token");
        }
    };

    // Look up the account for this auth user
    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id)).await;

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return redirect_to("/auth/index.html?slack_bot=error&reason=account_not_found");
        }
        Ok(Err(e)) => {
            error!("Failed to lookup account: {}", e);
            return redirect_to("/auth/index.html?slack_bot=error&reason=internal_error");
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return redirect_to("/auth/index.html?slack_bot=error&reason=internal_error");
        }
    };

    // Exchange code for access token to get team info
    let redirect_uri = format!(
        "{}/auth/slack/bot-callback",
        std::env::var("DOWHIZ_API_URL")
            .unwrap_or_else(|_| "https://api.production1.dowhiz.com/service".to_string())
    );

    let client = reqwest::Client::new();
    let token_res = client
        .post("https://slack.com/api/oauth.v2.access")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", params.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await;

    let installation = match token_res {
        Ok(res) if res.status().is_success() => match res.json::<SlackBotOAuthResponse>().await {
            Ok(data) if data.ok => {
                let team = data.team.unwrap_or(SlackTeam {
                    id: "unknown".to_string(),
                    name: None,
                });
                let bot_token = data.access_token.unwrap_or_default();
                if team.id.trim().is_empty() || bot_token.trim().is_empty() {
                    error!("Slack OAuth response missing team_id or access_token");
                    return redirect_to(
                        "/auth/index.html?slack_bot=error&reason=missing_installation_data",
                    );
                }

                let verified_identity = match verify_slack_bot_access(&bot_token).await {
                    Ok(identity) => identity,
                    Err(err) => {
                        error!("Slack auth.test failed after bot install: {}", err);
                        return redirect_to(
                            "/auth/index.html?slack_bot=error&reason=verification_failed",
                        );
                    }
                };

                if let Some(verified_team_id) = verified_identity.team_id.as_deref() {
                    if verified_team_id != team.id {
                        warn!(
                            "Slack OAuth team_id {} differed from auth.test team_id {}",
                            team.id, verified_team_id
                        );
                    }
                }

                SlackInstallation {
                    team_id: team.id,
                    team_name: team.name,
                    bot_token,
                    bot_user_id: data
                        .bot_user_id
                        .filter(|value| !value.trim().is_empty())
                        .or(verified_identity.user_id)
                        .unwrap_or_default(),
                    installed_at: Utc::now(),
                }
            }
            Ok(data) => {
                error!("Slack OAuth failed: {:?}", data.error);
                return redirect_to("/auth/index.html?slack_bot=error&reason=oauth_failed");
            }
            Err(e) => {
                error!("Failed to parse Slack response: {}", e);
                return redirect_to("/auth/index.html?slack_bot=error&reason=parse_error");
            }
        },
        Ok(res) => {
            error!("Slack token exchange failed: {}", res.status());
            return redirect_to("/auth/index.html?slack_bot=error&reason=token_exchange_failed");
        }
        Err(e) => {
            error!("Slack token request failed: {}", e);
            return redirect_to("/auth/index.html?slack_bot=error&reason=request_failed");
        }
    };

    let slack_store = state.slack_store.clone();
    let installation_for_save = installation.clone();
    let save_result =
        task::spawn_blocking(move || slack_store.upsert_installation(&installation_for_save)).await;
    match save_result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            error!(
                "Failed to save Slack installation from auth callback: {}",
                err
            );
            return redirect_to("/auth/index.html?slack_bot=error&reason=save_failed");
        }
        Err(err) => {
            error!(
                "spawn_blocking panicked while saving Slack installation: {}",
                err
            );
            return redirect_to("/auth/index.html?slack_bot=error&reason=internal_error");
        }
    }

    // Record the bot installation event
    track_auth_event(
        &state.account_store,
        "slack_bot_installed",
        Some(account.id),
        Some(account.auth_user_id),
        Some(format!(
            "slack_bot_installed:{}:{}",
            account.id, installation.team_id
        )),
        Some("/auth/slack/bot-callback"),
        serde_json::json!({
            "team_id": installation.team_id.clone(),
            "team_name": installation.team_name.clone(),
            "bot_user_id": installation.bot_user_id.clone(),
        }),
    );

    let onboarding_state = state.clone();
    let onboarding_installation = installation.clone();
    let onboarding_event_nonce = oauth_callback_event_nonce(&params.code);
    let onboarding_result = task::spawn_blocking(move || {
        execute_slack_install_onboarding(
            &onboarding_state,
            account.id,
            account.auth_user_id,
            onboarding_installation,
            &onboarding_event_nonce,
        )
    })
    .await;

    match onboarding_result {
        Ok(Ok(result)) => {
            info!(
                "Slack install onboarding finished for account {} team {} (public={}, dm={})",
                account.id,
                installation.team_id,
                result.public_status.as_str(),
                result.dm_status.as_str()
            );
        }
        Ok(Err(err)) => {
            warn!(
                "Slack install onboarding failed for account {} team {}: {}",
                account.id, installation.team_id, err
            );
        }
        Err(err) => {
            warn!(
                "Slack install onboarding task join failed for account {} team {}: {}",
                account.id, installation.team_id, err
            );
        }
    }

    info!(
        "Slack bot installed for account {} in team {}",
        account.id, installation.team_id
    );

    // Include the token in fragment so frontend can restore the session
    let encoded_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token.as_bytes());
    Redirect::to(&format!(
        "{}/auth/index.html?slack_bot=success#access_token={}",
        frontend_url, encoded_token
    ))
    .into_response()
}

// ============================================================================
// GitHub OAuth
// ============================================================================

/// Query params for GitHub OAuth callback
#[derive(Debug, Deserialize)]
pub struct GitHubCallbackQuery {
    pub code: String,
    pub state: String,
}

/// GitHub token response
#[derive(Debug, Deserialize)]
struct GitHubTokenResponse {
    access_token: String,
    token_type: String,
}

/// GitHub user response
#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
    id: u64,
}

/// GET /auth/github
/// Initiates GitHub OAuth flow - redirects to GitHub's authorization page.
pub async fn github_oauth_start(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check if GitHub OAuth is configured
    let (client_id, redirect_uri) = match (&state.github_client_id, &state.github_redirect_uri) {
        (Some(id), Some(uri)) => (id.clone(), uri.clone()),
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "GitHub OAuth not configured"
                })),
            )
                .into_response();
        }
    };

    // Extract and validate Supabase token
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    // Validate the token to ensure user is authenticated
    if let Err((status, msg)) = validate_supabase_token(&state.supabase_url, &token).await {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // Encode the Supabase token in state so we can identify the user on callback
    let encoded_state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token.as_bytes());

    // Build GitHub OAuth URL (no scope needed - public profile gives us username)
    let github_auth_url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        encoded_state
    );

    // Return the URL for the frontend to redirect to
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "redirect_url": github_auth_url
        })),
    )
        .into_response()
}

/// GET /auth/github/callback
/// Handles GitHub OAuth callback - exchanges code for token, gets user info, links account.
pub async fn github_oauth_callback(
    State(state): State<AuthState>,
    Query(params): Query<GitHubCallbackQuery>,
) -> impl IntoResponse {
    // Helper to build redirect URLs to the frontend
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    // Check if GitHub OAuth is configured
    let (client_id, client_secret, redirect_uri) = match (
        &state.github_client_id,
        &state.github_client_secret,
        &state.github_redirect_uri,
    ) {
        (Some(id), Some(secret), Some(uri)) => (id.clone(), secret.clone(), uri.clone()),
        _ => {
            return redirect_to("/auth/index.html?github=error&reason=not_configured");
        }
    };

    // Decode state to get the Supabase token
    let token = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&params.state) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                return redirect_to("/auth/index.html?github=error&reason=invalid_state");
            }
        },
        Err(_) => {
            return redirect_to("/auth/index.html?github=error&reason=invalid_state");
        }
    };

    // Validate Supabase token and get user
    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err(_) => {
            return redirect_to("/auth/index.html?github=error&reason=invalid_token");
        }
    };

    // Exchange code for GitHub access token
    let client = reqwest::Client::new();
    let token_res = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", params.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await;

    let github_token = match token_res {
        Ok(res) if res.status().is_success() => match res.json::<GitHubTokenResponse>().await {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to parse GitHub token response: {}", e);
                return redirect_to("/auth/index.html?github=error&reason=token_parse_error");
            }
        },
        Ok(res) => {
            error!("GitHub token exchange failed: {}", res.status());
            return redirect_to("/auth/index.html?github=error&reason=token_exchange_failed");
        }
        Err(e) => {
            error!("GitHub token request failed: {}", e);
            return redirect_to("/auth/index.html?github=error&reason=token_request_failed");
        }
    };

    // Get GitHub user info
    let user_res = client
        .get("https://api.github.com/user")
        .header(
            "Authorization",
            format!("Bearer {}", github_token.access_token),
        )
        .header("User-Agent", "DoWhiz")
        .send()
        .await;

    let github_user = match user_res {
        Ok(res) if res.status().is_success() => match res.json::<GitHubUser>().await {
            Ok(u) => u,
            Err(e) => {
                error!("Failed to parse GitHub user response: {}", e);
                return redirect_to("/auth/index.html?github=error&reason=user_parse_error");
            }
        },
        Ok(res) => {
            error!("GitHub user request failed: {}", res.status());
            return redirect_to("/auth/index.html?github=error&reason=user_request_failed");
        }
        Err(e) => {
            error!("GitHub user request failed: {}", e);
            return redirect_to("/auth/index.html?github=error&reason=user_request_failed");
        }
    };

    info!(
        "GitHub OAuth successful for user {} (GitHub: {} / {})",
        auth_user_id, github_user.login, github_user.id
    );

    // Get user's account
    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id)).await;

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return redirect_to("/auth/index.html?github=error&reason=account_not_found");
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return redirect_to("/auth/index.html?github=error&reason=db_error");
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return redirect_to("/auth/index.html?github=error&reason=internal_error");
        }
    };

    // Link GitHub username to account
    let store = state.account_store.clone();
    let github_username = github_user.login.clone();
    let link_result = task::spawn_blocking(move || {
        store.create_identifier(account.id, "github", &github_username)
    })
    .await;

    match link_result {
        Ok(Ok(_identifier)) => {
            info!(
                "Linked GitHub {} to account {}",
                github_user.login, account.id
            );
            track_auth_event(
                &state.account_store,
                "channel_connect_succeeded",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect:{}:github:{}",
                    account.id, github_user.login
                )),
                Some("/auth/github/callback"),
                serde_json::json!({
                    "identifier_type": "github",
                    "identifier": github_user.login,
                    "provider": "github",
                }),
            );
            track_auth_event(
                &state.account_store,
                "first_channel_or_tool_connected",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!("first_channel:{}", account.id)),
                Some("/auth/github/callback"),
                serde_json::json!({
                    "identifier_type": "github",
                    "provider": "github",
                }),
            );
            redirect_to("/auth/index.html?github=success")
        }
        Ok(Err(AccountStoreError::IdentifierTaken)) => {
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:github:{}",
                    account.id, github_user.login
                )),
                Some("/auth/github/callback"),
                serde_json::json!({
                    "identifier_type": "github",
                    "identifier": github_user.login,
                    "error_reason": "identifier_taken",
                }),
            );
            redirect_to("/auth/index.html?github=error&reason=already_linked")
        }
        Ok(Err(e)) => {
            error!("Failed to link GitHub: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:github:{}",
                    account.id, github_user.login
                )),
                Some("/auth/github/callback"),
                serde_json::json!({
                    "identifier_type": "github",
                    "identifier": github_user.login,
                    "error_reason": "link_failed",
                }),
            );
            redirect_to("/auth/index.html?github=error&reason=link_failed")
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:github:{}",
                    account.id, github_user.login
                )),
                Some("/auth/github/callback"),
                serde_json::json!({
                    "identifier_type": "github",
                    "identifier": github_user.login,
                    "error_reason": "internal_error",
                }),
            );
            redirect_to("/auth/index.html?github=error&reason=internal_error")
        }
    }
}

// ============================================================================
// Notion OAuth
// ============================================================================

/// Query params for Notion OAuth callback
#[derive(Debug, Deserialize)]
pub struct NotionCallbackQuery {
    pub code: String,
    pub state: String,
}

/// Notion OAuth token response
#[derive(Debug, Deserialize)]
struct NotionTokenResponse {
    access_token: String,
    token_type: String,
    bot_id: String,
    workspace_id: String,
    workspace_name: Option<String>,
    workspace_icon: Option<String>,
    owner: NotionOwner,
}

#[derive(Debug, Deserialize)]
struct NotionOwner {
    #[serde(rename = "type")]
    owner_type: String,
    user: Option<NotionUser>,
}

#[derive(Debug, Deserialize)]
struct NotionUser {
    id: String,
    name: Option<String>,
    avatar_url: Option<String>,
    #[serde(rename = "type")]
    user_type: Option<String>,
    person: Option<NotionPerson>,
}

#[derive(Debug, Deserialize)]
struct NotionPerson {
    email: Option<String>,
}

/// GET /auth/notion
/// Initiates Notion OAuth flow - returns URL for frontend to redirect to.
pub async fn notion_oauth_start(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check if Notion OAuth is configured
    let (client_id, redirect_uri) = match (&state.notion_client_id, &state.notion_redirect_uri) {
        (Some(id), Some(uri)) => (id.clone(), uri.clone()),
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Notion OAuth not configured"
                })),
            )
                .into_response();
        }
    };

    // Extract and validate Supabase token
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    // Validate the token to ensure user is authenticated
    if let Err((status, msg)) = validate_supabase_token(&state.supabase_url, &token).await {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // Encode the Supabase token in state so we can identify the user on callback
    let encoded_state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token.as_bytes());
    info!(
        "Notion OAuth start: token_len={}, state_len={}",
        token.len(),
        encoded_state.len()
    );

    // Build Notion OAuth URL
    // Notion uses owner=user for user-level access (vs owner=workspace for workspace integration)
    let notion_auth_url = format!(
        "https://api.notion.com/v1/oauth/authorize?client_id={}&response_type=code&owner=user&redirect_uri={}&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        encoded_state
    );

    // Return the URL for the frontend to redirect to
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "redirect_url": notion_auth_url
        })),
    )
        .into_response()
}

/// GET /auth/notion/callback
/// Handles Notion OAuth callback - exchanges code for token, gets user info, links account.
pub async fn notion_oauth_callback(
    State(state): State<AuthState>,
    Query(params): Query<NotionCallbackQuery>,
) -> impl IntoResponse {
    // Helper to build redirect URLs to the frontend
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    info!(
        "Notion callback: code_len={}, state_id={}",
        params.code.len(),
        params.state
    );

    // Check if Notion OAuth is configured
    let (client_id, client_secret, redirect_uri) = match (
        &state.notion_client_id,
        &state.notion_client_secret,
        &state.notion_redirect_uri,
    ) {
        (Some(id), Some(secret), Some(uri)) => (id.clone(), secret.clone(), uri.clone()),
        _ => {
            return redirect_to("/auth/index.html?notion=error&reason=not_configured");
        }
    };

    // Decode state to get the Supabase token
    let token = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&params.state) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                return redirect_to("/auth/index.html?notion=error&reason=invalid_state");
            }
        },
        Err(_) => {
            return redirect_to("/auth/index.html?notion=error&reason=invalid_state");
        }
    };

    // Validate Supabase token and get user
    info!(
        "Notion callback: validating token (len={}, first_50={}...)",
        token.len(),
        &token[..token.len().min(50)]
    );
    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => {
            info!("Notion callback: token valid, user_id={}", user.id);
            user.id
        }
        Err((status, msg)) => {
            error!(
                "Notion callback: token validation failed: status={}, msg={}",
                status, msg
            );
            return redirect_to("/auth/index.html?notion=error&reason=invalid_token");
        }
    };

    // Exchange code for Notion access token
    // Notion uses Basic Auth with client_id:client_secret
    let client = reqwest::Client::new();
    let auth_header = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", client_id, client_secret));

    let token_res = client
        .post("https://api.notion.com/v1/oauth/token")
        .header("Authorization", format!("Basic {}", auth_header))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "code": params.code,
            "redirect_uri": redirect_uri
        }))
        .send()
        .await;

    let notion_token = match token_res {
        Ok(res) if res.status().is_success() => match res.json::<NotionTokenResponse>().await {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to parse Notion token response: {}", e);
                return redirect_to("/auth/index.html?notion=error&reason=token_parse_error");
            }
        },
        Ok(res) => {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            error!("Notion token exchange failed: {} - {}", status, body);
            return redirect_to("/auth/index.html?notion=error&reason=token_exchange_failed");
        }
        Err(e) => {
            error!("Notion token request failed: {}", e);
            return redirect_to("/auth/index.html?notion=error&reason=token_request_failed");
        }
    };

    // Extract user info from the token response
    let notion_user_id = notion_token.owner.user.as_ref().map(|u| u.id.clone());
    let notion_user_email = notion_token
        .owner
        .user
        .as_ref()
        .and_then(|u| u.person.as_ref())
        .and_then(|p| p.email.clone());

    info!(
        "Notion OAuth successful for user {} (Notion workspace: {} / user: {:?})",
        auth_user_id,
        notion_token.workspace_name.as_deref().unwrap_or("unknown"),
        notion_user_id
    );

    // Get user's account
    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id)).await;

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return redirect_to("/auth/index.html?notion=error&reason=account_not_found");
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return redirect_to("/auth/index.html?notion=error&reason=db_error");
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return redirect_to("/auth/index.html?notion=error&reason=internal_error");
        }
    };

    // Create a unique identifier for this Notion connection
    // We use workspace_id to allow users to connect multiple workspaces
    let notion_identifier = format!(
        "{}:{}",
        notion_token.workspace_id,
        notion_user_id.as_deref().unwrap_or("bot")
    );

    // Link Notion to account
    let store = state.account_store.clone();
    let link_result = task::spawn_blocking(move || {
        store.create_identifier(account.id, "notion", &notion_identifier)
    })
    .await;

    match link_result {
        Ok(Ok(_identifier)) => {
            info!(
                "Linked Notion workspace {} to account {}",
                notion_token.workspace_id, account.id
            );

            // Store the access_token in NotionStore for future API calls
            let credential = NotionCredential {
                account_id: account.id,
                workspace_id: notion_token.workspace_id.clone(),
                workspace_name: notion_token.workspace_name.clone(),
                access_token: notion_token.access_token.clone(),
                bot_id: notion_token.bot_id.clone(),
                owner_user_id: notion_user_id.clone(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            let store_result = task::spawn_blocking(move || match NotionStore::new() {
                Ok(store) => store.upsert_credential(&credential),
                Err(e) => {
                    error!("Failed to create NotionStore: {}", e);
                    Err(e)
                }
            })
            .await;

            match store_result {
                Ok(Ok(())) => {
                    info!(
                        "Stored Notion access token for workspace {} (token starts with: {}...)",
                        notion_token.workspace_id,
                        &notion_token.access_token[..8.min(notion_token.access_token.len())]
                    );
                }
                Ok(Err(e)) => {
                    error!("Failed to store Notion credential: {}", e);
                    // Continue anyway - the identifier is already linked
                }
                Err(e) => {
                    error!(
                        "spawn_blocking panicked while storing Notion credential: {}",
                        e
                    );
                }
            }

            redirect_to("/auth/index.html?notion=success")
        }
        Ok(Err(AccountStoreError::IdentifierTaken)) => {
            redirect_to("/auth/index.html?notion=error&reason=already_linked")
        }
        Ok(Err(e)) => {
            error!("Failed to link Notion: {}", e);
            redirect_to("/auth/index.html?notion=error&reason=link_failed")
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            redirect_to("/auth/index.html?notion=error&reason=internal_error")
        }
    }
}

// ============================================================================
// Lark OAuth (User Authentication)
// ============================================================================

/// Query params for Lark OAuth callback
#[derive(Debug, Deserialize)]
pub struct LarkCallbackQuery {
    pub code: String,
    pub state: String,
}

/// Lark app access token response
#[derive(Debug, Deserialize)]
struct LarkAppTokenResponse {
    code: i32,
    msg: Option<String>,
    app_access_token: Option<String>,
}

/// Lark user access token response
#[derive(Debug, Deserialize)]
struct LarkUserTokenResponse {
    code: i32,
    msg: Option<String>,
    data: Option<LarkUserTokenData>,
}

#[derive(Debug, Deserialize)]
struct LarkUserTokenData {
    access_token: String,
}

/// Lark user info response
#[derive(Debug, Deserialize)]
struct LarkUserInfoResponse {
    code: i32,
    msg: Option<String>,
    data: Option<LarkUserInfo>,
}

#[derive(Debug, Deserialize)]
struct LarkUserInfo {
    open_id: String,
    name: Option<String>,
}

/// GET /auth/lark
/// Initiates Lark OAuth flow - returns redirect URL to Lark's authorization page.
pub async fn lark_oauth_start(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check if Lark OAuth is configured
    let (client_id, redirect_uri) = match (&state.lark_client_id, &state.lark_redirect_uri) {
        (Some(id), Some(uri)) => (id.clone(), uri.clone()),
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Lark OAuth not configured"
                })),
            )
                .into_response();
        }
    };

    // Extract and validate Supabase token
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    // Validate the token to ensure user is authenticated
    if let Err((status, msg)) = validate_supabase_token(&state.supabase_url, &token).await {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // Encode the Supabase token in state so we can identify the user on callback
    let encoded_state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token.as_bytes());

    // Build Lark OAuth URL
    let lark_auth_url = format!(
        "https://open.feishu.cn/open-apis/authen/v1/authorize?app_id={}&redirect_uri={}&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        encoded_state
    );

    // Return the URL for the frontend to redirect to
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "redirect_url": lark_auth_url
        })),
    )
        .into_response()
}

/// GET /auth/lark/callback
/// Handles Lark OAuth callback - exchanges code for token, gets user info, links account.
pub async fn lark_oauth_callback(
    State(state): State<AuthState>,
    Query(params): Query<LarkCallbackQuery>,
) -> impl IntoResponse {
    // Helper to build redirect URLs to the frontend
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    // Check if Lark OAuth is configured
    let (client_id, client_secret, _redirect_uri) = match (
        &state.lark_client_id,
        &state.lark_client_secret,
        &state.lark_redirect_uri,
    ) {
        (Some(id), Some(secret), Some(uri)) => (id.clone(), secret.clone(), uri.clone()),
        _ => {
            return redirect_to("/auth/index.html?lark=error&reason=not_configured");
        }
    };

    // Decode state to get the Supabase token
    let token = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&params.state) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                return redirect_to("/auth/index.html?lark=error&reason=invalid_state");
            }
        },
        Err(_) => {
            return redirect_to("/auth/index.html?lark=error&reason=invalid_state");
        }
    };

    // Validate Supabase token and get user
    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err(_) => {
            return redirect_to("/auth/index.html?lark=error&reason=invalid_token");
        }
    };

    let client = reqwest::Client::new();

    // Step 1: Get app_access_token
    let app_token_res = client
        .post("https://open.feishu.cn/open-apis/auth/v3/app_access_token/internal")
        .json(&serde_json::json!({
            "app_id": client_id,
            "app_secret": client_secret,
        }))
        .send()
        .await;

    let app_access_token = match app_token_res {
        Ok(res) if res.status().is_success() => match res.json::<LarkAppTokenResponse>().await {
            Ok(t) if t.code == 0 => match t.app_access_token {
                Some(token) => token,
                None => {
                    error!("Lark app token response missing token");
                    return redirect_to("/auth/index.html?lark=error&reason=app_token_missing");
                }
            },
            Ok(t) => {
                error!("Lark app token error {}: {:?}", t.code, t.msg);
                return redirect_to("/auth/index.html?lark=error&reason=app_token_error");
            }
            Err(e) => {
                error!("Failed to parse Lark app token response: {}", e);
                return redirect_to("/auth/index.html?lark=error&reason=app_token_parse_error");
            }
        },
        Ok(res) => {
            error!("Lark app token request failed: {}", res.status());
            return redirect_to("/auth/index.html?lark=error&reason=app_token_request_failed");
        }
        Err(e) => {
            error!("Lark app token request failed: {}", e);
            return redirect_to("/auth/index.html?lark=error&reason=app_token_request_failed");
        }
    };

    // Step 2: Exchange code for user access token
    let user_token_res = client
        .post("https://open.feishu.cn/open-apis/authen/v1/oidc/access_token")
        .header("Authorization", format!("Bearer {}", app_access_token))
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "code": params.code,
        }))
        .send()
        .await;

    let user_access_token = match user_token_res {
        Ok(res) if res.status().is_success() => match res.json::<LarkUserTokenResponse>().await {
            Ok(t) if t.code == 0 => match t.data {
                Some(data) => data.access_token,
                None => {
                    error!("Lark user token response missing data");
                    return redirect_to("/auth/index.html?lark=error&reason=user_token_missing");
                }
            },
            Ok(t) => {
                error!("Lark user token error {}: {:?}", t.code, t.msg);
                return redirect_to("/auth/index.html?lark=error&reason=user_token_error");
            }
            Err(e) => {
                error!("Failed to parse Lark user token response: {}", e);
                return redirect_to("/auth/index.html?lark=error&reason=user_token_parse_error");
            }
        },
        Ok(res) => {
            error!("Lark user token exchange failed: {}", res.status());
            return redirect_to("/auth/index.html?lark=error&reason=token_exchange_failed");
        }
        Err(e) => {
            error!("Lark user token request failed: {}", e);
            return redirect_to("/auth/index.html?lark=error&reason=token_request_failed");
        }
    };

    // Step 3: Get user info
    let user_res = client
        .get("https://open.feishu.cn/open-apis/authen/v1/user_info")
        .header("Authorization", format!("Bearer {}", user_access_token))
        .send()
        .await;

    let lark_user = match user_res {
        Ok(res) if res.status().is_success() => match res.json::<LarkUserInfoResponse>().await {
            Ok(r) if r.code == 0 => match r.data {
                Some(user) => user,
                None => {
                    error!("Lark user info response missing data");
                    return redirect_to("/auth/index.html?lark=error&reason=user_info_missing");
                }
            },
            Ok(r) => {
                error!("Lark user info error {}: {:?}", r.code, r.msg);
                return redirect_to("/auth/index.html?lark=error&reason=user_info_error");
            }
            Err(e) => {
                error!("Failed to parse Lark user info response: {}", e);
                return redirect_to("/auth/index.html?lark=error&reason=user_parse_error");
            }
        },
        Ok(res) => {
            error!("Lark user info request failed: {}", res.status());
            return redirect_to("/auth/index.html?lark=error&reason=user_request_failed");
        }
        Err(e) => {
            error!("Lark user info request failed: {}", e);
            return redirect_to("/auth/index.html?lark=error&reason=user_request_failed");
        }
    };

    info!(
        "Lark OAuth successful for user {} (Lark: {} / {})",
        auth_user_id,
        lark_user.name.as_deref().unwrap_or("unknown"),
        lark_user.open_id
    );

    // Step 4: Get user's DoWhiz account
    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id)).await;

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return redirect_to("/auth/index.html?lark=error&reason=account_not_found");
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return redirect_to("/auth/index.html?lark=error&reason=db_error");
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return redirect_to("/auth/index.html?lark=error&reason=internal_error");
        }
    };

    // Step 5: Link Lark open_id to account
    let store = state.account_store.clone();
    let lark_open_id = lark_user.open_id.clone();
    let link_result =
        task::spawn_blocking(move || store.create_identifier(account.id, "lark", &lark_open_id))
            .await;

    match link_result {
        Ok(Ok(_identifier)) => {
            info!(
                "Linked Lark {} to account {}",
                lark_user.open_id, account.id
            );
            track_auth_event(
                &state.account_store,
                "channel_connect_succeeded",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect:{}:lark:{}",
                    account.id, lark_user.open_id
                )),
                Some("/auth/lark/callback"),
                serde_json::json!({
                    "identifier_type": "lark",
                    "identifier": lark_user.open_id,
                    "provider": "lark",
                    "user_name": lark_user.name,
                }),
            );
            redirect_to("/auth/index.html?lark=success")
        }
        Ok(Err(AccountStoreError::IdentifierTaken)) => {
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:lark:{}",
                    account.id, lark_user.open_id
                )),
                Some("/auth/lark/callback"),
                serde_json::json!({
                    "identifier_type": "lark",
                    "identifier": lark_user.open_id,
                    "error_reason": "identifier_taken",
                }),
            );
            redirect_to("/auth/index.html?lark=error&reason=already_linked")
        }
        Ok(Err(e)) => {
            error!("Failed to link Lark: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:lark:{}",
                    account.id, lark_user.open_id
                )),
                Some("/auth/lark/callback"),
                serde_json::json!({
                    "identifier_type": "lark",
                    "identifier": lark_user.open_id,
                    "error_reason": "link_failed",
                }),
            );
            redirect_to("/auth/index.html?lark=error&reason=link_failed")
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            redirect_to("/auth/index.html?lark=error&reason=internal_error")
        }
    }
}

// ============================================================================
// WeCom OAuth (Enterprise WeChat)
// ============================================================================

/// Query params for WeCom OAuth callback
#[derive(Debug, Deserialize)]
pub struct WeComCallbackQuery {
    pub code: String,
    pub state: String,
}

/// WeCom access token response
#[derive(Debug, Deserialize)]
struct WeComAccessTokenResponse {
    errcode: Option<i32>,
    errmsg: Option<String>,
    access_token: Option<String>,
    expires_in: Option<i64>,
}

/// WeCom user info response
#[derive(Debug, Deserialize)]
struct WeComUserInfoResponse {
    errcode: Option<i32>,
    errmsg: Option<String>,
    #[serde(rename = "UserId")]
    user_id: Option<String>,
    #[serde(rename = "OpenId")]
    open_id: Option<String>,
    // QR code login returns lowercase userid
    userid: Option<String>,
    user_ticket: Option<String>,
}

/// Ephemeral KV store for WeCom OAuth state
/// Maps short state key -> (supabase_token, created_at)
/// TTL: 10 minutes (OAuth should complete quickly)
const WECOM_OAUTH_STATE_TTL_SECS: u64 = 600;

fn wecom_oauth_states() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    static STATES: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Store a token with a short key, returns the key
fn store_wecom_oauth_state(token: &str) -> String {
    let key = Uuid::new_v4().to_string(); // 36 chars, well under 128 limit
    let mut store = wecom_oauth_states().lock().expect("wecom oauth state lock");

    // Prune expired entries
    let now = Instant::now();
    store.retain(|_, (_, created)| {
        now.duration_since(*created).as_secs() < WECOM_OAUTH_STATE_TTL_SECS
    });

    store.insert(key.clone(), (token.to_string(), now));
    info!(
        "WeCom OAuth: stored state key={} (token_len={}, store_size={})",
        key,
        token.len(),
        store.len()
    );
    key
}

/// Retrieve and remove a token by key (one-time use)
fn take_wecom_oauth_state(key: &str) -> Option<String> {
    let mut store = wecom_oauth_states().lock().expect("wecom oauth state lock");

    // Prune expired entries
    let now = Instant::now();
    store.retain(|_, (_, created)| {
        now.duration_since(*created).as_secs() < WECOM_OAUTH_STATE_TTL_SECS
    });

    let result = store.remove(key);
    match &result {
        Some((token, _)) => info!(
            "WeCom OAuth: retrieved state key={} (token_len={}, remaining_store_size={})",
            key,
            token.len(),
            store.len()
        ),
        None => info!(
            "WeCom OAuth: state key={} not found (store_size={})",
            key,
            store.len()
        ),
    }
    result.map(|(token, _)| token)
}

/// GET /auth/wechat
/// Initiates WeCom OAuth flow - returns redirect URL to WeCom's authorization page.
pub async fn wecom_oauth_start(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check if WeCom OAuth is configured
    let (corp_id, agent_id, redirect_uri) = match (
        &state.wechat_corp_id,
        &state.wechat_agent_id,
        &state.wechat_redirect_uri,
    ) {
        (Some(id), Some(aid), Some(uri)) => (id.clone(), aid.clone(), uri.clone()),
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "WeCom OAuth not configured"
                })),
            )
                .into_response();
        }
    };

    // Extract and validate Supabase token
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    // Validate the token to ensure user is authenticated
    if let Err((status, msg)) = validate_supabase_token(&state.supabase_url, &token).await {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // Store token in ephemeral KV, use short key as state (WeChat has 128 char limit)
    let state_key = store_wecom_oauth_state(&token);

    // Build WeCom QR Code Login URL
    // This endpoint shows a QR code that users scan with WeCom app - works in any browser
    // (The open.weixin.qq.com/connect/oauth2/authorize endpoint only works inside WeCom's built-in browser)
    let wecom_auth_url = format!(
        "https://login.work.weixin.qq.com/wwlogin/sso/login?login_type=CorpApp&appid={}&agentid={}&redirect_uri={}&state={}",
        corp_id,
        agent_id,
        urlencoding::encode(&redirect_uri),
        state_key
    );

    info!(
        "WeCom OAuth start (QR login): corp_id={} agent_id={} redirect_uri={} full_url={}",
        corp_id, agent_id, redirect_uri, wecom_auth_url
    );

    // Return the URL for the frontend to redirect to
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "redirect_url": wecom_auth_url
        })),
    )
        .into_response()
}

/// GET /auth/wechat/callback
/// Handles WeCom OAuth callback - exchanges code for user identity, links account.
pub async fn wecom_oauth_callback(
    State(state): State<AuthState>,
    Query(params): Query<WeComCallbackQuery>,
) -> impl IntoResponse {
    info!(
        "WeCom OAuth callback received: code={} state_len={}",
        params.code,
        params.state.len()
    );

    // Helper to build redirect URLs to the frontend
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    // Check if WeCom OAuth is configured
    let (corp_id, corp_secret) = match (&state.wechat_corp_id, &state.wechat_corp_secret) {
        (Some(id), Some(secret)) => (id.clone(), secret.clone()),
        _ => {
            return redirect_to("/auth/index.html?wechat=error&reason=not_configured");
        }
    };

    // Retrieve token from ephemeral KV store (one-time use)
    let token = match take_wecom_oauth_state(&params.state) {
        Some(t) => t,
        None => {
            warn!("WeCom OAuth state not found or expired: {}", params.state);
            return redirect_to("/auth/index.html?wechat=error&reason=invalid_state");
        }
    };

    // Validate Supabase token and get user
    let auth_user_id = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user.id,
        Err(_) => {
            return redirect_to("/auth/index.html?wechat=error&reason=invalid_token");
        }
    };

    let client = reqwest::Client::new();

    // Step 1: Get corp access_token
    let access_token_url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={}&corpsecret={}",
        corp_id, corp_secret
    );

    let access_token_res = client.get(&access_token_url).send().await;

    let access_token = match access_token_res {
        Ok(res) if res.status().is_success() => {
            match res.json::<WeComAccessTokenResponse>().await {
                Ok(t) if t.errcode.unwrap_or(0) == 0 => match t.access_token {
                    Some(token) => token,
                    None => {
                        error!("WeCom access token response missing token");
                        return redirect_to(
                            "/auth/index.html?wechat=error&reason=access_token_missing",
                        );
                    }
                },
                Ok(t) => {
                    error!(
                        "WeCom access token error {}: {:?}",
                        t.errcode.unwrap_or(-1),
                        t.errmsg
                    );
                    return redirect_to("/auth/index.html?wechat=error&reason=access_token_error");
                }
                Err(e) => {
                    error!("Failed to parse WeCom access token response: {}", e);
                    return redirect_to(
                        "/auth/index.html?wechat=error&reason=access_token_parse_error",
                    );
                }
            }
        }
        Ok(res) => {
            error!("WeCom access token request failed: {}", res.status());
            return redirect_to("/auth/index.html?wechat=error&reason=access_token_request_failed");
        }
        Err(e) => {
            error!("WeCom access token request failed: {}", e);
            return redirect_to("/auth/index.html?wechat=error&reason=access_token_request_failed");
        }
    };

    // Step 2: Get user identity using the OAuth code
    let user_info_url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/auth/getuserinfo?access_token={}&code={}",
        access_token, params.code
    );

    let user_res = client.get(&user_info_url).send().await;

    let (wecom_user_id, wecom_display) = match user_res {
        Ok(res) if res.status().is_success() => {
            let body = res.text().await.unwrap_or_default();
            info!("WeCom user info raw response: {}", body);
            match serde_json::from_str::<WeComUserInfoResponse>(&body) {
                Ok(r) if r.errcode.unwrap_or(0) == 0 => {
                    // UserId/userid is for internal employees, OpenId is for external contacts
                    // QR code login returns lowercase "userid", OAuth returns "UserId"
                    let effective_user_id = r.user_id.or(r.userid);
                    match (effective_user_id, r.open_id) {
                        (Some(uid), _) => (uid.clone(), uid),
                        (None, Some(oid)) => (oid.clone(), format!("external:{}", oid)),
                        (None, None) => {
                            error!(
                                "WeCom user info response missing both UserId and OpenId: {:?}",
                                r.user_ticket
                            );
                            return redirect_to(
                                "/auth/index.html?wechat=error&reason=user_info_missing",
                            );
                        }
                    }
                }
                Ok(r) => {
                    error!(
                        "WeCom user info error {}: {:?}",
                        r.errcode.unwrap_or(-1),
                        r.errmsg
                    );
                    return redirect_to("/auth/index.html?wechat=error&reason=user_info_error");
                }
                Err(e) => {
                    error!("Failed to parse WeCom user info response: {}", e);
                    return redirect_to("/auth/index.html?wechat=error&reason=user_parse_error");
                }
            }
        }
        Ok(res) => {
            error!("WeCom user info request failed: {}", res.status());
            return redirect_to("/auth/index.html?wechat=error&reason=user_request_failed");
        }
        Err(e) => {
            error!("WeCom user info request failed: {}", e);
            return redirect_to("/auth/index.html?wechat=error&reason=user_request_failed");
        }
    };

    info!(
        "WeCom OAuth successful for user {} (WeCom: {} in corp {})",
        auth_user_id, wecom_display, corp_id
    );

    // Step 3: Get user's DoWhiz account
    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user_id)).await;

    let account = match account_result {
        Ok(Ok(Some(acc))) => acc,
        Ok(Ok(None)) => {
            return redirect_to("/auth/index.html?wechat=error&reason=account_not_found");
        }
        Ok(Err(e)) => {
            error!("Failed to get account: {}", e);
            return redirect_to("/auth/index.html?wechat=error&reason=db_error");
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return redirect_to("/auth/index.html?wechat=error&reason=internal_error");
        }
    };

    // Step 4: Link WeCom identifier to account
    // Format: {corp_id}_{user_id} to avoid collisions across corps
    let wecom_identifier = format!("{}_{}", corp_id, wecom_user_id);
    let store = state.account_store.clone();
    let identifier_for_link = wecom_identifier.clone();
    let link_result = task::spawn_blocking(move || {
        store.create_identifier(account.id, "wechat", &identifier_for_link)
    })
    .await;

    match link_result {
        Ok(Ok(_identifier)) => {
            info!(
                "Linked WeCom {} to account {}",
                wecom_identifier, account.id
            );
            track_auth_event(
                &state.account_store,
                "channel_connect_succeeded",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect:{}:wechat:{}",
                    account.id, wecom_identifier
                )),
                Some("/auth/wechat/callback"),
                serde_json::json!({
                    "identifier_type": "wechat",
                    "identifier": wecom_identifier,
                    "provider": "wecom",
                    "corp_id": corp_id,
                    "user_id": wecom_user_id,
                }),
            );
            redirect_to("/auth/index.html?wechat=success")
        }
        Ok(Err(AccountStoreError::IdentifierTaken)) => {
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:wechat:{}",
                    account.id, wecom_identifier
                )),
                Some("/auth/wechat/callback"),
                serde_json::json!({
                    "identifier_type": "wechat",
                    "identifier": wecom_identifier,
                    "error_reason": "identifier_taken",
                }),
            );
            redirect_to("/auth/index.html?wechat=error&reason=already_linked")
        }
        Ok(Err(e)) => {
            error!("Failed to link WeCom: {}", e);
            track_auth_event(
                &state.account_store,
                "channel_connect_failed",
                Some(account.id),
                Some(account.auth_user_id),
                Some(format!(
                    "channel_connect_failed:{}:wechat:{}",
                    account.id, wecom_identifier
                )),
                Some("/auth/wechat/callback"),
                serde_json::json!({
                    "identifier_type": "wechat",
                    "identifier": wecom_identifier,
                    "error_reason": "link_failed",
                }),
            );
            redirect_to("/auth/index.html?wechat=error&reason=link_failed")
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            redirect_to("/auth/index.html?wechat=error&reason=internal_error")
        }
    }
}

// ============================================================================
// Email Verification
// ============================================================================

/// Send a verification email with a magic link
async fn send_verification_email(
    email: &str,
    verify_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let postmark_token = std::env::var("POSTMARK_SERVER_TOKEN")
        .map_err(|_| "POSTMARK_SERVER_TOKEN not configured")?;
    let from_email =
        std::env::var("POSTMARK_FROM_EMAIL").unwrap_or_else(|_| "noreply@dowhiz.com".to_string());

    let html_body = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Verify your email</title>
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; padding: 40px; background: #f5f5f5;">
    <div style="max-width: 500px; margin: 0 auto; background: white; border-radius: 8px; padding: 40px; box-shadow: 0 2px 8px rgba(0,0,0,0.1);">
        <h1 style="margin: 0 0 20px; color: #333;">Verify your email</h1>
        <p style="color: #666; line-height: 1.6;">Click the button below to verify your email address and link it to your DoWhiz account.</p>
        <a href="{}" style="display: inline-block; margin: 20px 0; padding: 12px 24px; background: #333; color: white; text-decoration: none; border-radius: 6px; font-weight: 500;">Verify Email</a>
        <p style="color: #999; font-size: 14px; margin-top: 30px;">This link expires in 24 hours. If you didn't request this, you can ignore this email.</p>
    </div>
</body>
</html>"#,
        verify_url
    );

    let text_body = format!(
        "Verify your email\n\nClick the link below to verify your email address:\n{}\n\nThis link expires in 24 hours.",
        verify_url
    );

    let client = reqwest::Client::new();
    let res = client
        .post("https://api.postmarkapp.com/email")
        .header("X-Postmark-Server-Token", &postmark_token)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "From": from_email,
            "To": email,
            "Subject": "Verify your email for DoWhiz",
            "HtmlBody": html_body,
            "TextBody": text_body,
            "MessageStream": "outbound"
        }))
        .send()
        .await?;

    if !res.status().is_success() {
        let error_text = res.text().await.unwrap_or_default();
        return Err(format!("Postmark error: {}", error_text).into());
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailQuery {
    pub token: String,
}

/// GET /auth/verify-email?token=<token>
/// Verify an email address via magic link.
pub async fn verify_email(
    State(state): State<AuthState>,
    Query(query): Query<VerifyEmailQuery>,
) -> impl IntoResponse {
    let frontend_url = state.frontend_url.clone();
    let redirect_to = |path: &str| -> axum::response::Response {
        Redirect::to(&format!("{}{}", frontend_url, path)).into_response()
    };

    let store = state.account_store.clone();
    let token = query.token.clone();

    let verify_result = task::spawn_blocking(move || store.verify_email_token(&token))
        .await
        .map_err(|e| {
            error!("spawn_blocking panicked: {}", e);
            "Internal error"
        });

    match verify_result {
        Ok(Ok(identifier)) => {
            info!(
                "Email {} verified for account {}",
                identifier.identifier, identifier.account_id
            );
            track_auth_event(
                &state.account_store,
                "channel_connect_succeeded",
                Some(identifier.account_id),
                None,
                Some(format!(
                    "channel_connect:{}:email:{}",
                    identifier.account_id, identifier.identifier
                )),
                Some("/auth/verify-email"),
                serde_json::json!({
                    "identifier_type": "email",
                    "identifier": identifier.identifier,
                    "verification_method": "email_link",
                }),
            );
            track_auth_event(
                &state.account_store,
                "first_channel_or_tool_connected",
                Some(identifier.account_id),
                None,
                Some(format!("first_channel:{}", identifier.account_id)),
                Some("/auth/verify-email"),
                serde_json::json!({
                    "identifier_type": "email",
                    "provider": "email",
                }),
            );
            redirect_to("/auth/index.html?email_verified=success")
        }
        Ok(Err(AccountStoreError::TokenInvalid)) => {
            warn!("Invalid or expired email verification token");
            redirect_to("/auth/index.html?email_verified=error&reason=invalid_token")
        }
        Ok(Err(e)) => {
            error!("Failed to verify email: {}", e);
            redirect_to("/auth/index.html?email_verified=error&reason=database_error")
        }
        Err(_) => redirect_to("/auth/index.html?email_verified=error&reason=internal_error"),
    }
}

// ============================================================================
// Tasks
// ============================================================================

#[derive(Debug, Serialize)]
pub struct TasksResponse {
    pub tasks: Vec<TaskStatusSummary>,
}

#[derive(Debug, Serialize)]
pub struct RoutinesResponse {
    pub active: Vec<RoutineSummary>,
    pub history: Vec<RoutineSummary>,
}

#[derive(Debug, Serialize)]
struct RoutineMutationResponse {
    ok: bool,
    task_id: String,
}

#[derive(Debug, Serialize)]
struct TaskDetailResponse {
    task: TaskStatusSummary,
    executions: Vec<TaskExecutionSummary>,
}

#[derive(Debug, Serialize)]
struct TaskMutationResponse {
    ok: bool,
    task_id: String,
    resubmitted_task_id: Option<String>,
}

#[derive(Debug, Clone)]
struct TaskStorageMatch {
    path: PathBuf,
    task: ScheduledTask,
    summary: TaskStatusSummary,
    executions: Vec<TaskExecutionSummary>,
}

#[derive(Debug, Clone)]
struct TaskMutationOutcome {
    task_id: String,
    resubmitted_task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TasksQuery {
    pub channel: Option<String>,
    pub identifier: Option<String>,
}

/// GET /api/tasks?channel=discord&identifier=123456789
/// Returns tasks for a specific channel identifier.
pub async fn get_tasks(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Query(query): Query<TasksQuery>,
) -> impl IntoResponse {
    // Validate auth token
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    if let Err((status, msg)) = validate_supabase_token(&state.supabase_url, &token).await {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // Require channel and identifier
    let (channel, identifier) = match (query.channel, query.identifier) {
        (Some(c), Some(i)) => (c, i),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Missing required query params: channel and identifier"
                })),
            )
                .into_response();
        }
    };

    // Check if user_store is configured
    let (user_store, users_root) = match (&state.user_store, &state.users_root) {
        (Some(store), Some(root)) => (store.clone(), root.clone()),
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Task storage not configured"
                })),
            )
                .into_response();
        }
    };

    // Look up user by channel + identifier
    let user_result = task::spawn_blocking({
        let user_store = user_store.clone();
        move || user_store.get_user_by_identifier(&channel, &identifier)
    })
    .await;

    let user_record = match user_result {
        Ok(Ok(Some(record))) => record,
        Ok(Ok(None)) => {
            // No user found - return empty tasks (user hasn't interacted with bot yet)
            return (StatusCode::OK, Json(TasksResponse { tasks: Vec::new() })).into_response();
        }
        Ok(Err(e)) => {
            error!("Error looking up user: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to look up user" })),
            )
                .into_response();
        }
        Err(e) => {
            error!("spawn_blocking panicked: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal error" })),
            )
                .into_response();
        }
    };

    // Load tasks for this user
    let paths = user_store.user_paths(&users_root, &user_record.user_id);
    let tasks = match load_task_statuses_or_response(&paths.tasks_db_path, "channel-scoped") {
        Ok(tasks) => tasks,
        Err(response) => return response,
    };

    (StatusCode::OK, Json(TasksResponse { tasks })).into_response()
}

/// GET /api/account/tasks
/// Returns all tasks for the authenticated user's unified account.
/// This fetches from the account-level tasks.db which aggregates tasks from all channels.
/// For Slack, it also fetches from legacy user storage since Slack task status updates
/// go to legacy storage (because reply_to contains channel_id, not user_id).
pub async fn get_account_tasks(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    let task_paths = match load_unified_account_task_paths(&state, account.id).await {
        Ok(paths) => paths,
        Err(response) => return response,
    };

    // Clone paths for the blocking closure
    let paths_for_blocking = task_paths.clone();
    let load_result = task::spawn_blocking(move || {
        let mut tasks = Vec::new();
        for task_path in paths_for_blocking {
            let loaded = load_task_statuses_or_response(&task_path, "account-scoped")?;
            tasks = merge_task_summaries(tasks, loaded);
        }
        sort_task_summaries(&mut tasks);
        Ok::<_, Response>(tasks)
    })
    .await;

    match load_result {
        Ok(Ok(tasks)) => (StatusCode::OK, Json(TasksResponse { tasks })).into_response(),
        Ok(Err(response)) => response,
        Err(err) => {
            error!("spawn_blocking panicked loading account tasks: {}", err);
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load tasks")
        }
    }
}

/// GET /api/account/tasks/:task_id
/// Returns the current task summary together with execution history for one task.
pub async fn get_account_task_detail(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    let task_paths = match load_unified_account_task_paths(&state, account.id).await {
        Ok(paths) => paths,
        Err(response) => return response,
    };
    let task_id_for_log = task_id.clone();

    let detail_result =
        task::spawn_blocking(move || load_account_task_detail_blocking(&task_paths, &task_id))
            .await;

    match detail_result {
        Ok(Ok(Some(detail))) => (StatusCode::OK, Json(detail)).into_response(),
        Ok(Ok(None)) => json_error_response(StatusCode::NOT_FOUND, "Task not found"),
        Ok(Err(response)) => response,
        Err(err) => {
            error!(
                "spawn_blocking panicked while loading task detail {}: {}",
                task_id_for_log, err
            );
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        }
    }
}

/// POST /api/account/tasks/:task_id/cancel
pub async fn cancel_account_task(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    mutate_account_task_endpoint(state, headers, task_id, TaskMutationAction::Cancel).await
}

/// POST /api/account/tasks/:task_id/resubmit
pub async fn resubmit_account_task(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    mutate_account_task_endpoint(state, headers, task_id, TaskMutationAction::Resubmit).await
}

fn load_account_task_detail_blocking(
    task_paths: &[PathBuf],
    task_id: &str,
) -> Result<Option<TaskDetailResponse>, Response> {
    let matches = load_task_storage_matches_blocking(task_paths, task_id)?;
    let Some(selected_idx) = preferred_task_match_index(&matches) else {
        return Ok(None);
    };
    let executions = merge_task_execution_summaries(&matches);
    let selected = matches[selected_idx].clone();
    Ok(Some(TaskDetailResponse {
        task: selected.summary,
        executions,
    }))
}

fn load_task_storage_matches_blocking(
    task_paths: &[PathBuf],
    task_id: &str,
) -> Result<Vec<TaskStorageMatch>, Response> {
    let mut matches = Vec::new();
    for task_path in task_paths {
        let task = load_scheduled_task(task_path, task_id).map_err(|err| {
            error!(
                "failed to load task {} from {}: {}",
                task_id,
                task_path.display(),
                err
            );
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load task")
        })?;
        let Some(task) = task else {
            continue;
        };

        let summary = try_load_task_with_status(task_path, task_id)
            .map_err(|err| {
                error!(
                    "failed to load task status {} from {}: {}",
                    task_id,
                    task_path.display(),
                    err
                );
                json_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load task status",
                )
            })?
            .ok_or_else(|| {
                json_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load task status",
                )
            })?;

        let executions = try_load_task_executions(task_path, task_id).map_err(|err| {
            error!(
                "failed to load task executions {} from {}: {}",
                task_id,
                task_path.display(),
                err
            );
            json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load task history",
            )
        })?;

        matches.push(TaskStorageMatch {
            path: task_path.clone(),
            task,
            summary,
            executions,
        });
    }
    Ok(matches)
}

fn preferred_task_match_index(matches: &[TaskStorageMatch]) -> Option<usize> {
    let mut best_idx = 0usize;
    let mut found = false;
    for (idx, candidate) in matches.iter().enumerate() {
        if !found {
            best_idx = idx;
            found = true;
            continue;
        }
        if should_prefer_task_summary(&candidate.summary, &matches[best_idx].summary) {
            best_idx = idx;
        }
    }
    found.then_some(best_idx)
}

fn merge_task_execution_summaries(matches: &[TaskStorageMatch]) -> Vec<TaskExecutionSummary> {
    let mut seen = HashSet::new();
    let mut executions = Vec::new();

    for task_match in matches {
        for execution in &task_match.executions {
            let dedupe_key = format!(
                "{}|{}|{}|{}|{}",
                execution.status,
                execution.started_at,
                execution.finished_at.as_deref().unwrap_or(""),
                execution.error_message.as_deref().unwrap_or(""),
                execution.duration_seconds.unwrap_or_default()
            );
            if seen.insert(dedupe_key) {
                executions.push(execution.clone());
            }
        }
    }

    executions.sort_by(|left, right| {
        parse_rfc3339_utc(Some(right.started_at.as_str()))
            .cmp(&parse_rfc3339_utc(Some(left.started_at.as_str())))
            .then_with(|| right.execution_id.cmp(&left.execution_id))
    });
    executions
}

#[derive(Debug, Clone, Copy)]
enum TaskMutationAction {
    Cancel,
    Resubmit,
}

async fn mutate_account_task_endpoint(
    state: AuthState,
    headers: HeaderMap,
    task_id: String,
    action: TaskMutationAction,
) -> Response {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    match mutate_unified_account_task(&state, account.id, &task_id, action).await {
        Ok(Some(outcome)) => (
            StatusCode::OK,
            Json(TaskMutationResponse {
                ok: true,
                task_id: outcome.task_id,
                resubmitted_task_id: outcome.resubmitted_task_id,
            }),
        )
            .into_response(),
        Ok(None) => json_error_response(StatusCode::NOT_FOUND, "Task not found"),
        Err(response) => response,
    }
}

async fn mutate_unified_account_task(
    state: &AuthState,
    account_id: Uuid,
    task_id: &str,
    action: TaskMutationAction,
) -> Result<Option<TaskMutationOutcome>, Response> {
    let task_paths = load_unified_account_task_paths(state, account_id).await?;
    let primary_account_task_path = task_paths.first().cloned();
    let task_id = task_id.to_string();
    let action_name = match action {
        TaskMutationAction::Cancel => "cancel",
        TaskMutationAction::Resubmit => "resubmit",
    }
    .to_string();
    let task_id_for_log = task_id.clone();
    let action_name_for_log = action_name.clone();

    task::spawn_blocking(move || {
        mutate_unified_account_task_blocking(
            &task_paths,
            primary_account_task_path.as_deref(),
            &task_id,
            action,
            &action_name,
        )
    })
    .await
    .map_err(|err| {
        error!(
            "spawn_blocking panicked while applying task action {} to {}: {}",
            action_name_for_log, task_id_for_log, err
        );
        json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    })?
}

fn mutate_unified_account_task_blocking(
    task_paths: &[PathBuf],
    primary_account_task_path: Option<&std::path::Path>,
    task_id: &str,
    action: TaskMutationAction,
    action_name: &str,
) -> Result<Option<TaskMutationOutcome>, Response> {
    let matches = load_task_storage_matches_blocking(task_paths, task_id)?;
    let Some(selected_idx) = preferred_task_match_index(&matches) else {
        return Ok(None);
    };
    let selected = matches[selected_idx].clone();

    match action {
        TaskMutationAction::Cancel => {
            cancel_task_matches(&matches, &selected, action_name)?;
            Ok(Some(TaskMutationOutcome {
                task_id: task_id.to_string(),
                resubmitted_task_id: None,
            }))
        }
        TaskMutationAction::Resubmit => {
            if !selected.summary.can_resubmit {
                return Err(json_error_response(
                    StatusCode::CONFLICT,
                    "Only failed or expired workflow tasks can be resubmitted safely.",
                ));
            }
            let write_target_idx = primary_account_task_path
                .and_then(|path| preferred_task_write_match_index(&matches, path))
                .unwrap_or(selected_idx);
            let write_target = matches[write_target_idx].clone();
            let resubmitted_task = build_resubmitted_task(&selected.task, Utc::now())
                .map_err(|message| json_error_response(StatusCode::CONFLICT, &message))?;
            insert_scheduled_task(&write_target.path, &resubmitted_task).map_err(|err| {
                error!(
                    "failed to insert resubmitted task {} into {}: {}",
                    resubmitted_task.id,
                    write_target.path.display(),
                    err
                );
                json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to resubmit task")
            })?;
            if let Some(account_path) = primary_account_task_path {
                if account_path != write_target.path.as_path() {
                    insert_scheduled_task(account_path, &resubmitted_task).map_err(|err| {
                        error!(
                            "failed to mirror resubmitted task {} into {}: {}",
                            resubmitted_task.id,
                            account_path.display(),
                            err
                        );
                        json_error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Failed to mirror resubmitted task",
                        )
                    })?;
                }
            }
            Ok(Some(TaskMutationOutcome {
                task_id: task_id.to_string(),
                resubmitted_task_id: Some(resubmitted_task.id.to_string()),
            }))
        }
    }
}

fn cancel_task_matches(
    matches: &[TaskStorageMatch],
    selected: &TaskStorageMatch,
    action_name: &str,
) -> Result<(), Response> {
    if !selected.summary.can_cancel {
        return Err(json_error_response(
            StatusCode::CONFLICT,
            "Task cannot be cancelled in its current state.",
        ));
    }

    if selected.summary.status == "running" {
        let TaskKind::RunTask(run_task) = &selected.task.kind else {
            return Err(json_error_response(
                StatusCode::CONFLICT,
                "Only running workflow tasks support cancellation right now.",
            ));
        };
        request_running_task_cancellation(run_task)
            .map_err(|message| json_error_response(StatusCode::CONFLICT, &message))?;
    }

    let cancelled_at = Utc::now();
    for task_match in matches {
        let mut updated_task = task_match.task.clone();
        updated_task.enabled = false;
        persist_scheduled_task(&task_match.path, &updated_task).map_err(|err| {
            error!(
                "failed to persist task {} to {} during {}: {}",
                updated_task.id,
                task_match.path.display(),
                action_name,
                err
            );
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to update task")
        })?;

        if selected.summary.status != "running" {
            append_task_execution_event(
                &task_match.path,
                &updated_task.id.to_string(),
                cancelled_at,
                Some(cancelled_at),
                "cancelled",
                Some("Cancelled from the dashboard."),
            )
            .map_err(|err| {
                error!(
                    "failed to append cancelled event for task {} in {}: {}",
                    updated_task.id,
                    task_match.path.display(),
                    err
                );
                json_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to record task cancellation",
                )
            })?;
        }
    }

    Ok(())
}

fn preferred_task_write_match_index(
    matches: &[TaskStorageMatch],
    primary_account_task_path: &std::path::Path,
) -> Option<usize> {
    let mut best_idx: Option<usize> = None;
    for (idx, candidate) in matches.iter().enumerate() {
        if candidate.path.as_path() == primary_account_task_path {
            continue;
        }
        match best_idx {
            Some(existing_idx)
                if !should_prefer_task_summary(
                    &candidate.summary,
                    &matches[existing_idx].summary,
                ) => {}
            _ => best_idx = Some(idx),
        }
    }
    best_idx
}

fn request_running_task_cancellation(task: &crate::RunTaskTask) -> Result<(), String> {
    let state_path = task
        .thread_state_path
        .clone()
        .unwrap_or_else(|| default_thread_state_path(&task.workspace_dir));
    let expected_epoch = task.thread_epoch.unwrap_or(0);
    let now = Utc::now().to_rfc3339();

    if let Some(mut state) = load_thread_state(&state_path) {
        if state.epoch <= expected_epoch {
            let next_epoch = state.epoch.max(expected_epoch).saturating_add(1).max(1);
            state.epoch = next_epoch;
            state.last_email_seq = state.last_email_seq.max(next_epoch);
            state.updated_at = now;
            write_thread_state(&state_path, &state).map_err(|err| {
                format!(
                    "Failed to write thread state for cancellation at {}: {}",
                    state_path.display(),
                    err
                )
            })?;
        }
        return Ok(());
    }

    let Some(thread_id) = task
        .thread_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(
            "Task is already running, but its thread state is unavailable for cancellation."
                .to_string(),
        );
    };

    let next_epoch = expected_epoch.saturating_add(1).max(1);
    let state = ThreadState {
        thread_id: thread_id.to_string(),
        epoch: next_epoch,
        last_email_seq: next_epoch,
        last_message_id: None,
        updated_at: now,
    };
    write_thread_state(&state_path, &state).map_err(|err| {
        format!(
            "Failed to create thread state for cancellation at {}: {}",
            state_path.display(),
            err
        )
    })
}

fn build_resubmitted_task(
    task: &ScheduledTask,
    now: DateTime<Utc>,
) -> Result<ScheduledTask, String> {
    let TaskKind::RunTask(run_task) = &task.kind else {
        return Err("Only workflow tasks can be resubmitted safely.".to_string());
    };
    if !matches!(&task.schedule, Schedule::OneShot { .. }) {
        return Err("Recurring tasks should be managed from the routines dashboard.".to_string());
    }

    if let Some(expected_epoch) = run_task.thread_epoch {
        let state_path = run_task
            .thread_state_path
            .clone()
            .unwrap_or_else(|| default_thread_state_path(&run_task.workspace_dir));
        if let Some(state) = load_thread_state(&state_path) {
            if state.epoch > expected_epoch {
                return Err(
                    "This task belongs to an older thread state. Send a fresh message instead of resubmitting it."
                        .to_string(),
                );
            }
        }
    }

    let mut resubmitted = task.clone();
    resubmitted.id = Uuid::new_v4();
    resubmitted.enabled = true;
    resubmitted.created_at = now;
    resubmitted.last_run = None;
    resubmitted.schedule = Schedule::OneShot {
        run_at: now + ChronoDuration::seconds(1),
    };
    Ok(resubmitted)
}

/// GET /api/account/routines
/// Returns user-visible scheduled run_task routines for the authenticated user's unified account.
pub async fn get_account_routines(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    match try_load_unified_account_routines(&state, account.id).await {
        Ok(routines) => (StatusCode::OK, Json(routines)).into_response(),
        Err(response) => response,
    }
}

/// POST /api/account/routines/:task_id/pause
pub async fn pause_account_routine(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    mutate_account_routine_endpoint(state, headers, task_id, RoutineMutationAction::Pause).await
}

/// POST /api/account/routines/:task_id/resume
pub async fn resume_account_routine(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    mutate_account_routine_endpoint(state, headers, task_id, RoutineMutationAction::Resume).await
}

/// DELETE /api/account/routines/:task_id
pub async fn delete_account_routine(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    mutate_account_routine_endpoint(state, headers, task_id, RoutineMutationAction::Delete).await
}

async fn mutate_account_routine_endpoint(
    state: AuthState,
    headers: HeaderMap,
    task_id: String,
    action: RoutineMutationAction,
) -> Response {
    let account = match load_authenticated_account_from_headers(&state, &headers).await {
        Ok(account) => account,
        Err(response) => return response,
    };

    match mutate_unified_account_routine(&state, account.id, &task_id, action).await {
        Ok(true) => (
            StatusCode::OK,
            Json(RoutineMutationResponse { ok: true, task_id }),
        )
            .into_response(),
        Ok(false) => json_error_response(StatusCode::NOT_FOUND, "Routine not found"),
        Err(response) => response,
    }
}

#[derive(Debug, Deserialize)]
struct InstallOnboardingResendRequest {
    platform: InstallPlatform,
    workspace_id: String,
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Serialize)]
struct InstallOnboardingResendResponse {
    platform: InstallPlatform,
    workspace_id: String,
    #[serde(flatten)]
    result: InstallOnboardingRunResult,
}

/// POST /api/channel-install-onboarding/resend
/// Re-runs install onboarding for a previously recorded Slack workspace or Discord guild.
async fn resend_install_onboarding(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(request): Json<InstallOnboardingResendRequest>,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return json_error_response(StatusCode::UNAUTHORIZED, "Missing Authorization header");
        }
    };

    let auth_user = match validate_supabase_token(&state.supabase_url, &token).await {
        Ok(user) => user,
        Err((status, message)) => return json_error_response(status, &message),
    };

    let workspace_id = request.workspace_id.trim().to_string();
    if workspace_id.is_empty() {
        return json_error_response(StatusCode::BAD_REQUEST, "workspace_id is required");
    }

    let store = state.account_store.clone();
    let account_result =
        task::spawn_blocking(move || store.get_account_by_auth_user(auth_user.id)).await;

    let account = match account_result {
        Ok(Ok(Some(account))) => account,
        Ok(Ok(None)) => {
            return json_error_response(StatusCode::NOT_FOUND, "Account not found");
        }
        Ok(Err(err)) => {
            error!(
                "Failed to get account for manual onboarding resend: {}",
                err
            );
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to get account");
        }
        Err(err) => {
            error!(
                "spawn_blocking panicked during manual onboarding resend account lookup: {}",
                err
            );
            return json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error");
        }
    };

    let platform = request.platform;
    let force = request.force;
    let onboarding_state = state.clone();
    let workspace_id_for_task = workspace_id.clone();
    let resend_result = task::spawn_blocking(move || {
        execute_manual_install_onboarding_resend(
            &onboarding_state,
            account.id,
            account.auth_user_id,
            platform,
            &workspace_id_for_task,
            force,
        )
    })
    .await;

    match resend_result {
        Ok(Ok(result)) => (
            StatusCode::OK,
            Json(InstallOnboardingResendResponse {
                platform,
                workspace_id,
                result,
            }),
        )
            .into_response(),
        Ok(Err((status, message))) => json_error_response(status, &message),
        Err(err) => {
            error!(
                "spawn_blocking panicked during manual onboarding resend execution: {}",
                err
            );
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        }
    }
}

// ============================================================================
// Router
// ============================================================================

pub fn auth_router(state: AuthState) -> Router {
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/account", get(get_account).delete(delete_account))
        .route(
            "/auth/account/organization",
            put(set_account_organization).delete(clear_account_organization),
        )
        .route("/auth/organization", post(create_organization))
        .route("/auth/organizations", get(list_organizations))
        .route(
            "/auth/organization/:name/member-count",
            get(get_organization_member_count),
        )
        .route(
            "/auth/organization/:name/database",
            put(update_organization_database),
        )
        .route(
            "/auth/organization/:name/leader",
            put(update_organization_leader),
        )
        .route(
            "/auth/organization/:name/discord",
            put(update_organization_discord),
        )
        .route(
            "/auth/organization/:name/slack",
            put(update_organization_slack),
        )
        .route("/api/tpm/setup-cron", post(setup_tpm_cron))
        .route("/api/tpm/trigger-sync", post(trigger_tpm_sync_endpoint))
        .route("/auth/link", post(link_identifier))
        .route("/auth/verify", post(verify_identifier))
        .route("/auth/verify-email", get(verify_email))
        .route("/auth/unlink", delete(unlink_identifier))
        .route("/auth/memo", get(get_memo).post(update_memo))
        .route("/auth/discord", get(discord_oauth_start))
        .route("/auth/discord/callback", get(discord_oauth_callback))
        .route("/auth/discord/bot-callback", get(discord_bot_callback))
        .route("/auth/slack", get(slack_oauth_start))
        .route("/auth/slack/callback", get(slack_oauth_callback))
        .route("/auth/slack/bot-callback", get(slack_bot_callback))
        .route("/auth/github", get(github_oauth_start))
        .route("/auth/github/callback", get(github_oauth_callback))
        .route("/auth/notion", get(notion_oauth_start))
        .route("/auth/notion/callback", get(notion_oauth_callback))
        .route("/auth/lark", get(lark_oauth_start))
        .route("/auth/lark/callback", get(lark_oauth_callback))
        .route("/auth/wechat", get(wecom_oauth_start))
        .route("/auth/wechat/callback", get(wecom_oauth_callback))
        .route(
            "/api/channel-install-onboarding/resend",
            post(resend_install_onboarding),
        )
        .route(
            "/api/startup-workspace/intake-chat",
            post(startup_workspace_intake_chat),
        )
        .route(
            "/api/launch-execution/analyze",
            post(analyze_launch_execution),
        )
        .route(
            "/api/workspace/provider-state",
            get(get_workspace_provider_state),
        )
        .route(
            "/api/workspace/recommendation",
            post(get_workspace_recommendation),
        )
        .route(
            "/api/workspace/recommendation-feedback",
            post(record_workspace_recommendation_feedback),
        )
        .route(
            "/api/workspace/recommendation-preferences",
            get(get_workspace_recommendation_preferences)
                .post(update_workspace_recommendation_preferences),
        )
        .route("/api/tasks", get(get_tasks))
        .route("/api/account/tasks", get(get_account_tasks))
        .route("/api/account/tasks/:task_id", get(get_account_task_detail))
        .route(
            "/api/account/tasks/:task_id/cancel",
            post(cancel_account_task),
        )
        .route(
            "/api/account/tasks/:task_id/resubmit",
            post(resubmit_account_task),
        )
        .route("/api/account/routines", get(get_account_routines))
        .route(
            "/api/account/routines/:task_id/pause",
            post(pause_account_routine),
        )
        .route(
            "/api/account/routines/:task_id/resume",
            post(resume_account_routine),
        )
        .route(
            "/api/account/routines/:task_id",
            delete(delete_account_routine),
        )
        .with_state(state)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, TimeZone};
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::channel::Channel;
    use crate::thread_state::{write_thread_state, ThreadState};
    use crate::{RunTaskTask, Schedule, ScheduledTask, TaskKind};

    // Unit tests for GitHub OAuth structs and encoding logic
    // These don't require a database connection

    fn sample_run_task_task() -> RunTaskTask {
        RunTaskTask {
            workspace_dir: PathBuf::from("/tmp/routine-workspace"),
            input_email_dir: PathBuf::from("incoming_email"),
            input_attachments_dir: PathBuf::from("incoming_attachments"),
            memory_dir: PathBuf::from("memory"),
            reference_dir: PathBuf::from("references"),
            model_name: "gpt-test".to_string(),
            runner: "codex".to_string(),
            codex_disabled: false,
            reply_to: vec!["C123".to_string()],
            reply_from: None,
            archive_root: None,
            thread_id: Some("slack:C123:1234.5678".to_string()),
            thread_epoch: Some(1),
            thread_state_path: None,
            channel: Channel::Slack,
            slack_team_id: Some("T123".to_string()),
            employee_id: None,
            requester_identifier_type: None,
            requester_identifier: None,
            account_id: None,
            channel_metadata: Default::default(),
        }
    }

    fn sample_account_identifier(
        identifier_type: &str,
        identifier: &str,
        verified: bool,
    ) -> crate::account_store::AccountIdentifier {
        crate::account_store::AccountIdentifier {
            id: Uuid::new_v4(),
            account_id: Uuid::new_v4(),
            identifier_type: identifier_type.to_string(),
            identifier: identifier.to_string(),
            verified,
            created_at: Utc::now(),
        }
    }

    fn sample_routine_summary(
        id: &str,
        enabled: bool,
        execution_status: Option<&str>,
        last_run: Option<chrono::DateTime<Utc>>,
        run_at: Option<chrono::DateTime<Utc>>,
        created_at: chrono::DateTime<Utc>,
    ) -> RoutineSummary {
        RoutineSummary {
            id: id.to_string(),
            name: format!("Routine {}", id),
            kind: "run_task".to_string(),
            channel: "slack".to_string(),
            enabled,
            schedule_type: if run_at.is_some() {
                "one_shot".to_string()
            } else {
                "cron".to_string()
            },
            next_run: None,
            run_at: run_at.map(|value| value.to_rfc3339()),
            last_run: last_run.map(|value| value.to_rfc3339()),
            execution_status: execution_status.map(|value| value.to_string()),
            error_message: execution_status
                .filter(|value| *value == "failed")
                .map(|_| "temporary failure".to_string()),
            created_at: created_at.to_rfc3339(),
            is_recurring: run_at.is_none(),
        }
    }

    fn sample_task_status_summary(
        id: &str,
        status: &str,
        created_at: chrono::DateTime<Utc>,
    ) -> TaskStatusSummary {
        TaskStatusSummary {
            id: id.to_string(),
            kind: "run_task".to_string(),
            channel: "slack".to_string(),
            request_summary: Some(format!("Task {id}")),
            enabled: status != "failed",
            created_at: created_at.to_rfc3339(),
            last_run: None,
            schedule_type: "one_shot".to_string(),
            next_run: None,
            run_at: Some(created_at.to_rfc3339()),
            execution_status: Some(status.to_string()),
            error_message: (status == "failed").then(|| "boom".to_string()),
            execution_started_at: Some(created_at.to_rfc3339()),
            auto_disabled_reason: None,
            auto_disabled_at: None,
            status: status.to_string(),
            status_reason: None,
            status_changed_at: Some(created_at.to_rfc3339()),
            retry_at: None,
            will_retry: false,
            retry_count: 0,
            is_running_long: false,
            can_cancel: matches!(status, "queued" | "retry_scheduled"),
            can_resubmit: status == "failed",
        }
    }

    #[test]
    fn github_callback_query_deserializes_correctly() {
        let query = "code=abc123&state=encoded_token";
        let parsed: GitHubCallbackQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.code, "abc123");
        assert_eq!(parsed.state, "encoded_token");
    }

    #[test]
    fn github_callback_query_handles_special_chars() {
        let query = "code=abc%2B123%3D&state=token%2Fwith%2Fslashes";
        let parsed: GitHubCallbackQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.code, "abc+123=");
        assert_eq!(parsed.state, "token/with/slashes");
    }

    #[test]
    fn github_token_response_deserializes_correctly() {
        let json = r#"{"access_token":"gho_xxxxx","token_type":"bearer"}"#;
        let parsed: GitHubTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.access_token, "gho_xxxxx");
        assert_eq!(parsed.token_type, "bearer");
    }

    #[test]
    fn github_token_response_handles_extra_fields() {
        // GitHub may return additional fields we don't care about
        let json =
            r#"{"access_token":"gho_test","token_type":"bearer","scope":"","extra_field":123}"#;
        let parsed: GitHubTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.access_token, "gho_test");
        assert_eq!(parsed.token_type, "bearer");
    }

    #[test]
    fn github_user_response_deserializes_correctly() {
        let json = r#"{"login":"octocat","id":12345}"#;
        let parsed: GitHubUser = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.login, "octocat");
        assert_eq!(parsed.id, 12345);
    }

    #[test]
    fn github_user_response_handles_full_api_response() {
        // GitHub API returns many more fields - ensure we parse correctly
        let json = r#"{
            "login": "testuser",
            "id": 98765,
            "node_id": "MDQ6VXNlcjk4NzY1",
            "avatar_url": "https://avatars.githubusercontent.com/u/98765",
            "type": "User",
            "name": "Test User",
            "company": "TestCorp",
            "blog": "https://test.com",
            "location": "San Francisco",
            "email": null,
            "bio": "Testing",
            "public_repos": 10
        }"#;
        let parsed: GitHubUser = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.login, "testuser");
        assert_eq!(parsed.id, 98765);
    }

    #[test]
    fn base64_state_encoding_roundtrip() {
        let original_token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test";

        // Encode (as done in github_oauth_start)
        let encoded =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(original_token.as_bytes());

        // Decode (as done in github_oauth_callback)
        let decoded_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&encoded)
            .unwrap();
        let decoded = String::from_utf8(decoded_bytes).unwrap();

        assert_eq!(original_token, decoded);
    }

    #[test]
    fn base64_state_encoding_is_url_safe() {
        // JWT tokens may contain characters that need URL encoding
        let token = "eyJhbG+ciOi/JIUZ+I1NiIsInR5cCI6IkpXVCJ9";

        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token.as_bytes());

        // URL_SAFE encoding should not contain +, /, or =
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));

        // Should still roundtrip correctly
        let decoded_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&encoded)
            .unwrap();
        let decoded = String::from_utf8(decoded_bytes).unwrap();
        assert_eq!(token, decoded);
    }

    #[test]
    fn github_oauth_url_format() {
        let client_id = "test_client_id";
        let redirect_uri = "https://api.dowhiz.com/auth/github/callback";
        let state = "encoded_state";

        let url = format!(
            "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&state={}",
            client_id,
            urlencoding::encode(redirect_uri),
            state
        );

        assert!(url.starts_with("https://github.com/login/oauth/authorize"));
        assert!(url.contains("client_id=test_client_id"));
        assert!(
            url.contains("redirect_uri=https%3A%2F%2Fapi.dowhiz.com%2Fauth%2Fgithub%2Fcallback")
        );
        assert!(url.contains("state=encoded_state"));
    }

    #[test]
    fn invalid_base64_state_fails_decode() {
        let invalid_state = "!!!not_valid_base64!!!";
        let result = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(invalid_state);
        assert!(result.is_err());
    }

    #[test]
    fn extract_bearer_token_works() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer my_test_token".parse().unwrap());

        let token = extract_bearer_token(&headers);
        assert_eq!(token, Some("my_test_token".to_string()));
    }

    #[test]
    fn extract_bearer_token_returns_none_without_header() {
        let headers = HeaderMap::new();
        let token = extract_bearer_token(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn extract_bearer_token_returns_none_for_non_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Basic abc123".parse().unwrap());

        let token = extract_bearer_token(&headers);
        assert_eq!(token, None);
    }

    // ==================== Lark OAuth Tests ====================

    #[test]
    fn lark_callback_query_deserializes_correctly() {
        let query = "code=abc123&state=encoded_token";
        let parsed: LarkCallbackQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.code, "abc123");
        assert_eq!(parsed.state, "encoded_token");
    }

    #[test]
    fn lark_callback_query_handles_special_chars() {
        let query = "code=abc%2B123%3D&state=token%2Fwith%2Fslashes";
        let parsed: LarkCallbackQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.code, "abc+123=");
        assert_eq!(parsed.state, "token/with/slashes");
    }

    #[test]
    fn lark_app_token_response_deserializes_correctly() {
        let json = r#"{"code":0,"msg":"success","app_access_token":"a-xxxxx"}"#;
        let parsed: LarkAppTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.code, 0);
        assert_eq!(parsed.app_access_token, Some("a-xxxxx".to_string()));
    }

    #[test]
    fn lark_app_token_response_handles_error() {
        let json = r#"{"code":10003,"msg":"invalid app_id"}"#;
        let parsed: LarkAppTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.code, 10003);
        assert_eq!(parsed.msg, Some("invalid app_id".to_string()));
        assert_eq!(parsed.app_access_token, None);
    }

    #[test]
    fn lark_user_token_response_deserializes_correctly() {
        let json = r#"{"code":0,"msg":"success","data":{"access_token":"u-xxxxx"}}"#;
        let parsed: LarkUserTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.code, 0);
        assert!(parsed.data.is_some());
        assert_eq!(parsed.data.unwrap().access_token, "u-xxxxx");
    }

    #[test]
    fn lark_user_info_response_deserializes_correctly() {
        let json =
            r#"{"code":0,"msg":"success","data":{"open_id":"ou_abc123","name":"Test User"}}"#;
        let parsed: LarkUserInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.code, 0);
        assert!(parsed.data.is_some());
        let user = parsed.data.unwrap();
        assert_eq!(user.open_id, "ou_abc123");
        assert_eq!(user.name, Some("Test User".to_string()));
    }

    #[test]
    fn lark_user_info_response_handles_minimal_data() {
        let json = r#"{"code":0,"data":{"open_id":"ou_xyz789"}}"#;
        let parsed: LarkUserInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.code, 0);
        let user = parsed.data.unwrap();
        assert_eq!(user.open_id, "ou_xyz789");
        assert_eq!(user.name, None);
    }

    #[test]
    fn lark_oauth_url_format() {
        let client_id = "cli_test123";
        let redirect_uri = "https://api.dowhiz.com/auth/lark/callback";
        let state = "encoded_state";

        let url = format!(
            "https://open.feishu.cn/open-apis/authen/v1/authorize?app_id={}&redirect_uri={}&state={}",
            client_id,
            urlencoding::encode(redirect_uri),
            state
        );

        assert!(url.contains("app_id=cli_test123"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapi.dowhiz.com%2Fauth%2Flark%2Fcallback"));
        assert!(url.contains("state=encoded_state"));
    }

    #[test]
    fn slack_install_onboarding_request_uses_install_success_wiring() {
        let account_id = Uuid::new_v4();
        let auth_user_id = Uuid::new_v4();
        let event_nonce = "nonce1234";
        let installation = SlackInstallation {
            team_id: "T123".to_string(),
            team_name: Some("Acme".to_string()),
            bot_token: "xoxb-test".to_string(),
            bot_user_id: "Ubot".to_string(),
            installed_at: Utc::now(),
        };

        let request = build_slack_install_onboarding_request(
            account_id,
            auth_user_id,
            &installation,
            Some("Uowner".to_string()),
            event_nonce,
        );

        assert_eq!(request.platform, InstallPlatform::Slack);
        assert_eq!(request.trigger, InstallOnboardingTrigger::InstallSuccess);
        assert_eq!(request.workspace_id, "T123");
        assert_eq!(request.workspace_name.as_deref(), Some("Acme"));
        assert_eq!(request.installer_identifier, None);
        assert_eq!(request.linked_owner_identifier.as_deref(), Some("Uowner"));
        assert_eq!(
            request.linked_owner_identifier_source.as_deref(),
            Some("linked_account_owner")
        );
        let expected_event_key = format!("slack_bot_installed:{}:T123:{}", account_id, event_nonce);
        assert_eq!(
            request.event_key.as_deref(),
            Some(expected_event_key.as_str())
        );
        assert_eq!(
            request.route_path.as_deref(),
            Some("/auth/slack/bot-callback")
        );
    }

    #[test]
    fn discord_install_onboarding_request_uses_guild_context_and_safe_dm_fallback() {
        let account_id = Uuid::new_v4();
        let auth_user_id = Uuid::new_v4();
        let event_nonce = "nonce1234";
        let request = build_discord_install_onboarding_request(
            account_id,
            auth_user_id,
            "987654321",
            Some("Launchpad".to_string()),
            None,
            event_nonce,
        );

        assert_eq!(request.platform, InstallPlatform::Discord);
        assert_eq!(request.trigger, InstallOnboardingTrigger::InstallSuccess);
        assert_eq!(request.workspace_id, "987654321");
        assert_eq!(request.workspace_name.as_deref(), Some("Launchpad"));
        assert_eq!(request.installer_identifier, None);
        assert_eq!(request.linked_owner_identifier, None);
        assert_eq!(request.linked_owner_identifier_source, None);
        let expected_event_key = format!(
            "discord_bot_installed:{}:987654321:{}",
            account_id, event_nonce
        );
        assert_eq!(
            request.event_key.as_deref(),
            Some(expected_event_key.as_str())
        );
        assert_eq!(
            request.route_path.as_deref(),
            Some("/auth/discord/bot-callback")
        );
    }

    #[test]
    fn manual_resend_request_reuses_persisted_state_fields() {
        let account_id = Uuid::new_v4();
        let auth_user_id = Uuid::new_v4();
        let state = ChannelInstallOnboardingState {
            account_id,
            platform: "slack".to_string(),
            workspace_id: "T999".to_string(),
            workspace_name: Some("Workspace".to_string()),
            installer_identifier: Some("Uinstaller".to_string()),
            installer_identifier_source: Some("installer".to_string()),
            public_channel_id: Some("C123".to_string()),
            public_channel_name: Some("general".to_string()),
            dm_recipient_identifier: Some("Uinstaller".to_string()),
            dm_recipient_source: Some("installer".to_string()),
            last_event_key: Some("slack_bot_installed".to_string()),
            last_public_status: Some("sent".to_string()),
            last_public_error: None,
            last_dm_status: Some("sent".to_string()),
            last_dm_error: None,
            last_skip_reason: None,
            last_attempted_at: None,
            last_succeeded_at: None,
            last_manual_resend_at: None,
        };

        let request = build_manual_resend_install_onboarding_request(
            account_id,
            auth_user_id,
            InstallPlatform::Slack,
            &state,
            Some("Uowner".to_string()),
            true,
        );

        assert_eq!(request.platform, InstallPlatform::Slack);
        assert_eq!(request.trigger, InstallOnboardingTrigger::ManualResend);
        assert!(request.force);
        assert_eq!(request.workspace_id, "T999");
        assert_eq!(request.workspace_name.as_deref(), Some("Workspace"));
        assert_eq!(request.installer_identifier.as_deref(), Some("Uinstaller"));
        assert_eq!(request.public_channel_hint.as_deref(), Some("C123"));
        assert_eq!(request.linked_owner_identifier.as_deref(), Some("Uowner"));
        assert_eq!(
            request.linked_owner_identifier_source.as_deref(),
            Some("linked_account_owner")
        );
        assert_eq!(
            request.route_path.as_deref(),
            Some("/api/channel-install-onboarding/resend")
        );
        assert!(request
            .event_key
            .as_deref()
            .unwrap_or_default()
            .starts_with("manual_resend:slack:T999:"));
    }

    #[test]
    fn reconnect_request_reuses_workspace_state_with_unique_event_key() {
        let account_id = Uuid::new_v4();
        let auth_user_id = Uuid::new_v4();
        let state = ChannelInstallOnboardingState {
            account_id,
            platform: "discord".to_string(),
            workspace_id: "G123".to_string(),
            workspace_name: Some("Guild".to_string()),
            installer_identifier: Some("Uinstaller".to_string()),
            installer_identifier_source: Some("installer".to_string()),
            public_channel_id: Some("C123".to_string()),
            public_channel_name: Some("general".to_string()),
            dm_recipient_identifier: Some("Uinstaller".to_string()),
            dm_recipient_source: Some("installer".to_string()),
            last_event_key: Some("discord_bot_installed:old".to_string()),
            last_public_status: Some("sent".to_string()),
            last_public_error: None,
            last_dm_status: Some("sent".to_string()),
            last_dm_error: None,
            last_skip_reason: None,
            last_attempted_at: None,
            last_succeeded_at: None,
            last_manual_resend_at: None,
        };

        let request = build_reconnect_install_onboarding_request(
            account_id,
            auth_user_id,
            InstallPlatform::Discord,
            &state,
            Some("Uowner".to_string()),
            "nonce5678",
        );

        assert_eq!(request.platform, InstallPlatform::Discord);
        assert_eq!(request.trigger, InstallOnboardingTrigger::ReconnectSuccess);
        assert!(!request.force);
        assert_eq!(request.workspace_id, "G123");
        assert_eq!(request.workspace_name.as_deref(), Some("Guild"));
        assert_eq!(request.installer_identifier.as_deref(), Some("Uinstaller"));
        assert_eq!(request.public_channel_hint.as_deref(), Some("C123"));
        assert_eq!(request.linked_owner_identifier.as_deref(), Some("Uowner"));
        assert_eq!(
            request.route_path.as_deref(),
            Some("/auth/discord/callback")
        );
        let expected_event_key = format!("discord_reconnected:{}:G123:nonce5678", account_id);
        assert_eq!(
            request.event_key.as_deref(),
            Some(expected_event_key.as_str())
        );
    }

    #[test]
    fn oauth_callback_event_nonce_is_stable_and_hides_raw_code() {
        let first = oauth_callback_event_nonce("oauth-code-123");
        let second = oauth_callback_event_nonce("oauth-code-123");
        let different = oauth_callback_event_nonce("oauth-code-456");

        assert_eq!(first, second);
        assert_ne!(first, different);
        assert_eq!(first.len(), 16);
        assert!(!first.contains("oauth-code-123"));
    }

    #[test]
    fn merged_account_routines_prefer_terminal_legacy_status() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let account_copy = sample_routine_summary("task-1", true, Some("running"), None, None, now);
        let legacy_copy = sample_routine_summary(
            "task-1",
            true,
            Some("success"),
            Some(now + ChronoDuration::minutes(5)),
            None,
            now,
        );

        let merged = merge_routine_summaries(vec![account_copy], vec![legacy_copy]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].execution_status.as_deref(), Some("success"));
        assert_eq!(
            merged[0].last_run.as_deref(),
            Some((now + ChronoDuration::minutes(5)).to_rfc3339().as_str())
        );
    }

    #[test]
    fn merged_account_routines_prefer_newer_success_over_older_failed_with_error() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let newer_success = sample_routine_summary(
            "task-1",
            false,
            Some("success"),
            Some(now + ChronoDuration::minutes(5)),
            None,
            now,
        );
        let older_failed = sample_routine_summary(
            "task-1",
            false,
            Some("failed"),
            Some(now + ChronoDuration::minutes(1)),
            None,
            now,
        );

        let merged = merge_routine_summaries(vec![newer_success], vec![older_failed]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].execution_status.as_deref(), Some("success"));
        assert_eq!(merged[0].error_message, None);
        assert_eq!(
            merged[0].last_run.as_deref(),
            Some((now + ChronoDuration::minutes(5)).to_rfc3339().as_str())
        );
    }

    #[test]
    fn merged_account_tasks_prefer_newer_success_over_older_failed_with_error() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let newer_success =
            sample_task_status_summary("task-1", "success", now + ChronoDuration::minutes(5));
        let older_failed =
            sample_task_status_summary("task-1", "failed", now + ChronoDuration::minutes(1));

        let merged = merge_task_summaries(vec![newer_success], vec![older_failed]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].status, "success");
        assert_eq!(merged[0].execution_status.as_deref(), Some("success"));
        assert_eq!(merged[0].error_message, None);
        assert_eq!(
            merged[0].status_changed_at.as_deref(),
            Some((now + ChronoDuration::minutes(5)).to_rfc3339().as_str())
        );
    }

    #[test]
    fn routine_lookup_identifiers_include_all_verified_account_links() {
        let identifiers = vec![
            sample_account_identifier("email", "logan@example.com", true),
            sample_account_identifier("slack", "U123", true),
            sample_account_identifier("google_docs", "person-456", true),
            sample_account_identifier("email", "pending@example.com", false),
        ];

        let lookup_identifiers = legacy_routine_lookup_identifiers(&identifiers);

        assert_eq!(
            lookup_identifiers,
            vec![
                ("email".to_string(), "logan@example.com".to_string()),
                ("slack".to_string(), "U123".to_string()),
                ("google_docs".to_string(), "person-456".to_string()),
            ]
        );
    }

    #[test]
    fn account_routines_partition_active_and_cap_history() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let mut routines = vec![sample_routine_summary(
            "active-1",
            true,
            Some("running"),
            None,
            Some(now + ChronoDuration::hours(1)),
            now,
        )];

        for index in 0..60 {
            routines.push(sample_routine_summary(
                &format!("history-{index}"),
                false,
                Some("success"),
                Some(now + ChronoDuration::minutes(index as i64)),
                Some(now + ChronoDuration::minutes(index as i64)),
                now - ChronoDuration::days(1),
            ));
        }

        let partitioned = partition_routines(routines);
        assert_eq!(partitioned.active.len(), 1);
        assert_eq!(partitioned.history.len(), 50);
        assert_eq!(partitioned.history[0].id, "history-59");
        assert_eq!(
            partitioned
                .history
                .last()
                .map(|routine| routine.id.as_str()),
            Some("history-10")
        );
    }

    #[test]
    fn routine_mutation_actions_disable_and_resume_safely() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let task = ScheduledTask {
            id: Uuid::new_v4(),
            kind: TaskKind::RunTask(sample_run_task_task()),
            schedule: Schedule::OneShot {
                run_at: now + ChronoDuration::hours(2),
            },
            enabled: true,
            created_at: now - ChronoDuration::hours(1),
            last_run: None,
        };

        let paused =
            mutate_routine_task(&task, RoutineMutationAction::Pause, now).expect("pause routine");
        assert!(!paused.enabled);

        let deleted =
            mutate_routine_task(&task, RoutineMutationAction::Delete, now).expect("delete routine");
        assert!(!deleted.enabled);

        let resumed = mutate_routine_task(&paused, RoutineMutationAction::Resume, now)
            .expect("resume routine");
        assert!(resumed.enabled);
        match resumed.schedule {
            Schedule::OneShot { run_at } => {
                assert_eq!(run_at, now + ChronoDuration::hours(2));
            }
            _ => panic!("expected one-shot schedule"),
        }
    }

    #[test]
    fn routine_resume_fails_for_stale_one_shot() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let task = ScheduledTask {
            id: Uuid::new_v4(),
            kind: TaskKind::RunTask(sample_run_task_task()),
            schedule: Schedule::OneShot {
                run_at: now - ChronoDuration::minutes(1),
            },
            enabled: false,
            created_at: now - ChronoDuration::hours(2),
            last_run: Some(now - ChronoDuration::minutes(1)),
        };

        let error = mutate_routine_task(&task, RoutineMutationAction::Resume, now)
            .expect_err("stale one-shot");
        assert!(error.contains("run_at is in the past"));
    }

    #[test]
    fn task_resubmit_clones_failed_workflow_safely() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let task = ScheduledTask {
            id: Uuid::new_v4(),
            kind: TaskKind::RunTask(sample_run_task_task()),
            schedule: Schedule::OneShot {
                run_at: now - ChronoDuration::minutes(5),
            },
            enabled: false,
            created_at: now - ChronoDuration::hours(1),
            last_run: Some(now - ChronoDuration::minutes(4)),
        };

        let resubmitted = build_resubmitted_task(&task, now).expect("resubmit task");
        assert_ne!(resubmitted.id, task.id);
        assert!(resubmitted.enabled);
        assert_eq!(resubmitted.created_at, now);
        assert_eq!(resubmitted.last_run, None);
        match resubmitted.schedule {
            Schedule::OneShot { run_at } => {
                assert_eq!(run_at, now + ChronoDuration::seconds(1));
            }
            _ => panic!("expected one-shot schedule"),
        }
    }

    #[test]
    fn task_resubmit_rejects_stale_thread_epoch() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let temp = TempDir::new().expect("tempdir");
        let thread_state_path = temp.path().join("thread_state.json");
        let thread_state = ThreadState {
            thread_id: "slack:C123:1234.5678".to_string(),
            epoch: 2,
            last_email_seq: 2,
            last_message_id: None,
            updated_at: now.to_rfc3339(),
        };
        write_thread_state(&thread_state_path, &thread_state).expect("write thread state");

        let mut run_task = sample_run_task_task();
        run_task.workspace_dir = temp.path().to_path_buf();
        run_task.thread_state_path = Some(thread_state_path);
        run_task.thread_epoch = Some(1);

        let task = ScheduledTask {
            id: Uuid::new_v4(),
            kind: TaskKind::RunTask(run_task),
            schedule: Schedule::OneShot {
                run_at: now - ChronoDuration::minutes(5),
            },
            enabled: false,
            created_at: now - ChronoDuration::hours(1),
            last_run: Some(now - ChronoDuration::minutes(4)),
        };

        let error = build_resubmitted_task(&task, now).expect_err("stale thread epoch");
        assert!(error.contains("older thread state"));
    }

    #[test]
    fn preferred_task_write_match_index_skips_account_mirror_when_live_copy_exists() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let task = ScheduledTask {
            id: Uuid::new_v4(),
            kind: TaskKind::RunTask(sample_run_task_task()),
            schedule: Schedule::OneShot { run_at: now },
            enabled: false,
            created_at: now - ChronoDuration::minutes(5),
            last_run: Some(now - ChronoDuration::minutes(1)),
        };
        let account_path = PathBuf::from("/tmp/users/account-123/state/tasks.db");
        let live_path = PathBuf::from("/tmp/users/slack-user-1/state/tasks.db");
        let matches = vec![
            TaskStorageMatch {
                path: account_path.clone(),
                task: task.clone(),
                summary: sample_task_status_summary(&task.id.to_string(), "failed", now),
                executions: Vec::new(),
            },
            TaskStorageMatch {
                path: live_path,
                task,
                summary: sample_task_status_summary("live-copy", "failed", now),
                executions: Vec::new(),
            },
        ];

        assert_eq!(
            preferred_task_write_match_index(&matches, account_path.as_path()),
            Some(1)
        );
    }

    #[test]
    fn preferred_task_match_index_prefers_newer_success_over_older_failed_with_error() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let task = ScheduledTask {
            id: Uuid::new_v4(),
            kind: TaskKind::RunTask(sample_run_task_task()),
            schedule: Schedule::OneShot { run_at: now },
            enabled: false,
            created_at: now - ChronoDuration::minutes(5),
            last_run: Some(now),
        };
        let task_id = task.id.to_string();
        let success_path = PathBuf::from("/tmp/users/user-success/state/tasks.db");
        let failed_path = PathBuf::from("/tmp/users/user-failed/state/tasks.db");
        let matches = vec![
            TaskStorageMatch {
                path: success_path,
                task: task.clone(),
                summary: sample_task_status_summary(
                    &task_id,
                    "success",
                    now + ChronoDuration::minutes(5),
                ),
                executions: Vec::new(),
            },
            TaskStorageMatch {
                path: failed_path,
                task,
                summary: sample_task_status_summary(
                    &task_id,
                    "failed",
                    now + ChronoDuration::minutes(1),
                ),
                executions: Vec::new(),
            },
        ];

        assert_eq!(preferred_task_match_index(&matches), Some(0));
    }

    #[test]
    fn merged_task_execution_history_dedupes_identical_mirror_rows() {
        let now = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        let task = ScheduledTask {
            id: Uuid::new_v4(),
            kind: TaskKind::RunTask(sample_run_task_task()),
            schedule: Schedule::OneShot { run_at: now },
            enabled: false,
            created_at: now - ChronoDuration::minutes(5),
            last_run: Some(now),
        };
        let task_id = task.id.to_string();
        let execution = TaskExecutionSummary {
            execution_id: 1778001442579447,
            status: "failed".to_string(),
            started_at: now.to_rfc3339(),
            finished_at: Some((now + ChronoDuration::minutes(10)).to_rfc3339()),
            error_message: Some("primary runner failed".to_string()),
            duration_seconds: Some(600),
        };
        let matches = vec![
            TaskStorageMatch {
                path: PathBuf::from("/tmp/users/live-user/state/tasks.db"),
                task: task.clone(),
                summary: sample_task_status_summary(&task_id, "failed", now),
                executions: vec![execution.clone()],
            },
            TaskStorageMatch {
                path: PathBuf::from("/tmp/users/account-mirror/state/tasks.db"),
                task,
                summary: sample_task_status_summary(&task_id, "failed", now),
                executions: vec![execution.clone()],
            },
        ];

        let merged = merge_task_execution_summaries(&matches);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].execution_id, execution.execution_id);
        assert_eq!(merged[0].started_at, execution.started_at);
        assert_eq!(merged[0].finished_at, execution.finished_at);
    }

    // ==================== WeCom OAuth Tests ====================

    #[test]
    fn wecom_callback_query_deserializes_correctly() {
        let query = "code=abc123&state=encoded_token";
        let parsed: WeComCallbackQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.code, "abc123");
        assert_eq!(parsed.state, "encoded_token");
    }

    #[test]
    fn wecom_callback_query_handles_special_chars() {
        let query = "code=abc%2B123%3D&state=token%2Fwith%2Fslashes";
        let parsed: WeComCallbackQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.code, "abc+123=");
        assert_eq!(parsed.state, "token/with/slashes");
    }

    #[test]
    fn wecom_access_token_response_deserializes_correctly() {
        let json =
            r#"{"errcode":0,"errmsg":"ok","access_token":"accesstoken123","expires_in":7200}"#;
        let parsed: WeComAccessTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.errcode, Some(0));
        assert_eq!(parsed.access_token, Some("accesstoken123".to_string()));
        assert_eq!(parsed.expires_in, Some(7200));
    }

    #[test]
    fn wecom_access_token_response_handles_error() {
        let json = r#"{"errcode":40013,"errmsg":"invalid corpid"}"#;
        let parsed: WeComAccessTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.errcode, Some(40013));
        assert_eq!(parsed.errmsg, Some("invalid corpid".to_string()));
        assert_eq!(parsed.access_token, None);
    }

    #[test]
    fn wecom_user_info_response_deserializes_internal_user() {
        let json = r#"{"errcode":0,"errmsg":"ok","UserId":"zhangsan","DeviceId":"device123"}"#;
        let parsed: WeComUserInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.errcode, Some(0));
        assert_eq!(parsed.user_id, Some("zhangsan".to_string()));
        assert_eq!(parsed.open_id, None);
    }

    #[test]
    fn wecom_user_info_response_deserializes_external_contact() {
        let json = r#"{"errcode":0,"errmsg":"ok","OpenId":"oU1234567890"}"#;
        let parsed: WeComUserInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.errcode, Some(0));
        assert_eq!(parsed.user_id, None);
        assert_eq!(parsed.open_id, Some("oU1234567890".to_string()));
    }

    #[test]
    fn wecom_user_info_response_handles_error() {
        let json = r#"{"errcode":40029,"errmsg":"invalid code"}"#;
        let parsed: WeComUserInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.errcode, Some(40029));
        assert_eq!(parsed.user_id, None);
        assert_eq!(parsed.open_id, None);
    }

    #[test]
    fn wecom_oauth_url_format() {
        let corp_id = "ww1234567890abcdef";
        let agent_id = "1000002";
        let redirect_uri = "https://api.dowhiz.com/auth/wechat/callback";
        let state = "encoded_state";

        let url = format!(
            "https://open.weixin.qq.com/connect/oauth2/authorize?appid={}&redirect_uri={}&response_type=code&scope=snsapi_privateinfo&agentid={}&state={}#wechat_redirect",
            corp_id,
            urlencoding::encode(redirect_uri),
            agent_id,
            state
        );

        assert!(url.contains("appid=ww1234567890abcdef"));
        assert!(
            url.contains("redirect_uri=https%3A%2F%2Fapi.dowhiz.com%2Fauth%2Fwechat%2Fcallback")
        );
        assert!(url.contains("scope=snsapi_privateinfo"));
        assert!(url.contains("agentid=1000002"));
        assert!(url.contains("state=encoded_state"));
        assert!(url.ends_with("#wechat_redirect"));
    }

    #[test]
    fn wecom_identifier_format_combines_corp_and_user() {
        let corp_id = "ww1234567890abcdef";
        let user_id = "zhangsan";
        let identifier = format!("{}_{}", corp_id, user_id);
        assert_eq!(identifier, "ww1234567890abcdef_zhangsan");
    }

    #[test]
    fn wecom_identifier_format_handles_external_contact() {
        let corp_id = "ww1234567890abcdef";
        let open_id = "oU1234567890";
        let identifier = format!("{}_{}", corp_id, open_id);
        assert_eq!(identifier, "ww1234567890abcdef_oU1234567890");
    }

    // =========================================================================
    // Organization endpoint tests
    // =========================================================================

    #[test]
    fn create_organization_request_deserializes_correctly() {
        let json = r#"{"name":"deeptutor"}"#;
        let parsed: CreateOrganizationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.name, "deeptutor");
    }

    #[test]
    fn create_organization_request_handles_special_chars() {
        let json = r#"{"name":"Acme Corp (Test)"}"#;
        let parsed: CreateOrganizationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.name, "Acme Corp (Test)");
    }

    #[test]
    fn list_organizations_query_deserializes_with_search() {
        let query = "search=deep";
        let parsed: ListOrganizationsQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.search, Some("deep".to_string()));
    }

    #[test]
    fn list_organizations_query_deserializes_without_search() {
        let query = "";
        let parsed: ListOrganizationsQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.search, None);
    }

    #[test]
    fn list_organizations_query_handles_empty_search() {
        let query = "search=";
        let parsed: ListOrganizationsQuery = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parsed.search, Some("".to_string()));
    }

    #[test]
    fn set_organization_request_deserializes_correctly() {
        let json = r#"{"organization_name":"deeptutor"}"#;
        let parsed: SetOrganizationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.organization_name, "deeptutor");
    }
}
