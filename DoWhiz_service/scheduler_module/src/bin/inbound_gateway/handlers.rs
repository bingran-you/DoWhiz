use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration as StdDuration, Instant};

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header::CONTENT_TYPE, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use scheduler_module::adapters::bluebubbles::BlueBubblesInboundAdapter;
use scheduler_module::adapters::lark::LarkInboundAdapter;
use scheduler_module::adapters::postmark::PostmarkInboundPayload;
use scheduler_module::adapters::slack::{
    is_url_verification, SlackChallengeResponse, SlackEventWrapper, SlackInboundAdapter,
};
use scheduler_module::adapters::telegram::TelegramInboundAdapter;
use scheduler_module::adapters::wechat::WeChatInboundAdapter;
use scheduler_module::adapters::wechat_mp::WeChatMpInboundAdapter;
use scheduler_module::adapters::whatsapp::WhatsAppInboundAdapter;
use scheduler_module::channel::{Channel, ChannelMetadata, InboundAdapter, InboundMessage};
use scheduler_module::ingestion::{IngestionEnvelope, IngestionPayload};
use scheduler_module::ingestion_queue::IngestionQueue;
use scheduler_module::raw_payload_store::{self, RawPayloadStoreError};
use scheduler_module::service::derive_inbound_email_text;
use scheduler_module::slack_store::resolve_slack_bot_user_id_for_runtime;
use scheduler_module::user_store::extract_emails;

use super::routes::{build_dedupe_key, normalize_email, normalize_phone_number, resolve_route};
use super::state::{find_service_address, GatewayState, RouteDecision, RouteKey, RouteTarget};
use super::verify::{
    verify_bluebubbles, verify_lark, verify_lark_challenge, verify_postmark, verify_slack,
    verify_twilio, verify_wechat, verify_wechat_mp, verify_wechat_mp_message,
    verify_whatsapp_subscription,
};

const SLACK_ENGAGED_THREAD_TTL: StdDuration = StdDuration::from_secs(12 * 60 * 60);
const WECHAT_MP_PASSIVE_ACK_BODY: &str = "success";
const WECHAT_MP_PASSIVE_REPLY_TEXT_ENV: &str = "WECHAT_MP_PASSIVE_REPLY_TEXT";

/// Request payload for creating a workspace brief document
#[derive(Debug, Deserialize)]
pub(super) struct CreateWorkspaceBriefRequest {
    pub founder_name: String,
    pub founder_email: String,
    pub venture_name: Option<String>,
    pub thesis: Option<String>,
    pub stage: Option<String>,
    pub goals: Vec<String>,
    pub current_assets: Option<Vec<String>>,
    pub plan_horizon_days: Option<i32>,
    pub account_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Create90DayPlanRequest {
    pub founder_name: String,
    pub founder_email: String,
    pub venture_name: Option<String>,
    pub thesis: Option<String>,
    pub stage: Option<String>,
    pub goals: Vec<String>,
    pub current_assets: Option<Vec<String>>,
    pub plan_horizon_days: Option<i32>,
    pub account_id: Option<Uuid>,
}

pub(super) async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub(super) async fn ingest_postmark(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Err(reason) = verify_postmark(&headers) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status": reason})));
    }

    let payload: PostmarkInboundPayload = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(e) => {
            let body_preview = String::from_utf8_lossy(&body[..body.len().min(500)]);
            warn!(
                "gateway failed to parse postmark payload: {} - body preview: {}",
                e, body_preview
            );
            return (StatusCode::BAD_REQUEST, Json(json!({"status": "bad_json"})));
        }
    };

    if payload_contains_no_reply_marker(&payload) {
        info!(
            "gateway ignoring no-reply postmark inbound from={}",
            payload.from.as_deref().unwrap_or("")
        );
        return (StatusCode::OK, Json(json!({"status": "ignored_no_reply"})));
    }

    let address = find_service_address(&payload, &state.employee_directory.service_addresses);
    let Some(address) = address else {
        let body_preview = String::from_utf8_lossy(&body[..body.len().min(1000)]);
        info!(
            "gateway no service address found in postmark payload: to={:?}, cc={:?}, bcc={:?}, original_recipient={:?}, from={:?}, subject={:?}, body_preview={}",
            payload.to,
            payload.cc,
            payload.bcc,
            payload.original_recipient,
            payload.from,
            payload.subject,
            body_preview
        );
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    let route_key = normalize_email(&address);
    let Some(route) = resolve_route(Channel::Email, &route_key, &state) else {
        info!("gateway no route for email address={}", route_key);
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    let adapter = scheduler_module::adapters::postmark::PostmarkInboundAdapter::new(
        state.employee_directory.service_addresses.clone(),
    );
    let message = match adapter.parse(&body) {
        Ok(message) => message,
        Err(err) => {
            warn!("gateway failed to parse postmark payload: {}", err);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"status": "parse_error"})),
            );
        }
    };

    let external_message_id = payload
        .header_message_id()
        .or(payload.message_id.as_deref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let envelope =
        match build_envelope(route, Channel::Email, external_message_id, &message, &body).await {
            Ok(envelope) => envelope,
            Err(err) => {
                error!("gateway failed to store raw payload: {}", err);
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"status": "payload_store_failed"})),
                );
            }
        };
    enqueue_envelope(state.queue.clone(), envelope).await
}

pub(super) async fn ingest_slack(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Some(verification) = is_url_verification(&body) {
        let response = SlackChallengeResponse {
            challenge: verification.challenge,
        };
        return (StatusCode::OK, Json(json!(response)));
    }

    if let Err(reason) = verify_slack(&headers, &body) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status": reason})));
    }

    let wrapper: SlackEventWrapper = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"status": "bad_json"}))),
    };

    // Extract api_app_id for routing (each Slack app has unique app_id)
    let api_app_id = wrapper.api_app_id.as_deref().unwrap_or("");
    if api_app_id.is_empty() {
        info!("gateway no api_app_id in slack payload");
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    }

    let event_id = wrapper.event_id.clone();

    let Some(route) = resolve_slack_route(api_app_id, &state) else {
        info!("gateway no route for slack api_app_id={}", api_app_id);
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    info!(
        "gateway slack routing: api_app_id={} -> employee_id={}",
        api_app_id, route.employee_id
    );

    let bot_user_id =
        resolve_slack_bot_user_id_for_employee(&route.employee_id, wrapper.team_id.as_deref());
    if !should_enqueue_slack_message(&wrapper, bot_user_id.as_deref()) {
        info!(
            "gateway ignoring slack event for employee={} api_app_id={} (not dm/app_mention/mention)",
            route.employee_id, api_app_id
        );
        return (StatusCode::OK, Json(json!({"status": "ignored"})));
    }

    let mut bot_user_ids = HashSet::new();
    if let Some(id) = bot_user_id {
        bot_user_ids.insert(id);
    }
    let adapter = SlackInboundAdapter::new(bot_user_ids);
    let message = match adapter.parse(&body) {
        Ok(message) => message,
        Err(err) => {
            warn!("gateway failed to parse slack payload: {}", err);
            return (StatusCode::OK, Json(json!({"status": "ignored"})));
        }
    };

    let envelope = match build_envelope(route, Channel::Slack, event_id, &message, &body).await {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("gateway failed to store raw payload: {}", err);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"status": "payload_store_failed"})),
            );
        }
    };
    enqueue_envelope(state.queue.clone(), envelope).await
}

fn resolve_slack_bot_user_id_for_employee(
    employee_id: &str,
    team_id: Option<&str>,
) -> Option<String> {
    resolve_slack_bot_user_id_for_runtime(team_id, Some(employee_id))
}

fn route_decision_from_target(target: RouteTarget, state: &GatewayState) -> RouteDecision {
    let tenant_id = target
        .tenant_id
        .clone()
        .or_else(|| state.config.defaults.tenant_id.clone())
        .unwrap_or_else(|| "default".to_string());
    RouteDecision {
        tenant_id,
        employee_id: target.employee_id,
    }
}

fn resolve_employee_id_by_slack_app_id(api_app_id: &str, state: &GatewayState) -> Option<String> {
    let app_id = api_app_id.trim();
    if app_id.is_empty() {
        return None;
    }

    for employee in &state.employee_directory.employees {
        let env_key = format!(
            "{}_SLACK_APP_ID",
            employee.id.to_uppercase().replace('-', "_")
        );
        let matched = std::env::var(&env_key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| value == app_id)
            .unwrap_or(false);
        if matched {
            return Some(employee.id.clone());
        }
    }

    let default_matched = std::env::var("SLACK_APP_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| value == app_id)
        .unwrap_or(false);
    if default_matched {
        return state
            .config
            .defaults
            .employee_id
            .clone()
            .or_else(|| state.employee_directory.default_employee_id.clone());
    }

    None
}

fn resolve_slack_route(api_app_id: &str, state: &GatewayState) -> Option<RouteDecision> {
    // 1) Exact route in gateway config (api_app_id specific) has highest precedence.
    let explicit_key = RouteKey {
        channel: Channel::Slack,
        key: api_app_id.to_string(),
    };
    if let Some(target) = state.config.routes.get(&explicit_key).cloned() {
        return Some(route_decision_from_target(target, state));
    }

    // 2) Env-based app-id mapping (e.g. BOILED_EGG_SLACK_APP_ID) to avoid wildcard misrouting.
    if let Some(employee_id) = resolve_employee_id_by_slack_app_id(api_app_id, state) {
        return Some(RouteDecision {
            tenant_id: state
                .config
                .defaults
                .tenant_id
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            employee_id,
        });
    }

    // 3) Fallback to existing wildcard/default route behavior.
    resolve_route(Channel::Slack, api_app_id, state)
}

fn should_enqueue_slack_message(wrapper: &SlackEventWrapper, bot_user_id: Option<&str>) -> bool {
    let Some(event) = wrapper.event.as_ref() else {
        return false;
    };
    if event.subtype.is_some() {
        return false;
    }
    // Filter out bot messages to prevent self-loops
    if event.bot_id.is_some() {
        return false;
    }
    // Also filter out messages from our own bot user ID
    if let (Some(user), Some(bot_id)) = (event.user.as_deref(), bot_user_id) {
        if user == bot_id {
            return false;
        }
    }

    match event.event_type.as_str() {
        "app_mention" => {
            remember_slack_engaged_thread(wrapper);
            true
        }
        "message" => {
            if matches!(event.channel_type.as_deref(), Some("im") | Some("mpim")) {
                return true;
            }
            if event.thread_ts.is_some() && slack_thread_is_engaged(wrapper) {
                return true;
            }
            let Some(bot_user_id) = bot_user_id else {
                return false;
            };
            let mention = format!("<@{}>", bot_user_id.trim());
            let should_enqueue = event
                .text
                .as_deref()
                .map(|text| text.contains(&mention))
                .unwrap_or(false);
            if should_enqueue {
                remember_slack_engaged_thread(wrapper);
            }
            should_enqueue
        }
        _ => false,
    }
}

fn slack_engaged_threads() -> &'static Mutex<HashMap<String, Instant>> {
    static THREADS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    THREADS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn slack_thread_scope_key(wrapper: &SlackEventWrapper) -> Option<String> {
    let event = wrapper.event.as_ref()?;
    let team_id = wrapper.team_id.as_deref().unwrap_or("unknown").trim();
    let channel_id = event.channel.as_deref()?.trim();
    let root_ts = event
        .thread_ts
        .as_deref()
        .unwrap_or(event.ts.as_str())
        .trim();
    if channel_id.is_empty() || root_ts.is_empty() {
        return None;
    }
    Some(format!("slack:{team_id}:{channel_id}:{root_ts}"))
}

fn prune_stale_slack_engaged_threads(store: &mut HashMap<String, Instant>, now: Instant) {
    store.retain(|_, last_seen| now.duration_since(*last_seen) <= SLACK_ENGAGED_THREAD_TTL);
}

fn remember_slack_engaged_thread(wrapper: &SlackEventWrapper) {
    let Some(key) = slack_thread_scope_key(wrapper) else {
        return;
    };
    let now = Instant::now();
    let mut store = slack_engaged_threads()
        .lock()
        .expect("slack engaged thread lock poisoned");
    prune_stale_slack_engaged_threads(&mut store, now);
    store.insert(key, now);
}

fn slack_thread_is_engaged(wrapper: &SlackEventWrapper) -> bool {
    let Some(key) = slack_thread_scope_key(wrapper) else {
        return false;
    };
    let now = Instant::now();
    let mut store = slack_engaged_threads()
        .lock()
        .expect("slack engaged thread lock poisoned");
    prune_stale_slack_engaged_threads(&mut store, now);
    store.contains_key(&key)
}

pub(super) async fn ingest_bluebubbles(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Err(reason) = verify_bluebubbles(&headers) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status": reason})));
    }

    let adapter = BlueBubblesInboundAdapter::new();
    let message = match adapter.parse(&body) {
        Ok(message) => message,
        Err(err) => {
            debug!("gateway ignoring bluebubbles event: {}", err);
            return (StatusCode::OK, Json(json!({"status": "ignored"})));
        }
    };

    let chat_guid = message
        .metadata
        .bluebubbles_chat_guid
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let Some(route) = resolve_route(Channel::BlueBubbles, &chat_guid, &state) else {
        info!("gateway no route for bluebubbles chat_guid={}", chat_guid);
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    let external_message_id = message.message_id.clone();
    let envelope = match build_envelope(
        route,
        Channel::BlueBubbles,
        external_message_id,
        &message,
        &body,
    )
    .await
    {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("gateway failed to store raw payload: {}", err);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"status": "payload_store_failed"})),
            );
        }
    };
    enqueue_envelope(state.queue.clone(), envelope).await
}

pub(super) async fn ingest_sms(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Err(reason) = verify_twilio(&headers, &body) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status": reason})));
    }

    let params: HashMap<String, String> = match serde_urlencoded::from_bytes(&body) {
        Ok(values) => values,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"status": "bad_form"}))),
    };

    let from = params.get("From").cloned().unwrap_or_default();
    let to = params.get("To").cloned().unwrap_or_default();
    let body_text = params.get("Body").cloned().unwrap_or_default();
    let message_sid = params.get("MessageSid").cloned();

    if from.is_empty() || to.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status": "missing_fields"})),
        );
    }

    let route_key = normalize_phone_number(&to);
    let Some(route) = resolve_route(Channel::Sms, &route_key, &state) else {
        info!("gateway no route for sms to={}", route_key);
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    let message = InboundMessage {
        channel: Channel::Sms,
        sender: from.clone(),
        sender_name: None,
        recipient: to.clone(),
        subject: None,
        text_body: Some(body_text),
        html_body: None,
        thread_id: format!("sms:{}:{}", route_key, normalize_phone_number(&from)),
        message_id: message_sid.clone(),
        attachments: Vec::new(),
        reply_to: vec![from.clone()],
        raw_payload: body.to_vec(),
        metadata: ChannelMetadata {
            sms_from: Some(from.clone()),
            sms_to: Some(to.clone()),
            ..Default::default()
        },
    };

    let envelope = match build_envelope(route, Channel::Sms, message_sid, &message, &body).await {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("gateway failed to store raw payload: {}", err);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"status": "payload_store_failed"})),
            );
        }
    };
    enqueue_envelope(state.queue.clone(), envelope).await
}

pub(super) async fn ingest_telegram(
    State(state): State<Arc<GatewayState>>,
    body: Bytes,
) -> impl IntoResponse {
    let adapter = TelegramInboundAdapter::new();
    let message = match adapter.parse(&body) {
        Ok(message) => message,
        Err(err) => {
            debug!("gateway ignoring telegram event: {}", err);
            return (StatusCode::OK, Json(json!({"status": "ignored"})));
        }
    };

    let chat_id = message
        .metadata
        .telegram_chat_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let Some(route) = resolve_route(Channel::Telegram, &chat_id, &state) else {
        info!("gateway no route for telegram chat_id={}", chat_id);
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    let external_message_id = message.message_id.clone();
    let envelope = match build_envelope(
        route,
        Channel::Telegram,
        external_message_id,
        &message,
        &body,
    )
    .await
    {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("gateway failed to store raw payload: {}", err);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"status": "payload_store_failed"})),
            );
        }
    };
    enqueue_envelope(state.queue.clone(), envelope).await
}

/// Query parameters for WhatsApp webhook verification
#[derive(Debug, Deserialize)]
pub(super) struct WhatsAppVerifyParams {
    #[serde(rename = "hub.mode")]
    pub hub_mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    pub hub_verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    pub hub_challenge: Option<String>,
}

/// Handle WhatsApp webhook verification (GET request)
pub(super) async fn verify_whatsapp_webhook(
    Query(params): Query<WhatsAppVerifyParams>,
) -> impl IntoResponse {
    match verify_whatsapp_subscription(
        params.hub_mode.as_deref(),
        params.hub_verify_token.as_deref(),
        params.hub_challenge.as_deref(),
    ) {
        Ok(challenge) => (StatusCode::OK, challenge),
        Err(reason) => {
            info!("whatsapp webhook verification failed: {}", reason);
            (StatusCode::FORBIDDEN, reason.to_string())
        }
    }
}

/// Handle WhatsApp inbound messages (POST request)
pub(super) async fn ingest_whatsapp(
    State(state): State<Arc<GatewayState>>,
    body: Bytes,
) -> impl IntoResponse {
    let adapter = WhatsAppInboundAdapter::new();
    let message = match adapter.parse(&body) {
        Ok(message) => message,
        Err(err) => {
            debug!("gateway ignoring whatsapp event: {}", err);
            return (StatusCode::OK, Json(json!({"status": "ignored"})));
        }
    };

    let phone_number = message
        .metadata
        .whatsapp_phone_number
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let Some(route) = resolve_route(Channel::WhatsApp, &phone_number, &state) else {
        info!(
            "gateway no route for whatsapp phone_number={}",
            phone_number
        );
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    let external_message_id = message.message_id.clone();
    let envelope = match build_envelope(
        route,
        Channel::WhatsApp,
        external_message_id,
        &message,
        &body,
    )
    .await
    {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("gateway failed to store raw payload: {}", err);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"status": "payload_store_failed"})),
            );
        }
    };
    enqueue_envelope(state.queue.clone(), envelope).await
}

/// Query parameters for WeChat webhook verification
#[derive(Debug, Deserialize)]
pub(super) struct WeChatVerifyParams {
    /// WeChat sends "signature" for URL verification, "msg_signature" for encrypted messages
    #[serde(alias = "msg_signature")]
    pub signature: Option<String>,
    pub timestamp: Option<String>,
    pub nonce: Option<String>,
    pub echostr: Option<String>,
}

/// Query parameters for WeChat Official Account webhook (GET/POST)
#[derive(Debug, Deserialize)]
pub(super) struct WeChatMpWebhookParams {
    pub signature: Option<String>,
    pub msg_signature: Option<String>,
    pub timestamp: Option<String>,
    pub nonce: Option<String>,
    pub echostr: Option<String>,
}

/// Handle WeChat webhook verification (GET request)
pub(super) async fn verify_wechat_webhook(
    Query(params): Query<WeChatVerifyParams>,
) -> impl IntoResponse {
    info!(
        "wechat verification request: signature={:?} timestamp={:?} nonce={:?} echostr_len={:?}",
        params.signature.as_deref(),
        params.timestamp.as_deref(),
        params.nonce.as_deref(),
        params.echostr.as_ref().map(|s| s.len())
    );

    match verify_wechat(
        params.signature.as_deref(),
        params.timestamp.as_deref(),
        params.nonce.as_deref(),
        params.echostr.as_deref(),
    ) {
        Ok(echostr) => {
            info!(
                "wechat verification succeeded, returning echostr len={}",
                echostr.len()
            );
            (StatusCode::OK, echostr)
        }
        Err(reason) => {
            warn!("wechat webhook verification failed: {}", reason);
            (StatusCode::FORBIDDEN, reason.to_string())
        }
    }
}

/// Handle WeChat Official Account webhook verification (GET request)
pub(super) async fn verify_wechat_mp_webhook(
    Query(params): Query<WeChatMpWebhookParams>,
) -> impl IntoResponse {
    info!(
        "wechat_mp verification request: signature={:?} msg_signature={:?} timestamp={:?} nonce={:?} echostr_len={:?}",
        params.signature.as_deref(),
        params.msg_signature.as_deref(),
        params.timestamp.as_deref(),
        params.nonce.as_deref(),
        params.echostr.as_ref().map(|s| s.len())
    );

    match verify_wechat_mp(
        params.signature.as_deref(),
        params.timestamp.as_deref(),
        params.nonce.as_deref(),
        params.echostr.as_deref(),
    ) {
        Ok(echostr) => {
            info!(
                "wechat_mp verification succeeded, returning echostr len={}",
                echostr.len()
            );
            (StatusCode::OK, echostr)
        }
        Err(reason) => {
            warn!("wechat_mp webhook verification failed: {}", reason);
            (StatusCode::FORBIDDEN, reason.to_string())
        }
    }
}

/// Handle WeChat inbound messages (POST request)
pub(super) async fn ingest_wechat(
    State(state): State<Arc<GatewayState>>,
    body: Bytes,
) -> impl IntoResponse {
    let body_preview = String::from_utf8_lossy(&body[..body.len().min(500)]);
    info!(
        "wechat POST received, body_len={}, preview={}",
        body.len(),
        body_preview
    );

    let adapter = WeChatInboundAdapter::new();
    let message = match adapter.parse(&body) {
        Ok(message) => message,
        Err(err) => {
            info!("gateway ignoring wechat event: {}", err);
            return (StatusCode::OK, Json(json!({"status": "ignored"})));
        }
    };

    let user_id = message
        .metadata
        .wechat_user_id
        .clone()
        .unwrap_or_else(|| message.sender.clone());

    info!(
        "wechat message parsed: user_id={}, content_preview={}",
        user_id,
        message
            .text_body
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(50)
            .collect::<String>()
    );

    let Some(route) = resolve_route(Channel::WeChat, &user_id, &state) else {
        info!("gateway no route for wechat user_id={}", user_id);
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    info!(
        "wechat route found: tenant={}, employee={}",
        route.tenant_id, route.employee_id
    );

    let external_message_id = message.message_id.clone();
    let envelope =
        match build_envelope(route, Channel::WeChat, external_message_id, &message, &body).await {
            Ok(envelope) => envelope,
            Err(err) => {
                error!("gateway failed to store raw payload: {}", err);
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"status": "payload_store_failed"})),
                );
            }
        };
    info!("wechat message enqueuing");
    enqueue_envelope(state.queue.clone(), envelope).await
}

/// Handle WeChat Official Account inbound messages (POST request)
pub(super) async fn ingest_wechat_mp(
    State(state): State<Arc<GatewayState>>,
    Query(params): Query<WeChatMpWebhookParams>,
    body: Bytes,
) -> Response {
    if let Err(reason) = verify_wechat_mp_message(
        params.signature.as_deref(),
        params.msg_signature.as_deref(),
        params.timestamp.as_deref(),
        params.nonce.as_deref(),
        &body,
    ) {
        warn!("wechat_mp POST signature verification failed: {}", reason);
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let body_preview = String::from_utf8_lossy(&body[..body.len().min(500)]);
    info!(
        "wechat_mp POST received, body_len={}, preview={}",
        body.len(),
        body_preview
    );

    let adapter = WeChatMpInboundAdapter::new();
    let message = match adapter.parse(&body) {
        Ok(message) => message,
        Err(err) => {
            info!("gateway ignoring wechat_mp event: {}", err);
            return (StatusCode::OK, WECHAT_MP_PASSIVE_ACK_BODY).into_response();
        }
    };

    let open_id = message
        .metadata
        .wechat_mp_open_id
        .clone()
        .unwrap_or_else(|| message.sender.clone());

    info!(
        "wechat_mp message parsed: open_id={}, content_preview={}",
        open_id,
        message
            .text_body
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(50)
            .collect::<String>()
    );

    let Some(route) = resolve_route(Channel::WeChatMp, &open_id, &state) else {
        info!("gateway no route for wechat_mp open_id={}", open_id);
        return wechat_mp_passive_ack_response(Some(&message));
    };

    info!(
        "wechat_mp route found: tenant={}, employee={}",
        route.tenant_id, route.employee_id
    );

    let ack_response = wechat_mp_passive_ack_response(Some(&message));
    let queue = state.queue.clone();
    let external_message_id = message.message_id.clone();
    let raw_payload = body.clone();
    tokio::spawn(async move {
        process_wechat_mp_async(queue, route, external_message_id, message, raw_payload).await;
    });
    ack_response
}

async fn process_wechat_mp_async(
    queue: Arc<dyn IngestionQueue>,
    route: RouteDecision,
    external_message_id: Option<String>,
    message: InboundMessage,
    raw_payload: Bytes,
) {
    let envelope = match build_envelope(
        route,
        Channel::WeChatMp,
        external_message_id,
        &message,
        &raw_payload,
    )
    .await
    {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("wechat_mp async build_envelope failed: {}", err);
            return;
        }
    };

    info!("wechat_mp async message enqueuing");
    let dedupe_key = envelope.dedupe_key.clone();
    let (status_code, body) = enqueue_envelope(queue, envelope).await;
    let enqueue_status = body
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    if status_code == StatusCode::OK {
        info!(
            "wechat_mp async enqueue finished: status={} dedupe_key={}",
            enqueue_status, dedupe_key
        );
    } else {
        error!(
            "wechat_mp async enqueue failed: http_status={} status={} dedupe_key={}",
            status_code.as_u16(),
            enqueue_status,
            dedupe_key
        );
    }
}

fn wechat_mp_passive_ack_response(message: Option<&InboundMessage>) -> Response {
    if let (Some(reply_text), Some(message)) = (resolve_wechat_mp_passive_reply_text(), message) {
        if let Some(response) = build_wechat_mp_passive_text_response(message, &reply_text) {
            return response;
        }
    }
    (StatusCode::OK, WECHAT_MP_PASSIVE_ACK_BODY).into_response()
}

fn resolve_wechat_mp_passive_reply_text() -> Option<String> {
    std::env::var(WECHAT_MP_PASSIVE_REPLY_TEXT_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_wechat_mp_passive_text_response(
    message: &InboundMessage,
    reply_text: &str,
) -> Option<Response> {
    let to_user = message
        .metadata
        .wechat_mp_open_id
        .as_deref()
        .unwrap_or(message.sender.as_str())
        .trim();
    let from_user = message
        .metadata
        .wechat_mp_app_id
        .as_deref()
        .unwrap_or(message.recipient.as_str())
        .trim();

    if to_user.is_empty() || from_user.is_empty() {
        return None;
    }

    let cdata_safe = |value: &str| value.replace("]]>", "]]]]><![CDATA[>");
    let xml = format!(
        "<xml><ToUserName><![CDATA[{to_user}]]></ToUserName><FromUserName><![CDATA[{from_user}]]></FromUserName><CreateTime>{create_time}</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[{content}]]></Content></xml>",
        to_user = cdata_safe(to_user),
        from_user = cdata_safe(from_user),
        create_time = Utc::now().timestamp(),
        content = cdata_safe(reply_text),
    );

    Some(
        (
            StatusCode::OK,
            [(CONTENT_TYPE, "application/xml; charset=utf-8")],
            xml,
        )
            .into_response(),
    )
}

/// Handle Lark inbound messages (POST request)
pub(super) async fn ingest_lark(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Handle URL verification challenge
    if let Some(challenge) = verify_lark_challenge(&body) {
        info!("lark URL verification, returning challenge");
        return (StatusCode::OK, Json(json!({"challenge": challenge})));
    }

    // Verify signature
    if let Err(reason) = verify_lark(&headers, &body) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status": reason})));
    }

    let adapter = LarkInboundAdapter::new();
    let message = match adapter.parse(&body) {
        Ok(message) => message,
        Err(err) => {
            info!("gateway ignoring lark event: {}", err);
            return (StatusCode::OK, Json(json!({"status": "ignored"})));
        }
    };

    let chat_id = message
        .metadata
        .lark_chat_id
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    info!(
        "lark message received: chat_id={}, sender={}, message_id={:?}, text_preview={}",
        chat_id,
        message.sender,
        message.message_id,
        message
            .text_body
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(50)
            .collect::<String>()
    );

    let Some(route) = resolve_route(Channel::Lark, &chat_id, &state) else {
        info!(
            "gateway no route for lark chat_id={}, channel_defaults_has_lark={}, global_default_employee={:?}",
            chat_id,
            state.config.channel_defaults.contains_key(&Channel::Lark),
            state.config.defaults.employee_id
        );
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    let external_message_id = message.message_id.clone();
    let envelope =
        match build_envelope(route, Channel::Lark, external_message_id, &message, &body).await {
            Ok(envelope) => envelope,
            Err(err) => {
                error!("gateway failed to store raw payload: {}", err);
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"status": "payload_store_failed"})),
                );
            }
        };
    enqueue_envelope(state.queue.clone(), envelope).await
}

pub(super) async fn enqueue_envelope(
    queue: Arc<dyn IngestionQueue>,
    envelope: IngestionEnvelope,
) -> (StatusCode, Json<serde_json::Value>) {
    let result = tokio::task::spawn_blocking(move || queue.enqueue(&envelope)).await;
    match result {
        Ok(Ok(result)) => {
            if result.inserted {
                (StatusCode::OK, Json(json!({"status": "accepted"})))
            } else {
                (StatusCode::OK, Json(json!({"status": "duplicate"})))
            }
        }
        Ok(Err(err)) => {
            error!("gateway enqueue error: {}", err);
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"status": "enqueue_failed"})),
            )
        }
        Err(err) => {
            error!("gateway enqueue join error: {}", err);
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"status": "enqueue_failed"})),
            )
        }
    }
}

const NO_REPLY_MARKERS: [&str; 5] = [
    "noreply",
    "no-reply",
    "do-not-reply",
    "mailer-daemon",
    "postmaster",
];
const EMAIL_QUEUE_TEXT_PREVIEW_MAX_BYTES: usize = 16 * 1024;
const EMAIL_QUEUE_TEXT_TRUNCATED_SUFFIX: &str = "\n\n[truncated for queue delivery]";

fn payload_contains_no_reply_marker(payload: &PostmarkInboundPayload) -> bool {
    let candidates = [payload.from.as_deref(), payload.reply_to.as_deref()];
    candidates
        .into_iter()
        .flatten()
        .any(contains_no_reply_marker)
}

fn contains_no_reply_marker(value: &str) -> bool {
    let emails = extract_emails(value);
    if emails.is_empty() {
        return false;
    }
    emails.into_iter().any(|email| {
        let normalized = email.trim().to_ascii_lowercase();
        NO_REPLY_MARKERS
            .iter()
            .any(|marker| normalized.contains(marker))
    })
}

fn build_queue_payload(channel: Channel, message: &InboundMessage) -> IngestionPayload {
    let mut payload = IngestionPayload::from_inbound(message);
    if channel == Channel::Email {
        // Keep queue envelopes small; the worker loads the authoritative email payload from blob.
        payload.attachments.clear();
        payload.text_body = payload
            .text_body
            .as_deref()
            .and_then(compact_email_text_for_queue)
            .or_else(|| {
                payload
                    .html_body
                    .as_deref()
                    .and_then(compact_email_html_for_queue)
            });
        payload.html_body = None;
    }
    payload
}

fn compact_email_text_for_queue(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_for_queue(
        trimmed,
        EMAIL_QUEUE_TEXT_PREVIEW_MAX_BYTES,
        EMAIL_QUEUE_TEXT_TRUNCATED_SUFFIX,
    ))
}

fn compact_email_html_for_queue(html: &str) -> Option<String> {
    derive_inbound_email_text(None, None, Some(html))
        .and_then(|text| compact_email_text_for_queue(&text))
}

fn truncate_for_queue(input: &str, max_bytes: usize, suffix: &str) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }

    if suffix.len() >= max_bytes {
        return truncate_utf8_for_queue(input, max_bytes);
    }

    let mut end = max_bytes - suffix.len();
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }

    let mut output = input[..end].to_string();
    output.push_str(suffix);
    output
}

fn truncate_utf8_for_queue(input: &str, max_bytes: usize) -> String {
    let mut end = input.len().min(max_bytes);
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    input[..end].to_string()
}

async fn rewrite_email_payload_attachments_to_blob_refs(
    envelope_id: Uuid,
    received_at: chrono::DateTime<chrono::Utc>,
    raw_payload: &[u8],
) -> Vec<u8> {
    let mut payload_json: serde_json::Value = match serde_json::from_slice(raw_payload) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "gateway failed to parse email raw payload for attachment offload: {}",
                err
            );
            return raw_payload.to_vec();
        }
    };
    let Some(attachments) = payload_json
        .get_mut("Attachments")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return raw_payload.to_vec();
    };

    for (index, attachment) in attachments.iter_mut().enumerate() {
        let Some(obj) = attachment.as_object_mut() else {
            continue;
        };
        let content = obj
            .get("Content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if content.is_empty() {
            continue;
        }
        let file_name = obj
            .get("Name")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("attachment");

        let decoded = match BASE64_STANDARD.decode(content.as_bytes()) {
            Ok(bytes) => bytes,
            Err(err) => {
                warn!(
                    "gateway failed to decode email attachment '{}' for blob offload: {}",
                    file_name, err
                );
                continue;
            }
        };
        if decoded.is_empty() {
            continue;
        }

        match raw_payload_store::upload_attachment(
            envelope_id,
            received_at,
            index,
            file_name,
            &decoded,
        )
        .await
        {
            Ok(storage_ref) => {
                obj.insert(
                    "StorageRef".to_string(),
                    serde_json::Value::String(storage_ref),
                );
                obj.insert(
                    "Content".to_string(),
                    serde_json::Value::String(String::new()),
                );
                obj.insert(
                    "ContentLength".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(decoded.len())),
                );
            }
            Err(err) => {
                warn!(
                    "gateway failed to upload email attachment '{}' to blob: {}",
                    file_name, err
                );
            }
        }
    }

    serde_json::to_vec(&payload_json).unwrap_or_else(|err| {
        warn!(
            "gateway failed to serialize rewritten email raw payload; using original: {}",
            err
        );
        raw_payload.to_vec()
    })
}

fn rewrite_email_payload_attachments_to_blob_refs_blocking(
    envelope_id: Uuid,
    received_at: chrono::DateTime<chrono::Utc>,
    raw_payload: &[u8],
) -> Vec<u8> {
    let mut payload_json: serde_json::Value = match serde_json::from_slice(raw_payload) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "gateway failed to parse email raw payload for attachment offload: {}",
                err
            );
            return raw_payload.to_vec();
        }
    };
    let Some(attachments) = payload_json
        .get_mut("Attachments")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return raw_payload.to_vec();
    };

    for (index, attachment) in attachments.iter_mut().enumerate() {
        let Some(obj) = attachment.as_object_mut() else {
            continue;
        };
        let content = obj
            .get("Content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if content.is_empty() {
            continue;
        }
        let file_name = obj
            .get("Name")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("attachment");

        let decoded = match BASE64_STANDARD.decode(content.as_bytes()) {
            Ok(bytes) => bytes,
            Err(err) => {
                warn!(
                    "gateway failed to decode email attachment '{}' for blob offload: {}",
                    file_name, err
                );
                continue;
            }
        };
        if decoded.is_empty() {
            continue;
        }

        match raw_payload_store::upload_attachment_blocking(
            envelope_id,
            received_at,
            index,
            file_name,
            &decoded,
        ) {
            Ok(storage_ref) => {
                obj.insert(
                    "StorageRef".to_string(),
                    serde_json::Value::String(storage_ref),
                );
                obj.insert(
                    "Content".to_string(),
                    serde_json::Value::String(String::new()),
                );
                obj.insert(
                    "ContentLength".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(decoded.len())),
                );
            }
            Err(err) => {
                warn!(
                    "gateway failed to upload email attachment '{}' to blob: {}",
                    file_name, err
                );
            }
        }
    }

    serde_json::to_vec(&payload_json).unwrap_or_else(|err| {
        warn!(
            "gateway failed to serialize rewritten email raw payload; using original: {}",
            err
        );
        raw_payload.to_vec()
    })
}

pub(super) async fn build_envelope(
    route: RouteDecision,
    channel: Channel,
    external_message_id: Option<String>,
    message: &InboundMessage,
    raw_payload: &[u8],
) -> Result<IngestionEnvelope, RawPayloadStoreError> {
    let envelope_id = Uuid::new_v4();
    let received_at = Utc::now();
    let queue_payload = build_queue_payload(channel, message);
    let dedupe_key = build_dedupe_key(
        &route.tenant_id,
        &route.employee_id,
        channel,
        external_message_id.as_deref(),
        raw_payload,
    );
    let stored_payload_bytes = if channel == Channel::Email {
        rewrite_email_payload_attachments_to_blob_refs(envelope_id, received_at, raw_payload).await
    } else {
        raw_payload.to_vec()
    };
    let raw_payload_ref = if raw_payload.is_empty() {
        None
    } else if channel == Channel::Email {
        // Email queue payloads are intentionally compact, so the archived raw payload
        // becomes the authoritative source for full body reconstruction and attachments.
        Some(
            raw_payload_store::upload_raw_payload(envelope_id, received_at, &stored_payload_bytes)
                .await?,
        )
    } else {
        match raw_payload_store::upload_raw_payload(envelope_id, received_at, &stored_payload_bytes)
            .await
        {
            Ok(payload_ref) => Some(payload_ref),
            Err(err) => {
                // Non-blocking: log error but continue without raw payload archival
                tracing::error!("failed to upload raw payload: {}", err);
                None
            }
        }
    };
    Ok(IngestionEnvelope {
        envelope_id,
        received_at,
        tenant_id: Some(route.tenant_id),
        employee_id: route.employee_id,
        channel,
        external_message_id,
        dedupe_key,
        payload: queue_payload,
        raw_payload_ref,
        account_id: None,
    })
}

/// Handle workspace brief creation request
/// POST /api/workspace/create-brief
pub(super) async fn create_workspace_brief(
    State(state): State<Arc<GatewayState>>,
    Json(request): Json<CreateWorkspaceBriefRequest>,
) -> impl IntoResponse {
    info!(
        "workspace brief request: founder={} email={}",
        request.founder_name, request.founder_email
    );

    // Determine employee to route to (default to oliver)
    let employee_id = state
        .config
        .defaults
        .employee_id
        .clone()
        .unwrap_or_else(|| "oliver".to_string());

    let tenant_id = state
        .config
        .defaults
        .tenant_id
        .clone()
        .unwrap_or_else(|| "default".to_string());

    // Build the prompt for Codex
    let venture_name = request.venture_name.as_deref().unwrap_or("Startup");
    let thesis = request
        .thesis
        .as_deref()
        .unwrap_or("Building something great");
    let stage = request.stage.as_deref().unwrap_or("idea");
    let horizon = request.plan_horizon_days.unwrap_or(30);
    let goals_text = if request.goals.is_empty() {
        "- Define initial goals".to_string()
    } else {
        request
            .goals
            .iter()
            .map(|g| format!("- {}", g))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let assets_text = request
        .current_assets
        .as_ref()
        .filter(|a| !a.is_empty())
        .map(|a| {
            a.iter()
                .map(|x| format!("- {}", x))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "- None listed yet".to_string());

    let prompt = format!(
        r#"Create a Startup Workspace Brief Google Doc for this founder and share it with them.

## Founder Information
- **Name:** {founder_name}
- **Email:** {founder_email}
- **Venture Name:** {venture_name}
- **Thesis:** {thesis}
- **Stage:** {stage}
- **Planning Horizon:** {horizon} days

## Goals (30-90 days)
{goals_text}

## Current Assets
{assets_text}

## Instructions
1. Create a new Google Doc titled "Startup Workspace Brief - {venture_name}"
2. Add the following sections with professional formatting:
   - Executive Summary (synthesize the thesis and stage)
   - Founder Profile
   - 30-90 Day Goals (expand on each goal with suggested milestones)
   - Current Assets & Resources
   - Recommended Next Steps
   - Key Metrics to Track
3. Share the document with {founder_email} as a writer
4. Send an email to {founder_email} with the document link and a brief introduction

Use the google-docs skill to create and share the document."#,
        founder_name = request.founder_name,
        founder_email = request.founder_email,
        venture_name = venture_name,
        thesis = thesis,
        stage = stage,
        horizon = horizon,
        goals_text = goals_text,
        assets_text = assets_text,
    );

    // Build InboundMessage with the prompt using Email channel
    let message_id = format!("<workspace-brief-{}@dowhiz.com>", Uuid::new_v4());
    let recipient_email = state
        .employee_directory
        .employees
        .iter()
        .find(|e| e.id == employee_id)
        .and_then(|e| e.address_set.iter().next())
        .cloned()
        .unwrap_or_else(|| format!("{}@dowhiz.com", employee_id));
    let subject = format!("Create Workspace Brief for {}", venture_name);

    let message = InboundMessage {
        channel: Channel::Email,
        sender: request.founder_email.clone(),
        sender_name: Some(request.founder_name.clone()),
        recipient: recipient_email.clone(),
        subject: Some(subject.clone()),
        text_body: Some(prompt.clone()),
        html_body: None,
        thread_id: message_id.clone(),
        message_id: Some(message_id.clone()),
        attachments: Vec::new(),
        reply_to: vec![request.founder_email.clone()],
        raw_payload: Vec::new(),
        metadata: ChannelMetadata::default(),
    };

    // Build synthetic Postmark-style email payload
    let email_payload = json!({
        "From": format!("{} <{}>", request.founder_name, request.founder_email),
        "To": recipient_email,
        "ReplyTo": request.founder_email,
        "Subject": subject,
        "TextBody": prompt,
        "MessageID": message_id
    });
    let raw_payload = serde_json::to_vec(&email_payload).unwrap_or_default();

    let route = RouteDecision {
        tenant_id,
        employee_id,
    };

    let external_message_id = message.message_id.clone();
    let mut envelope = match build_envelope(
        route,
        Channel::Email,
        external_message_id,
        &message,
        &raw_payload,
    )
    .await
    {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("failed to build workspace brief envelope: {}", err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "envelope_build_failed", "error": err.to_string()})),
            );
        }
    };

    envelope.account_id = request.account_id;

    let task_id = envelope.envelope_id.to_string();
    let result = enqueue_envelope(state.queue.clone(), envelope).await;

    // Augment response with task_id for potential polling
    match result {
        (StatusCode::OK, Json(mut body)) => {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("task_id".to_string(), json!(task_id));
            }
            (StatusCode::OK, Json(body))
        }
        other => other,
    }
}

pub(super) async fn create_90_day_plan(
    State(state): State<Arc<GatewayState>>,
    Json(request): Json<Create90DayPlanRequest>,
) -> impl IntoResponse {
    info!(
        "90-day plan request: founder={} email={}",
        request.founder_name, request.founder_email
    );

    let employee_id = state
        .config
        .defaults
        .employee_id
        .clone()
        .unwrap_or_else(|| "oliver".to_string());

    let tenant_id = state
        .config
        .defaults
        .tenant_id
        .clone()
        .unwrap_or_else(|| "default".to_string());

    let venture_name = request.venture_name.as_deref().unwrap_or("Startup");
    let thesis = request
        .thesis
        .as_deref()
        .unwrap_or("Building something great");
    let stage = request.stage.as_deref().unwrap_or("idea");
    let horizon = request.plan_horizon_days.unwrap_or(90);
    let goals_text = if request.goals.is_empty() {
        "- Define initial goals".to_string()
    } else {
        request
            .goals
            .iter()
            .map(|g| format!("- {}", g))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let assets_text = request
        .current_assets
        .as_ref()
        .filter(|a| !a.is_empty())
        .map(|a| {
            a.iter()
                .map(|x| format!("- {}", x))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "- None listed yet".to_string());

    let prompt = format!(
        r#"Create a professional, well-formatted {horizon}-Day Plan Google Doc for this founder and share it with them.

## Founder Information
- **Name:** {founder_name}
- **Email:** {founder_email}
- **Venture Name:** {venture_name}
- **Thesis:** {thesis}
- **Stage:** {stage}

## Goals
{goals_text}

## Current Assets
{assets_text}

## Instructions
1. Create a new Google Doc titled "{horizon}-Day Plan - {venture_name}"
2. Use professional formatting throughout:
   - Clear heading hierarchy (Title, H1, H2, H3)
   - Consistent spacing and indentation
   - Bullet points and numbered lists where appropriate
   - Bold text for key terms and deadlines
   - Tables for weekly breakdowns if helpful
3. Structure the document with:
   - Executive Summary (1 paragraph synthesizing the goals and timeline)
   - Week-by-week breakdown for {horizon} days ({weeks} weeks total)
   - Each week should have 2-3 concrete, actionable tasks derived from the goals
   - Milestones section marking key checkpoints at day 30, 60, and 90
   - Success Metrics (specific, measurable criteria for each goal)
   - Resources Needed (based on current assets and identified gaps)
4. Share the document with {founder_email} as a writer
5. Send an email to {founder_email} with the document link

Use the google-docs skill to create and share the document."#,
        founder_name = request.founder_name,
        founder_email = request.founder_email,
        venture_name = venture_name,
        thesis = thesis,
        stage = stage,
        horizon = horizon,
        weeks = horizon / 7,
        goals_text = goals_text,
        assets_text = assets_text,
    );

    let message_id = format!("<90-day-plan-{}@dowhiz.com>", Uuid::new_v4());
    let recipient_email = state
        .employee_directory
        .employees
        .iter()
        .find(|e| e.id == employee_id)
        .and_then(|e| e.address_set.iter().next())
        .cloned()
        .unwrap_or_else(|| format!("{}@dowhiz.com", employee_id));
    let subject = format!("Create {}-Day Plan for {}", horizon, venture_name);

    let message = InboundMessage {
        channel: Channel::Email,
        sender: request.founder_email.clone(),
        sender_name: Some(request.founder_name.clone()),
        recipient: recipient_email.clone(),
        subject: Some(subject.clone()),
        text_body: Some(prompt.clone()),
        html_body: None,
        thread_id: message_id.clone(),
        message_id: Some(message_id.clone()),
        attachments: Vec::new(),
        reply_to: vec![request.founder_email.clone()],
        raw_payload: Vec::new(),
        metadata: ChannelMetadata::default(),
    };

    let email_payload = json!({
        "From": format!("{} <{}>", request.founder_name, request.founder_email),
        "To": recipient_email,
        "ReplyTo": request.founder_email,
        "Subject": subject,
        "TextBody": prompt,
        "MessageID": message_id
    });
    let raw_payload = serde_json::to_vec(&email_payload).unwrap_or_default();

    let route = RouteDecision {
        tenant_id,
        employee_id,
    };

    let external_message_id = message.message_id.clone();
    let mut envelope = match build_envelope(
        route,
        Channel::Email,
        external_message_id,
        &message,
        &raw_payload,
    )
    .await
    {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("failed to build 90-day plan envelope: {}", err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "envelope_build_failed", "error": err.to_string()})),
            );
        }
    };

    envelope.account_id = request.account_id;

    let task_id = envelope.envelope_id.to_string();
    let result = enqueue_envelope(state.queue.clone(), envelope).await;

    match result {
        (StatusCode::OK, Json(mut body)) => {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("task_id".to_string(), json!(task_id));
            }
            (StatusCode::OK, Json(body))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use axum::body::to_bytes;
    use scheduler_module::adapters::slack::{SlackEventWrapper, SlackMessageEvent};
    use scheduler_module::channel::Attachment;
    use scheduler_module::employee_config::EmployeeDirectory;
    use scheduler_module::ingestion_queue::{
        EnqueueResult, IngestionQueue, IngestionQueueError, QueuedEnvelope,
    };
    use sha1::{Digest, Sha1};

    #[derive(Default)]
    struct MockIngestionQueue {
        envelopes: Mutex<Vec<IngestionEnvelope>>,
    }

    impl MockIngestionQueue {
        fn len(&self) -> usize {
            self.envelopes
                .lock()
                .expect("recording queue mutex poisoned")
                .len()
        }
    }

    impl IngestionQueue for MockIngestionQueue {
        fn enqueue(
            &self,
            envelope: &IngestionEnvelope,
        ) -> Result<EnqueueResult, IngestionQueueError> {
            self.envelopes
                .lock()
                .expect("recording queue mutex poisoned")
                .push(envelope.clone());
            Ok(EnqueueResult { inserted: true })
        }

        fn claim_next(
            &self,
            _employee_id: &str,
        ) -> Result<Option<QueuedEnvelope>, IngestionQueueError> {
            Ok(None)
        }

        fn mark_done(&self, _id: &Uuid) -> Result<(), IngestionQueueError> {
            Ok(())
        }

        fn mark_failed(&self, _id: &Uuid, _error: &str) -> Result<(), IngestionQueueError> {
            Ok(())
        }
    }

    fn make_gateway_state(
        queue: Arc<dyn IngestionQueue>,
        default_employee_id: Option<&str>,
    ) -> Arc<GatewayState> {
        Arc::new(GatewayState {
            config: super::super::state::GatewayConfig {
                defaults: super::super::config::GatewayDefaultsConfig {
                    tenant_id: Some("tenant-test".to_string()),
                    employee_id: default_employee_id.map(str::to_string),
                },
                routes: HashMap::new(),
                channel_defaults: HashMap::new(),
            },
            employee_directory: EmployeeDirectory {
                employees: Vec::new(),
                employee_by_id: HashMap::new(),
                default_employee_id: None,
                service_addresses: HashSet::new(),
            },
            address_to_employee: HashMap::new(),
            queue,
            drive_changes_manager: None,
            drive_change_notifier: None,
        })
    }

    fn make_wechat_mp_text_xml(open_id: &str, app_id: &str, content: &str, msg_id: &str) -> String {
        format!(
            r#"<xml>
<ToUserName><![CDATA[{app_id}]]></ToUserName>
<FromUserName><![CDATA[{open_id}]]></FromUserName>
<CreateTime>1712540000</CreateTime>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[{content}]]></Content>
<MsgId>{msg_id}</MsgId>
</xml>"#
        )
    }

    fn sha1_sorted_parts(parts: &[&str]) -> String {
        let mut sorted = parts.to_vec();
        sorted.sort();
        let data = sorted.join("");
        let mut hasher = Sha1::new();
        hasher.update(data.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn make_wechat_mp_signed_params(body: &[u8]) -> WeChatMpWebhookParams {
        let timestamp = "1712540000".to_string();
        let nonce = "nonce-123".to_string();
        let signature = std::env::var("WECHAT_MP_TOKEN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|token| sha1_sorted_parts(&[token.as_str(), timestamp.as_str(), nonce.as_str()]));
        let msg_signature = std::str::from_utf8(body)
            .ok()
            .and_then(|raw| {
                let cdata_start = "<Encrypt><![CDATA[";
                let cdata_end = "]]></Encrypt>";
                raw.find(cdata_start).and_then(|start| {
                    let begin = start + cdata_start.len();
                    raw[begin..]
                        .find(cdata_end)
                        .map(|len| raw[begin..begin + len].to_string())
                })
            })
            .and_then(|encrypt| {
                std::env::var("WECHAT_MP_TOKEN")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .map(|token| {
                        sha1_sorted_parts(&[
                            token.as_str(),
                            timestamp.as_str(),
                            nonce.as_str(),
                            encrypt.as_str(),
                        ])
                    })
            });

        WeChatMpWebhookParams {
            signature,
            msg_signature,
            timestamp: Some(timestamp),
            nonce: Some(nonce),
            echostr: None,
        }
    }

    async fn response_body_text(response: Response) -> String {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        String::from_utf8(body.to_vec()).expect("response body utf8")
    }

    #[tokio::test]
    async fn ingest_wechat_mp_returns_success_when_parse_fails() {
        let queue = Arc::new(MockIngestionQueue::default());
        let state = make_gateway_state(queue, None);
        let body = Bytes::from_static(b"<xml><MsgType><![CDATA[text]]></MsgType></xml>");
        let params = make_wechat_mp_signed_params(&body);

        let response = ingest_wechat_mp(State(state), Query(params), body).await;
        let status = response.status();
        let response_body = response_body_text(response).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_body, WECHAT_MP_PASSIVE_ACK_BODY);
    }

    #[tokio::test]
    async fn ingest_wechat_mp_returns_success_when_no_route_found() {
        let queue = Arc::new(MockIngestionQueue::default());
        let state = make_gateway_state(queue, None);
        let xml = make_wechat_mp_text_xml("openid-no-route", "gh_app_1", "hello", "1001");
        let body = Bytes::from(xml);
        let params = make_wechat_mp_signed_params(&body);

        let response = ingest_wechat_mp(State(state), Query(params), body).await;
        let status = response.status();
        let response_body = response_body_text(response).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_body, WECHAT_MP_PASSIVE_ACK_BODY);
    }

    #[tokio::test]
    async fn ingest_wechat_mp_acknowledges_immediately_and_enqueues_async() {
        let queue = Arc::new(MockIngestionQueue::default());
        let queue_for_assert = queue.clone();
        let state = make_gateway_state(queue, Some("employee-1"));
        let xml =
            make_wechat_mp_text_xml("openid-async", "gh_app_1", "你是谁？你能做什么？", "1002");
        let body = Bytes::from(xml);
        let params = make_wechat_mp_signed_params(&body);

        let response = ingest_wechat_mp(State(state), Query(params), body).await;
        let status = response.status();
        let response_body = response_body_text(response).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_body, WECHAT_MP_PASSIVE_ACK_BODY);

        for _ in 0..50 {
            if queue_for_assert.len() > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(queue_for_assert.len(), 1);
    }

    #[tokio::test]
    async fn build_wechat_mp_passive_text_response_returns_xml() {
        let message = InboundMessage {
            channel: Channel::WeChatMp,
            sender: "openid_123".to_string(),
            sender_name: None,
            recipient: "gh_app_456".to_string(),
            subject: None,
            text_body: Some("hello".to_string()),
            html_body: None,
            thread_id: "wechat_mp:gh_app_456:openid_123".to_string(),
            message_id: Some("msg_1".to_string()),
            attachments: Vec::new(),
            reply_to: vec!["openid_123".to_string()],
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                wechat_mp_app_id: Some("gh_app_456".to_string()),
                wechat_mp_open_id: Some("openid_123".to_string()),
                ..Default::default()
            },
        };

        let response = build_wechat_mp_passive_text_response(&message, "已收到，处理中")
            .expect("xml response");
        let status = response.status();
        let body = response_body_text(response).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<MsgType><![CDATA[text]]></MsgType>"));
        assert!(body.contains("<ToUserName><![CDATA[openid_123]]></ToUserName>"));
        assert!(body.contains("<FromUserName><![CDATA[gh_app_456]]></FromUserName>"));
        assert!(body.contains("<Content><![CDATA[已收到，处理中]]></Content>"));
    }

    #[test]
    fn payload_contains_no_reply_marker_detects_from() {
        let payload: PostmarkInboundPayload =
            serde_json::from_str(r#"{"From":"noreply@example.com"}"#).expect("payload");
        assert!(payload_contains_no_reply_marker(&payload));
    }

    #[test]
    fn payload_contains_no_reply_marker_detects_reply_to() {
        let payload: PostmarkInboundPayload =
            serde_json::from_str(r#"{"From":"user@example.com","ReplyTo":"no-reply@x.com"}"#)
                .expect("payload");
        assert!(payload_contains_no_reply_marker(&payload));
    }

    #[test]
    fn payload_contains_no_reply_marker_allows_normal_sender() {
        let payload: PostmarkInboundPayload =
            serde_json::from_str(r#"{"From":"user@example.com"}"#).expect("payload");
        assert!(!payload_contains_no_reply_marker(&payload));
    }

    #[test]
    fn payload_contains_no_reply_marker_detects_mailer_daemon() {
        let payload: PostmarkInboundPayload =
            serde_json::from_str(r#"{"From":"mailer-daemon@googlemail.com"}"#).expect("payload");
        assert!(payload_contains_no_reply_marker(&payload));
    }

    #[test]
    fn build_queue_payload_clears_email_attachments() {
        let message = InboundMessage {
            channel: Channel::Email,
            sender: "a@example.com".to_string(),
            sender_name: None,
            recipient: "svc@example.com".to_string(),
            subject: Some("s".to_string()),
            text_body: Some("t".to_string()),
            html_body: None,
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![Attachment {
                name: "a.txt".to_string(),
                content_type: "text/plain".to_string(),
                content: "Zm9v".to_string(),
            }],
            reply_to: vec!["a@example.com".to_string()],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Email, &message);
        assert!(payload.attachments.is_empty());
    }

    #[test]
    fn build_queue_payload_keeps_non_email_attachments() {
        let message = InboundMessage {
            channel: Channel::Slack,
            sender: "U123".to_string(),
            sender_name: None,
            recipient: "C456".to_string(),
            subject: None,
            text_body: Some("hello".to_string()),
            html_body: None,
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![Attachment {
                name: "file.pdf".to_string(),
                content_type: "application/pdf".to_string(),
                content: "placeholder".to_string(),
            }],
            reply_to: vec!["C456".to_string()],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Slack, &message);
        assert_eq!(payload.attachments.len(), 1);
    }

    #[test]
    fn should_enqueue_slack_message_accepts_app_mention() {
        let wrapper = SlackEventWrapper {
            event_type: "event_callback".to_string(),
            challenge: None,
            token: None,
            team_id: Some("T1".to_string()),
            api_app_id: Some("A1".to_string()),
            event: Some(SlackMessageEvent {
                event_type: "app_mention".to_string(),
                subtype: None,
                channel: Some("C1".to_string()),
                user: Some("U1".to_string()),
                text: Some("<@B1> hi".to_string()),
                ts: "1.01".to_string(),
                thread_ts: None,
                bot_id: None,
                app_id: None,
                files: None,
                channel_type: Some("channel".to_string()),
                event_ts: None,
            }),
            event_id: Some("Ev1".to_string()),
            event_time: None,
        };

        assert!(should_enqueue_slack_message(&wrapper, Some("B1")));
    }

    #[test]
    fn should_enqueue_slack_message_rejects_channel_message_without_bot_mention() {
        let wrapper = SlackEventWrapper {
            event_type: "event_callback".to_string(),
            challenge: None,
            token: None,
            team_id: Some("T1".to_string()),
            api_app_id: Some("A1".to_string()),
            event: Some(SlackMessageEvent {
                event_type: "message".to_string(),
                subtype: None,
                channel: Some("C1".to_string()),
                user: Some("U1".to_string()),
                text: Some("hello world".to_string()),
                ts: "1.02".to_string(),
                thread_ts: None,
                bot_id: None,
                app_id: None,
                files: None,
                channel_type: Some("channel".to_string()),
                event_ts: None,
            }),
            event_id: Some("Ev2".to_string()),
            event_time: None,
        };

        assert!(!should_enqueue_slack_message(&wrapper, Some("B1")));
    }

    #[test]
    fn should_enqueue_slack_message_accepts_dm_message() {
        let wrapper = SlackEventWrapper {
            event_type: "event_callback".to_string(),
            challenge: None,
            token: None,
            team_id: Some("T1".to_string()),
            api_app_id: Some("A1".to_string()),
            event: Some(SlackMessageEvent {
                event_type: "message".to_string(),
                subtype: None,
                channel: Some("D1".to_string()),
                user: Some("U1".to_string()),
                text: Some("hello in dm".to_string()),
                ts: "1.03".to_string(),
                thread_ts: None,
                bot_id: None,
                app_id: None,
                files: None,
                channel_type: Some("im".to_string()),
                event_ts: None,
            }),
            event_id: Some("Ev3".to_string()),
            event_time: None,
        };

        assert!(should_enqueue_slack_message(&wrapper, None));
    }

    #[test]
    fn should_enqueue_slack_message_accepts_follow_up_in_engaged_thread() {
        let root = SlackEventWrapper {
            event_type: "event_callback".to_string(),
            challenge: None,
            token: None,
            team_id: Some("T_engaged".to_string()),
            api_app_id: Some("A1".to_string()),
            event: Some(SlackMessageEvent {
                event_type: "app_mention".to_string(),
                subtype: None,
                channel: Some("C_engaged".to_string()),
                user: Some("U1".to_string()),
                text: Some("<@B1> start thread".to_string()),
                ts: "9.01".to_string(),
                thread_ts: None,
                bot_id: None,
                app_id: None,
                files: None,
                channel_type: Some("channel".to_string()),
                event_ts: None,
            }),
            event_id: Some("Ev_engaged_root".to_string()),
            event_time: None,
        };
        assert!(should_enqueue_slack_message(&root, Some("B1")));

        let follow_up = SlackEventWrapper {
            event_type: "event_callback".to_string(),
            challenge: None,
            token: None,
            team_id: Some("T_engaged".to_string()),
            api_app_id: Some("A1".to_string()),
            event: Some(SlackMessageEvent {
                event_type: "message".to_string(),
                subtype: None,
                channel: Some("C_engaged".to_string()),
                user: Some("U1".to_string()),
                text: Some("link for follow-up".to_string()),
                ts: "9.02".to_string(),
                thread_ts: Some("9.01".to_string()),
                bot_id: None,
                app_id: None,
                files: None,
                channel_type: Some("channel".to_string()),
                event_ts: None,
            }),
            event_id: Some("Ev_engaged_reply".to_string()),
            event_time: None,
        };

        assert!(should_enqueue_slack_message(&follow_up, Some("B1")));
    }

    #[test]
    fn should_enqueue_slack_message_rejects_follow_up_in_unengaged_thread() {
        let wrapper = SlackEventWrapper {
            event_type: "event_callback".to_string(),
            challenge: None,
            token: None,
            team_id: Some("T_unengaged".to_string()),
            api_app_id: Some("A1".to_string()),
            event: Some(SlackMessageEvent {
                event_type: "message".to_string(),
                subtype: None,
                channel: Some("C_unengaged".to_string()),
                user: Some("U1".to_string()),
                text: Some("plain thread reply".to_string()),
                ts: "10.02".to_string(),
                thread_ts: Some("10.01".to_string()),
                bot_id: None,
                app_id: None,
                files: None,
                channel_type: Some("channel".to_string()),
                event_ts: None,
            }),
            event_id: Some("Ev_unengaged_reply".to_string()),
            event_time: None,
        };

        assert!(!should_enqueue_slack_message(&wrapper, Some("B1")));
    }

    #[test]
    fn create_workspace_brief_request_parses_full_payload() {
        let json = r#"{
            "founder_name": "Dylan Tang",
            "founder_email": "dylan@example.com",
            "venture_name": "Acme Labs",
            "thesis": "Building AI tools for productivity",
            "stage": "mvp",
            "goals": ["Launch MVP", "Get 3 pilot customers", "Raise seed round"],
            "current_assets": ["Landing page", "Figma mockups"],
            "plan_horizon_days": 60
        }"#;

        let request: CreateWorkspaceBriefRequest =
            serde_json::from_str(json).expect("should parse");

        assert_eq!(request.founder_name, "Dylan Tang");
        assert_eq!(request.founder_email, "dylan@example.com");
        assert_eq!(request.venture_name.as_deref(), Some("Acme Labs"));
        assert_eq!(
            request.thesis.as_deref(),
            Some("Building AI tools for productivity")
        );
        assert_eq!(request.stage.as_deref(), Some("mvp"));
        assert_eq!(request.goals.len(), 3);
        assert_eq!(request.goals[0], "Launch MVP");
        assert_eq!(request.current_assets.as_ref().map(|a| a.len()), Some(2));
        assert_eq!(request.plan_horizon_days, Some(60));
    }

    #[test]
    fn create_workspace_brief_request_parses_minimal_payload() {
        let json = r#"{
            "founder_name": "Jane Doe",
            "founder_email": "jane@example.com",
            "goals": []
        }"#;

        let request: CreateWorkspaceBriefRequest =
            serde_json::from_str(json).expect("should parse");

        assert_eq!(request.founder_name, "Jane Doe");
        assert_eq!(request.founder_email, "jane@example.com");
        assert!(request.venture_name.is_none());
        assert!(request.thesis.is_none());
        assert!(request.stage.is_none());
        assert!(request.goals.is_empty());
        assert!(request.current_assets.is_none());
        assert!(request.plan_horizon_days.is_none());
    }

    #[test]
    fn create_workspace_brief_request_rejects_missing_required_fields() {
        // Missing founder_email
        let json = r#"{"founder_name": "Test", "goals": []}"#;
        let result: Result<CreateWorkspaceBriefRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());

        // Missing founder_name
        let json = r#"{"founder_email": "test@example.com", "goals": []}"#;
        let result: Result<CreateWorkspaceBriefRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());

        // Missing goals
        let json = r#"{"founder_name": "Test", "founder_email": "test@example.com"}"#;
        let result: Result<CreateWorkspaceBriefRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_90_day_plan_request_parses_full_payload() {
        let json = r#"{
            "founder_name": "Dylan Tang",
            "founder_email": "dylan@example.com",
            "venture_name": "Acme Labs",
            "thesis": "Building AI tools for productivity",
            "stage": "mvp",
            "goals": ["Launch MVP", "Get 3 pilot customers", "Raise seed round"],
            "current_assets": ["Landing page", "Figma mockups"],
            "plan_horizon_days": 90
        }"#;

        let request: Create90DayPlanRequest = serde_json::from_str(json).expect("should parse");

        assert_eq!(request.founder_name, "Dylan Tang");
        assert_eq!(request.founder_email, "dylan@example.com");
        assert_eq!(request.venture_name.as_deref(), Some("Acme Labs"));
        assert_eq!(
            request.thesis.as_deref(),
            Some("Building AI tools for productivity")
        );
        assert_eq!(request.stage.as_deref(), Some("mvp"));
        assert_eq!(request.goals.len(), 3);
        assert_eq!(request.goals[0], "Launch MVP");
        assert_eq!(request.current_assets.as_ref().map(|a| a.len()), Some(2));
        assert_eq!(request.plan_horizon_days, Some(90));
    }

    #[test]
    fn create_90_day_plan_request_parses_minimal_payload() {
        let json = r#"{
            "founder_name": "Jane Doe",
            "founder_email": "jane@example.com",
            "goals": []
        }"#;

        let request: Create90DayPlanRequest = serde_json::from_str(json).expect("should parse");

        assert_eq!(request.founder_name, "Jane Doe");
        assert_eq!(request.founder_email, "jane@example.com");
        assert!(request.venture_name.is_none());
        assert!(request.thesis.is_none());
        assert!(request.stage.is_none());
        assert!(request.goals.is_empty());
        assert!(request.current_assets.is_none());
        assert!(request.plan_horizon_days.is_none());
    }

    #[test]
    fn create_90_day_plan_request_rejects_missing_required_fields() {
        let json = r#"{"founder_name": "Test", "goals": []}"#;
        let result: Result<Create90DayPlanRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());

        let json = r#"{"founder_email": "test@example.com", "goals": []}"#;
        let result: Result<Create90DayPlanRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());

        let json = r#"{"founder_name": "Test", "founder_email": "test@example.com"}"#;
        let result: Result<Create90DayPlanRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn build_queue_payload_strips_html_body_for_notion_so_email() {
        let message = InboundMessage {
            channel: Channel::Email,
            sender: "notify@mail.notion.so".to_string(),
            sender_name: Some("Notion".to_string()),
            recipient: "oliver@dowhiz.com".to_string(),
            subject: Some("Someone mentioned you".to_string()),
            text_body: Some("Comment text".to_string()),
            html_body: Some("<html>Large HTML content...</html>".to_string()),
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![],
            reply_to: vec![],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Email, &message);
        assert!(payload.html_body.is_none());
        assert!(payload.text_body.is_some());
    }

    #[test]
    fn build_queue_payload_strips_html_body_for_notion_com_email() {
        let message = InboundMessage {
            channel: Channel::Email,
            sender: "notifications@notion.com".to_string(),
            sender_name: Some("Notion".to_string()),
            recipient: "oliver@dowhiz.com".to_string(),
            subject: Some("Someone mentioned you".to_string()),
            text_body: Some("Comment text".to_string()),
            html_body: Some("<html>Large HTML content...</html>".to_string()),
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![],
            reply_to: vec![],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Email, &message);
        assert!(payload.html_body.is_none());
        assert!(payload.text_body.is_some());
    }

    #[test]
    fn build_queue_payload_strips_html_body_for_non_notion_email() {
        let message = InboundMessage {
            channel: Channel::Email,
            sender: "user@gmail.com".to_string(),
            sender_name: Some("User".to_string()),
            recipient: "oliver@dowhiz.com".to_string(),
            subject: Some("Hello".to_string()),
            text_body: Some("Text content".to_string()),
            html_body: Some("<html>HTML content</html>".to_string()),
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![],
            reply_to: vec![],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Email, &message);
        assert!(payload.html_body.is_none());
        assert_eq!(payload.text_body.as_deref(), Some("Text content"));
    }

    #[test]
    fn build_queue_payload_uses_html_preview_when_text_missing() {
        let message = InboundMessage {
            channel: Channel::Email,
            sender: "user@gmail.com".to_string(),
            sender_name: Some("User".to_string()),
            recipient: "oliver@dowhiz.com".to_string(),
            subject: Some("Hello".to_string()),
            text_body: None,
            html_body: Some("<html><body><p>Hello from html</p></body></html>".to_string()),
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![],
            reply_to: vec![],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Email, &message);
        assert!(payload.html_body.is_none());
        assert_eq!(payload.text_body.as_deref(), Some("Hello from html"));
    }

    #[test]
    fn build_queue_payload_preserves_html_links_when_text_missing() {
        let message = InboundMessage {
            channel: Channel::Email,
            sender: "user@gmail.com".to_string(),
            sender_name: Some("User".to_string()),
            recipient: "oliver@dowhiz.com".to_string(),
            subject: Some("Hello".to_string()),
            text_body: None,
            html_body: Some(
                "<html><body><p>Open <a href=\"https://learning.edx.org/course/123\">the course</a></p><img src=\"data:image/png;base64,AAAA\"></body></html>"
                    .to_string(),
            ),
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![],
            reply_to: vec![],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Email, &message);
        let text = payload.text_body.expect("text preview");
        assert!(payload.html_body.is_none());
        assert!(text.contains("the course (https://learning.edx.org/course/123)"));
        assert!(!text.contains("data:image"));
    }

    #[test]
    fn build_queue_payload_truncates_large_email_text() {
        let long_text = "a".repeat(EMAIL_QUEUE_TEXT_PREVIEW_MAX_BYTES + 512);
        let message = InboundMessage {
            channel: Channel::Email,
            sender: "user@gmail.com".to_string(),
            sender_name: Some("User".to_string()),
            recipient: "oliver@dowhiz.com".to_string(),
            subject: Some("Hello".to_string()),
            text_body: Some(long_text),
            html_body: None,
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![],
            reply_to: vec![],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Email, &message);
        let text = payload.text_body.expect("text preview");
        assert!(text.ends_with(EMAIL_QUEUE_TEXT_TRUNCATED_SUFFIX));
        assert!(text.len() <= EMAIL_QUEUE_TEXT_PREVIEW_MAX_BYTES);
    }

    #[test]
    fn build_queue_payload_keeps_html_body_for_non_email_channel() {
        let message = InboundMessage {
            channel: Channel::Slack,
            sender: "notify@mail.notion.so".to_string(),
            sender_name: None,
            recipient: "C456".to_string(),
            subject: None,
            text_body: Some("hello".to_string()),
            html_body: Some("<html>content</html>".to_string()),
            thread_id: "thread-1".to_string(),
            message_id: Some("m1".to_string()),
            attachments: vec![],
            reply_to: vec![],
            raw_payload: br#"{}"#.to_vec(),
            metadata: ChannelMetadata::default(),
        };
        let payload = build_queue_payload(Channel::Slack, &message);
        assert!(payload.html_body.is_some());
    }
}

pub(super) fn build_envelope_blocking(
    route: RouteDecision,
    channel: Channel,
    external_message_id: Option<String>,
    message: &InboundMessage,
    raw_payload: &[u8],
) -> Result<IngestionEnvelope, RawPayloadStoreError> {
    let envelope_id = Uuid::new_v4();
    let received_at = Utc::now();
    let queue_payload = build_queue_payload(channel, message);
    let dedupe_key = build_dedupe_key(
        &route.tenant_id,
        &route.employee_id,
        channel,
        external_message_id.as_deref(),
        raw_payload,
    );
    let stored_payload_bytes = if channel == Channel::Email {
        rewrite_email_payload_attachments_to_blob_refs_blocking(
            envelope_id,
            received_at,
            raw_payload,
        )
    } else {
        raw_payload.to_vec()
    };
    let raw_payload_ref = if raw_payload.is_empty() {
        None
    } else {
        Some(raw_payload_store::upload_raw_payload_blocking(
            envelope_id,
            received_at,
            &stored_payload_bytes,
        )?)
    };
    Ok(IngestionEnvelope {
        envelope_id,
        received_at,
        tenant_id: Some(route.tenant_id),
        employee_id: route.employee_id,
        channel,
        external_message_id,
        dedupe_key,
        payload: queue_payload,
        raw_payload_ref,
        account_id: None,
    })
}
