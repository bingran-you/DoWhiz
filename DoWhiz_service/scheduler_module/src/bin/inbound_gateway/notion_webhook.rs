//! Notion webhook handler for comment.created events.
//!
//! This module handles incoming Notion webhooks, with:
//! - HMAC-SHA256 signature verification
//! - Self-trigger prevention (ignores comments posted by our own bot)
//! - Multi-environment routing (each env has its own integration_id)

use std::env;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, warn};

use scheduler_module::channel::{Channel, ChannelMetadata, InboundMessage};
use scheduler_module::notion_browser::models::NotionMention;
use scheduler_module::notion_store::{NotionCredential, NotionStore, NotionStoreError};

use super::handlers::{build_envelope, enqueue_envelope};
use super::state::{GatewayState, RouteDecision};

/// Notion webhook event types we handle
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotionEventType {
    #[serde(rename = "comment.created")]
    CommentCreated,
    #[serde(other)]
    Unknown,
}

/// Author information in webhook payload
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionWebhookAuthor {
    pub id: String,
    #[serde(rename = "type")]
    pub author_type: String,
}

/// Rich text element in comment
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionRichTextElement {
    #[serde(rename = "type")]
    pub element_type: String,
    pub plain_text: Option<String>,
    pub text: Option<NotionTextContent>,
    pub mention: Option<NotionMentionContent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionTextContent {
    pub content: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionMentionContent {
    #[serde(rename = "type")]
    pub mention_type: Option<String>,
    pub user: Option<NotionMentionUser>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionMentionUser {
    pub id: Option<String>,
    pub name: Option<String>,
}

/// Comment data in webhook payload
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionCommentData {
    pub id: String,
    pub parent: Option<NotionCommentParent>,
    pub created_by: Option<NotionCommentCreatedBy>,
    pub rich_text: Option<Vec<NotionRichTextElement>>,
    pub discussion_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionCommentParent {
    #[serde(rename = "type")]
    pub parent_type: String,
    pub page_id: Option<String>,
    pub block_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionCommentCreatedBy {
    pub id: String,
    #[serde(rename = "type")]
    pub author_type: String,
    pub name: Option<String>,
}

/// Main webhook payload structure
/// Based on actual Notion webhook format (API version 2026-03-11)
/// Uses serde_json::Value for dynamic fields to handle varying payload structures
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionWebhookPayload {
    /// Event ID
    pub id: String,
    /// Event type (e.g., "comment.created") - may not be present in all payloads
    #[serde(rename = "type", default)]
    pub event_type: Option<NotionEventType>,
    /// Timestamp of the event
    #[serde(default)]
    pub timestamp: Option<String>,
    /// The integration that received this webhook.
    pub integration_id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub workspace_name: Option<String>,
    #[serde(default)]
    pub subscription_id: Option<String>,
    /// Authors who triggered this event
    #[serde(default)]
    pub authors: Option<Vec<NotionWebhookAuthor>>,
    /// Users/bots who can access the affected resource
    #[serde(default)]
    pub accessible_by: Option<Vec<NotionWebhookAuthor>>,
    /// Comment data - structure varies, use Value for flexibility
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    /// Entity data - alternative location for comment data
    #[serde(default)]
    pub entity: Option<serde_json::Value>,
    /// Page reference
    #[serde(default)]
    pub page_id: Option<String>,
    /// Discussion/thread reference
    #[serde(default)]
    pub discussion_id: Option<String>,
    /// Comment reference
    #[serde(default)]
    pub comment_id: Option<String>,
    /// Catch all other fields we haven't explicitly defined
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl NotionWebhookPayload {
    /// Check if this event was triggered by our own bot posting a comment.
    ///
    /// Returns true if any author is a bot with the same ID as the integration_id,
    /// meaning our integration posted this comment and we should ignore it.
    pub fn is_self_triggered(&self) -> bool {
        let Some(authors) = &self.authors else {
            return false;
        };

        for author in authors {
            if author.author_type == "bot" && author.id == self.integration_id {
                return true;
            }
        }

        false
    }

    /// Check if the given bot_id is among the authors.
    ///
    /// This is used for self-trigger prevention when we have the workspace-specific
    /// bot_id from the NotionCredential, which is different from the public integration_id.
    pub fn is_author_bot(&self, bot_id: &str) -> bool {
        // Check in authors array
        if let Some(authors) = &self.authors {
            for author in authors {
                if author.id == bot_id {
                    return true;
                }
            }
        }

        // Also check created_by in the data
        if let Some(author_id) = self.author_id() {
            if author_id == bot_id {
                return true;
            }
        }

        false
    }

    /// Get the comment/entity data as a JSON Value
    fn comment_data_value(&self) -> Option<&serde_json::Value> {
        self.data
            .as_ref()
            .or(self.entity.as_ref())
            .or_else(|| self.extra.get("comment"))
            .or_else(|| self.extra.get("block"))
    }

    /// Extract the plain text content from the comment's rich_text array.
    pub fn extract_comment_text(&self) -> String {
        let Some(data) = self.comment_data_value() else {
            return String::new();
        };

        // Try to get rich_text array
        let rich_text = data.get("rich_text").and_then(|v| v.as_array());
        let Some(rich_text) = rich_text else {
            return String::new();
        };

        rich_text
            .iter()
            .filter_map(|elem| {
                elem.get("plain_text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        elem.get("text")
                            .and_then(|t| t.get("content"))
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string())
                    })
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Get the page ID - check multiple possible locations
    pub fn get_page_id(&self) -> Option<String> {
        // Direct field
        if let Some(ref id) = self.page_id {
            return Some(id.clone());
        }
        // From data/entity
        if let Some(data) = self.comment_data_value() {
            if let Some(parent) = data.get("parent") {
                if let Some(page_id) = parent.get("page_id").and_then(|v| v.as_str()) {
                    return Some(page_id.to_string());
                }
            }
            if let Some(page_id) = data.get("page_id").and_then(|v| v.as_str()) {
                return Some(page_id.to_string());
            }
        }
        // From extra fields
        self.extra
            .get("page_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Get the comment ID - check multiple possible locations
    pub fn get_comment_id(&self) -> Option<String> {
        // Direct field
        if let Some(ref id) = self.comment_id {
            return Some(id.clone());
        }
        // From data/entity
        if let Some(data) = self.comment_data_value() {
            if let Some(id) = data.get("id").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
        // Newer webhook payloads carry the changed object under `entity`.
        if let Some(entity) = self.entity.as_ref() {
            if let Some(id) = entity.get("id").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
        // From extra fields
        self.extra
            .get("comment_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Get the discussion ID - check multiple possible locations
    pub fn get_discussion_id(&self) -> Option<String> {
        // Direct field
        if let Some(ref id) = self.discussion_id {
            return Some(id.clone());
        }
        // From data/entity
        if let Some(data) = self.comment_data_value() {
            if let Some(id) = data.get("discussion_id").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
        // From extra fields
        self.extra
            .get("discussion_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Get the author name from created_by
    pub fn author_name(&self) -> Option<String> {
        if let Some(data) = self.comment_data_value() {
            if let Some(created_by) = data.get("created_by") {
                if let Some(name) = created_by.get("name").and_then(|v| v.as_str()) {
                    return Some(name.to_string());
                }
            }
        }
        // Fallback: get first author's name if available
        if let Some(authors) = &self.authors {
            if let Some(first) = authors.first() {
                // Authors don't have names in the basic struct, return ID as fallback
                return Some(first.id.clone());
            }
        }
        None
    }

    /// Get the author ID from created_by
    pub fn author_id(&self) -> Option<String> {
        if let Some(data) = self.comment_data_value() {
            if let Some(created_by) = data.get("created_by") {
                if let Some(id) = created_by.get("id").and_then(|v| v.as_str()) {
                    return Some(id.to_string());
                }
            }
        }
        // Fallback: get first author's ID
        if let Some(authors) = &self.authors {
            if let Some(first) = authors.first() {
                return Some(first.id.clone());
            }
        }
        None
    }

    /// Bot IDs that can identify the workspace-specific OAuth credential.
    pub fn credential_lookup_bot_ids(&self) -> Vec<String> {
        let mut bot_ids = Vec::new();

        if let Some(accessible_by) = &self.accessible_by {
            for principal in accessible_by {
                if principal.author_type == "bot" {
                    push_unique(&mut bot_ids, &principal.id);
                }
            }
        }

        if let Some(authors) = &self.authors {
            for author in authors {
                if author.author_type == "bot" {
                    push_unique(&mut bot_ids, &author.id);
                }
            }
        }

        push_unique(&mut bot_ids, &self.integration_id);
        bot_ids
    }

    /// Check if the comment contains an @mention for a specific bot/integration.
    pub fn contains_bot_mention(&self, integration_id: &str) -> bool {
        let Some(data) = self.comment_data_value() else {
            return false;
        };
        let Some(rich_text) = data.get("rich_text").and_then(|v| v.as_array()) else {
            return false;
        };

        for elem in rich_text {
            let elem_type = elem.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if elem_type == "mention" {
                if let Some(mention) = elem.get("mention") {
                    let mention_type = mention.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if mention_type == "user" {
                        if let Some(user) = mention.get("user") {
                            if let Some(user_id) = user.get("id").and_then(|v| v.as_str()) {
                                if user_id == integration_id {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Check if this is a comment.created event
    pub fn is_comment_created(&self) -> bool {
        matches!(self.event_type, Some(NotionEventType::CommentCreated))
    }

    /// Check if any author is a bot (regardless of which bot).
    ///
    /// This prevents cross-environment triggers where one employee's bot reply
    /// triggers another employee's webhook handler.
    pub fn is_from_any_bot(&self) -> bool {
        if let Some(authors) = &self.authors {
            for author in authors {
                if author.author_type == "bot" {
                    return true;
                }
            }
        }
        false
    }
}

fn push_unique(values: &mut Vec<String>, candidate: &str) {
    let candidate = candidate.trim();
    if !candidate.is_empty() && !values.iter().any(|value| value == candidate) {
        values.push(candidate.to_string());
    }
}

trait NotionCredentialLookup {
    fn lookup_by_workspace(&self, workspace_id: &str)
        -> Result<NotionCredential, NotionStoreError>;
    fn lookup_by_bot_id(&self, bot_id: &str) -> Result<NotionCredential, NotionStoreError>;
}

impl NotionCredentialLookup for NotionStore {
    fn lookup_by_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<NotionCredential, NotionStoreError> {
        self.get_credential_by_workspace(workspace_id)
    }

    fn lookup_by_bot_id(&self, bot_id: &str) -> Result<NotionCredential, NotionStoreError> {
        self.get_credential_by_bot_id(bot_id)
    }
}

fn lookup_notion_credential(
    lookup: &impl NotionCredentialLookup,
    payload: &NotionWebhookPayload,
) -> Result<NotionCredential, NotionStoreError> {
    let workspace_id = payload.workspace_id.trim();
    let mut not_found_reasons = Vec::new();

    if !workspace_id.is_empty() {
        match lookup.lookup_by_workspace(workspace_id) {
            Ok(credential) => return Ok(credential),
            Err(NotionStoreError::NotFound(reason)) => {
                not_found_reasons.push(format!("workspace_id={workspace_id}: {reason}"));
            }
            Err(error) => return Err(error),
        }
    } else {
        not_found_reasons.push("workspace_id missing or empty".to_string());
    }

    for bot_id in payload.credential_lookup_bot_ids() {
        match lookup.lookup_by_bot_id(&bot_id) {
            Ok(credential) => {
                info!(
                    "notion webhook credential matched by bot_id fallback: bot_id={} workspace_id={} original_workspace_id={:?}",
                    bot_id,
                    credential.workspace_id,
                    workspace_id
                );
                return Ok(credential);
            }
            Err(NotionStoreError::NotFound(reason)) => {
                not_found_reasons.push(format!("bot_id={bot_id}: {reason}"));
            }
            Err(error) => return Err(error),
        }
    }

    Err(NotionStoreError::NotFound(not_found_reasons.join("; ")))
}

/// Check if this webhook is for our environment's integration.
///
/// Compares payload.integration_id with NOTION_INTEGRATION_ID env var.
/// If no match, this event is for another environment (staging vs prod).
fn is_my_integration(payload: &NotionWebhookPayload) -> bool {
    let my_integration_id = env::var("NOTION_INTEGRATION_ID")
        .ok()
        .filter(|v| !v.trim().is_empty());

    match my_integration_id {
        Some(my_id) => payload.integration_id == my_id,
        // If not configured, accept all (for backwards compatibility during rollout)
        None => {
            debug!(
                "NOTION_INTEGRATION_ID not configured, accepting webhook for integration_id={}",
                payload.integration_id
            );
            true
        }
    }
}

/// Handle incoming Notion webhook POST request.
pub async fn ingest_notion_webhook(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Check for verification token (webhook setup handshake)
    // Notion sends {"verification_token": "<token>"} and expects it echoed back
    if let Ok(verification) = serde_json::from_slice::<serde_json::Value>(&body) {
        if let Some(token) = verification
            .get("verification_token")
            .and_then(|v| v.as_str())
        {
            info!("notion webhook verification token: {}", token);
            return (StatusCode::OK, Json(json!({"verification_token": token})));
        }
    }

    // Verify signature
    if let Err(reason) = super::verify::verify_notion(&headers, &body) {
        warn!("notion webhook signature verification failed: {}", reason);
        return (StatusCode::UNAUTHORIZED, Json(json!({"status": reason})));
    }

    // Parse payload
    let payload: NotionWebhookPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            let body_preview = String::from_utf8_lossy(&body[..body.len().min(500)]);
            warn!(
                "notion webhook failed to parse payload: {} - body preview: {}",
                e, body_preview
            );
            return (StatusCode::BAD_REQUEST, Json(json!({"status": "bad_json"})));
        }
    };

    info!(
        "notion webhook received: type={:?} integration_id={} workspace_id={}",
        payload.event_type, payload.integration_id, payload.workspace_id
    );

    // Only handle comment.created events
    if !payload.is_comment_created() {
        debug!(
            "notion webhook ignoring event type: {:?}",
            payload.event_type
        );
        return (
            StatusCode::OK,
            Json(json!({"status": "ignored", "reason": "unsupported_event_type"})),
        );
    }

    // Check if this webhook is for our environment
    if !is_my_integration(&payload) {
        info!(
            "notion webhook not for our integration: payload.integration_id={} (ignoring)",
            payload.integration_id
        );
        return (
            StatusCode::OK,
            Json(json!({"status": "ignored", "reason": "not_my_integration"})),
        );
    }

    // Look up credentials first (needed for self-trigger check using bot_id)
    let notion_store = match NotionStore::new() {
        Ok(store) => store,
        Err(e) => {
            warn!("notion webhook failed to create NotionStore: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "store_error"})),
            );
        }
    };

    // Look up credential by workspace_id first. Some real deliveries have
    // arrived with an empty workspace_id, so fall back to the bot principal IDs
    // Notion includes for public integration webhooks.
    let credential = match lookup_notion_credential(&notion_store, &payload) {
        Ok(cred) => cred,
        Err(e) => return notion_credential_lookup_failure_response(&payload.workspace_id, e),
    };
    let resolved_workspace_id = credential.workspace_id.clone();

    // Check for self-trigger using the workspace-specific bot_id from credential
    // The bot_id is the bot's user ID within this workspace, while integration_id is the public integration ID
    if payload.is_author_bot(&credential.bot_id) {
        info!(
            "notion webhook self-triggered: bot_id={} (ignoring)",
            credential.bot_id
        );
        return (
            StatusCode::OK,
            Json(json!({"status": "ignored", "reason": "self_triggered"})),
        );
    }

    // Also filter out comments from ANY bot (not just our own)
    // This prevents cross-environment triggers (e.g., prod Oliver triggering staging Boiled-Egg)
    if payload.is_from_any_bot() {
        info!("notion webhook from bot author (ignoring to prevent cross-env trigger)");
        return (
            StatusCode::OK,
            Json(json!({"status": "ignored", "reason": "bot_author"})),
        );
    }

    // Build routing decision based on employee directory
    let route = resolve_notion_route(&resolved_workspace_id, &credential, &state);
    let Some(route) = route else {
        info!(
            "notion webhook no route for workspace_id={}",
            resolved_workspace_id
        );
        return (StatusCode::OK, Json(json!({"status": "no_route"})));
    };

    // Get employee's Notion user ID for @mention filtering.
    // We check this after fetching comment via API (webhook v2 doesn't include rich_text).
    let required_mention_user_id: Option<String> = state
        .employee_directory
        .employee_by_id
        .get(&route.employee_id)
        .and_then(|e| e.notion_user_id.clone());

    // Extract message details
    let mut comment_text = payload.extract_comment_text();
    let page_id = payload
        .get_page_id()
        .unwrap_or_else(|| "unknown".to_string());
    let comment_id = payload
        .get_comment_id()
        .unwrap_or_else(|| payload.id.clone());
    let discussion_id = payload
        .get_discussion_id()
        .unwrap_or_else(|| comment_id.clone());
    let mut author_name = payload.author_name();
    let author_id = payload.author_id().unwrap_or_else(|| "unknown".to_string());

    // Notion webhook v2 doesn't include comment text in payload - need to fetch via API
    // Must paginate through all comments since pages can have 100+ comments
    // Webhook can arrive before API has the comment, so retry once after delay
    if comment_text.is_empty() && page_id != "unknown" {
        info!(
            "notion webhook comment text empty, fetching via API: page_id={} comment_id={}",
            page_id, comment_id
        );
        let http_client = reqwest::Client::new();
        let max_retries = 2; // Try twice (initial + 1 retry after delay)

        'retry: for attempt in 0..max_retries {
            if attempt > 0 {
                // Wait 1.5s before retry - gives Notion API time to sync
                info!("notion webhook retry attempt {} after delay", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            }

            let mut next_cursor: Option<String> = None;
            let mut total_comments_checked = 0;
            let max_pages = 10; // Safety limit: 10 pages * 100 = 1000 comments max
            let mut pages_fetched = 0;

            'pagination: loop {
                if pages_fetched >= max_pages {
                    warn!(
                        "notion webhook pagination limit reached after {} pages ({} comments)",
                        pages_fetched, total_comments_checked
                    );
                    break 'pagination;
                }

                let mut url = format!("https://api.notion.com/v1/comments?block_id={}", page_id);
                if let Some(ref cursor) = next_cursor {
                    url.push_str(&format!("&start_cursor={}", cursor));
                }

                match http_client
                    .get(&url)
                    .header(
                        "Authorization",
                        format!("Bearer {}", credential.access_token),
                    )
                    .header("Notion-Version", "2022-06-28")
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            if let Ok(data) = resp.json::<serde_json::Value>().await {
                                if let Some(results) = data["results"].as_array() {
                                    total_comments_checked += results.len();
                                    for c in results {
                                        let cid = c["id"].as_str().unwrap_or("");
                                        if cid == comment_id {
                                            // Extract plain text from rich_text array
                                            if let Some(rich_text) = c["rich_text"].as_array() {
                                                // Check if comment @mentions the employee's Notion user (person, not bot)
                                                if let Some(ref user_id) = required_mention_user_id
                                                {
                                                    info!(
                                                        "notion webhook rich_text for mention check: {:?}",
                                                        rich_text
                                                    );
                                                    let has_mention =
                                                        rich_text.iter().any(|elem| {
                                                            elem.get("type")
                                                                .and_then(|t| t.as_str())
                                                                == Some("mention")
                                                                && elem
                                                                    .get("mention")
                                                                    .and_then(|m| m.get("type"))
                                                                    .and_then(|t| t.as_str())
                                                                    == Some("user")
                                                                && elem
                                                                    .get("mention")
                                                                    .and_then(|m| m.get("user"))
                                                                    .and_then(|u| u.get("id"))
                                                                    .and_then(|id| id.as_str())
                                                                    == Some(user_id.as_str())
                                                        });
                                                    if !has_mention {
                                                        info!(
                                                            "notion webhook ignoring: comment does not @mention employee notion_user_id={}",
                                                            user_id
                                                        );
                                                        return (
                                                            StatusCode::OK,
                                                            Json(
                                                                json!({"status": "ignored", "reason": "employee_not_mentioned"}),
                                                            ),
                                                        );
                                                    }
                                                }
                                                comment_text = rich_text
                                                    .iter()
                                                    .filter_map(|rt| rt["plain_text"].as_str())
                                                    .collect::<Vec<_>>()
                                                    .join("");
                                            }
                                            // Extract author name
                                            if author_name.is_none() {
                                                if let Some(name) = c["created_by"]["name"].as_str()
                                                {
                                                    author_name = Some(name.to_string());
                                                }
                                            }
                                            info!(
                                                "notion webhook fetched comment text: {} chars (page {}, attempt {})",
                                                comment_text.len(),
                                                pages_fetched + 1,
                                                attempt + 1
                                            );
                                            break 'retry; // Found it, exit both loops
                                        }
                                    }
                                }
                                // Check for more pages
                                let has_more = data["has_more"].as_bool().unwrap_or(false);
                                if has_more {
                                    next_cursor =
                                        data["next_cursor"].as_str().map(|s| s.to_string());
                                    pages_fetched += 1;
                                } else {
                                    // No more pages - try retry if available
                                    if attempt + 1 < max_retries {
                                        info!(
                                            "notion webhook comment_id={} not found in {} comments, will retry",
                                            comment_id, total_comments_checked
                                        );
                                    } else {
                                        warn!(
                                            "notion webhook comment_id={} not found in {} comments (checked {} pages, {} attempts)",
                                            comment_id, total_comments_checked, pages_fetched + 1, attempt + 1
                                        );
                                    }
                                    break 'pagination;
                                }
                            } else {
                                warn!("notion webhook failed to parse API response as JSON");
                                break 'pagination;
                            }
                        } else {
                            warn!(
                                "notion webhook API returned status {}: {:?}",
                                resp.status(),
                                resp.text().await
                            );
                            break 'pagination;
                        }
                    }
                    Err(e) => {
                        warn!("notion webhook failed to fetch comments via API: {}", e);
                        break 'pagination;
                    }
                }
            }
        }
    }

    // Log payload structure for debugging
    info!(
        "notion webhook payload: id={} extra_keys={:?}",
        payload.id,
        payload.extra.keys().collect::<Vec<_>>()
    );
    info!(
        "notion webhook processing comment: page_id={} comment_id={} author={:?} text_preview={}",
        page_id,
        comment_id,
        author_name,
        comment_text.chars().take(50).collect::<String>()
    );

    // Build InboundMessage
    let thread_id = format!("notion:{}:{}", resolved_workspace_id, discussion_id);
    let message_id = format!("notion-comment-{}", comment_id);

    let message = InboundMessage {
        channel: Channel::Notion,
        sender: author_id.clone(),
        sender_name: author_name.clone(),
        recipient: payload.integration_id.clone(),
        subject: Some(format!("Notion comment on page {}", page_id)),
        text_body: Some(comment_text.clone()),
        html_body: None,
        thread_id,
        message_id: Some(message_id.clone()),
        attachments: Vec::new(),
        reply_to: vec![],
        raw_payload: body.to_vec(),
        metadata: ChannelMetadata {
            notion_page_id: Some(page_id.clone()),
            notion_comment_id: Some(comment_id.clone()),
            notion_workspace_id: Some(resolved_workspace_id.clone()),
            ..Default::default()
        },
    };

    // Convert webhook payload to NotionMention format for worker compatibility
    let notion_mention = NotionMention {
        id: payload.id.clone(),
        workspace_id: resolved_workspace_id.clone(),
        workspace_name: payload
            .workspace_name
            .clone()
            .or_else(|| credential.workspace_name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        page_id: page_id.clone(),
        page_title: format!("Notion Page {}", page_id), // Title not available in webhook
        block_id: None,
        comment_id: Some(comment_id.clone()),
        sender_name: author_name.clone().unwrap_or_else(|| "Unknown".to_string()),
        sender_id: Some(author_id.clone()),
        comment_text: comment_text.clone(),
        thread_context: vec![], // Thread context not available in webhook
        url: format!("https://notion.so/{}", page_id.replace("-", "")),
        detected_at: chrono::Utc::now(),
    };

    let notion_mention_bytes = match serde_json::to_vec(&notion_mention) {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("notion webhook failed to serialize NotionMention: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "serialization_error"})),
            );
        }
    };

    info!(
        "notion webhook building envelope: route={:?} message_id={}",
        route.employee_id, message_id
    );

    // Build and enqueue envelope
    let envelope = match build_envelope(
        route,
        Channel::Notion,
        Some(message_id.clone()),
        &message,
        &notion_mention_bytes,
    )
    .await
    {
        Ok(env) => {
            info!("notion webhook envelope built: id={}", env.envelope_id);
            env
        }
        Err(e) => {
            warn!("notion webhook failed to build envelope: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "envelope_build_error"})),
            );
        }
    };

    info!(
        "notion webhook enqueuing envelope: {}",
        envelope.envelope_id
    );
    let result = enqueue_envelope(state.queue.clone(), envelope).await;
    info!("notion webhook enqueue result: {:?}", result.0);
    result
}

fn notion_credential_lookup_failure_response(
    workspace_id: &str,
    error: NotionStoreError,
) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        NotionStoreError::NotFound(reason) => {
            warn!(
                "notion webhook no credential found for workspace_id={}: {}",
                workspace_id, reason
            );
            (
                StatusCode::OK,
                Json(json!({"status": "ignored", "reason": "no_credential"})),
            )
        }
        error => {
            warn!(
                "notion webhook credential lookup failed for workspace_id={}: {}",
                workspace_id, error
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status": "credential_lookup_error"})),
            )
        }
    }
}

/// Resolve routing for Notion webhook based on workspace/integration mapping.
fn resolve_notion_route(
    workspace_id: &str,
    credential: &scheduler_module::notion_store::NotionCredential,
    state: &GatewayState,
) -> Option<RouteDecision> {
    // Try to find employee by workspace_id in routes
    let route_key = workspace_id.to_string();

    // Check explicit routes first
    if let Some(route) = super::routes::resolve_route(Channel::Notion, &route_key, state) {
        return Some(route);
    }

    // Fallback: use account_id from credential to find associated employee
    // For now, use default employee from config
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

    info!(
        "notion webhook using default route: employee_id={} tenant_id={} account_id={}",
        employee_id, tenant_id, credential.account_id
    );

    Some(RouteDecision {
        tenant_id,
        employee_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::cell::RefCell;
    use std::collections::HashMap;

    fn make_test_payload(authors: Option<Vec<NotionWebhookAuthor>>) -> NotionWebhookPayload {
        NotionWebhookPayload {
            id: "event-1".to_string(),
            event_type: Some(NotionEventType::CommentCreated),
            timestamp: None,
            integration_id: "bot-123".to_string(),
            workspace_id: "ws-456".to_string(),
            workspace_name: None,
            subscription_id: None,
            authors,
            accessible_by: None,
            data: Some(serde_json::json!({
                "id": "comment-1",
                "parent": {"type": "page_id", "page_id": "page-789"},
                "created_by": {"id": "user-111", "type": "person", "name": "Test User"},
                "rich_text": [{"type": "text", "plain_text": "Hello world"}],
                "discussion_id": "disc-222"
            })),
            entity: None,
            page_id: None,
            discussion_id: None,
            comment_id: None,
            extra: std::collections::HashMap::new(),
        }
    }

    fn test_credential(workspace_id: &str, bot_id: &str) -> NotionCredential {
        NotionCredential {
            account_id: uuid::Uuid::nil(),
            workspace_id: workspace_id.to_string(),
            workspace_name: Some("Test Workspace".to_string()),
            access_token: "secret_test_token".to_string(),
            bot_id: bot_id.to_string(),
            owner_user_id: Some("owner-user".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[derive(Default)]
    struct FakeCredentialLookup {
        by_workspace: HashMap<String, NotionCredential>,
        by_bot_id: HashMap<String, NotionCredential>,
        calls: RefCell<Vec<String>>,
    }

    impl FakeCredentialLookup {
        fn with_workspace(mut self, workspace_id: &str, credential: NotionCredential) -> Self {
            self.by_workspace
                .insert(workspace_id.to_string(), credential);
            self
        }

        fn with_bot_id(mut self, bot_id: &str, credential: NotionCredential) -> Self {
            self.by_bot_id.insert(bot_id.to_string(), credential);
            self
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl NotionCredentialLookup for FakeCredentialLookup {
        fn lookup_by_workspace(
            &self,
            workspace_id: &str,
        ) -> Result<NotionCredential, NotionStoreError> {
            self.calls
                .borrow_mut()
                .push(format!("workspace:{workspace_id}"));
            self.by_workspace
                .get(workspace_id)
                .cloned()
                .ok_or_else(|| NotionStoreError::NotFound(workspace_id.to_string()))
        }

        fn lookup_by_bot_id(&self, bot_id: &str) -> Result<NotionCredential, NotionStoreError> {
            self.calls.borrow_mut().push(format!("bot:{bot_id}"));
            self.by_bot_id
                .get(bot_id)
                .cloned()
                .ok_or_else(|| NotionStoreError::NotFound(format!("bot_id: {bot_id}")))
        }
    }

    #[test]
    fn test_is_self_triggered_false_for_person() {
        let payload = make_test_payload(Some(vec![NotionWebhookAuthor {
            id: "user-111".to_string(),
            author_type: "person".to_string(),
        }]));
        assert!(!payload.is_self_triggered());
    }

    #[test]
    fn test_is_self_triggered_true_for_matching_bot() {
        let payload = make_test_payload(Some(vec![NotionWebhookAuthor {
            id: "bot-123".to_string(), // Same as integration_id
            author_type: "bot".to_string(),
        }]));
        assert!(payload.is_self_triggered());
    }

    #[test]
    fn test_is_self_triggered_false_for_different_bot() {
        let payload = make_test_payload(Some(vec![NotionWebhookAuthor {
            id: "other-bot-999".to_string(),
            author_type: "bot".to_string(),
        }]));
        assert!(!payload.is_self_triggered());
    }

    #[test]
    fn test_is_self_triggered_false_for_no_authors() {
        let payload = make_test_payload(None);
        assert!(!payload.is_self_triggered());
    }

    #[test]
    fn test_extract_comment_text() {
        let payload = make_test_payload(None);
        assert_eq!(payload.extract_comment_text(), "Hello world");
    }

    #[test]
    fn test_extract_comment_text_multiple_elements() {
        let mut payload = make_test_payload(None);
        payload.data = Some(serde_json::json!({
            "rich_text": [
                {"type": "text", "plain_text": "Hello "},
                {"type": "mention", "plain_text": "@Bot", "mention": {"type": "user", "user": {"id": "bot-123", "name": "Bot"}}},
                {"type": "text", "plain_text": " please help"}
            ]
        }));
        assert_eq!(payload.extract_comment_text(), "Hello @Bot please help");
    }

    #[test]
    fn test_contains_bot_mention_true() {
        let mut payload = make_test_payload(None);
        payload.data = Some(serde_json::json!({
            "rich_text": [
                {"type": "mention", "plain_text": "@Bot", "mention": {"type": "user", "user": {"id": "bot-123", "name": "Bot"}}}
            ]
        }));
        assert!(payload.contains_bot_mention("bot-123"));
    }

    #[test]
    fn test_contains_bot_mention_false_different_id() {
        let mut payload = make_test_payload(None);
        payload.data = Some(serde_json::json!({
            "rich_text": [
                {"type": "mention", "plain_text": "@User", "mention": {"type": "user", "user": {"id": "user-other", "name": "User"}}}
            ]
        }));
        assert!(!payload.contains_bot_mention("bot-123"));
    }

    #[test]
    fn test_get_page_id() {
        let payload = make_test_payload(None);
        assert_eq!(payload.get_page_id(), Some("page-789".to_string()));
    }

    #[test]
    fn test_get_comment_id() {
        let payload = make_test_payload(None);
        assert_eq!(payload.get_comment_id(), Some("comment-1".to_string()));
    }

    #[test]
    fn test_get_comment_id_from_entity() {
        let mut payload = make_test_payload(None);
        payload.data = Some(serde_json::json!({
            "page_id": "page-789"
        }));
        payload.entity = Some(serde_json::json!({
            "id": "comment-from-entity",
            "type": "comment"
        }));

        assert_eq!(
            payload.get_comment_id(),
            Some("comment-from-entity".to_string())
        );
    }

    #[test]
    fn test_author_name() {
        let payload = make_test_payload(None);
        assert_eq!(payload.author_name(), Some("Test User".to_string()));
    }

    #[test]
    fn credential_lookup_not_found_remains_non_retryable_no_credential() {
        let (status, Json(body)) = notion_credential_lookup_failure_response(
            "ws-456",
            NotionStoreError::NotFound("ws-456".to_string()),
        );

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ignored");
        assert_eq!(body["reason"], "no_credential");
    }

    #[test]
    fn credential_lookup_store_errors_are_retryable_failures() {
        let (status, Json(body)) = notion_credential_lookup_failure_response(
            "ws-456",
            NotionStoreError::MongoConfig("missing MONGODB_URI".to_string()),
        );

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["status"], "credential_lookup_error");
        assert!(body.get("reason").is_none());
    }

    #[test]
    fn credential_lookup_uses_workspace_before_bot_ids() {
        let payload = make_test_payload(None);
        let lookup = FakeCredentialLookup::default()
            .with_workspace("ws-456", test_credential("ws-456", "workspace-bot"))
            .with_bot_id("bot-123", test_credential("other-ws", "bot-123"));

        let credential = lookup_notion_credential(&lookup, &payload).unwrap();

        assert_eq!(credential.workspace_id, "ws-456");
        assert_eq!(lookup.calls(), vec!["workspace:ws-456"]);
    }

    #[test]
    fn credential_lookup_falls_back_to_accessible_bot_when_workspace_empty() {
        let mut payload = make_test_payload(None);
        payload.workspace_id.clear();
        payload.integration_id = "public-integration".to_string();
        payload.accessible_by = Some(vec![
            NotionWebhookAuthor {
                id: "person-1".to_string(),
                author_type: "person".to_string(),
            },
            NotionWebhookAuthor {
                id: "workspace-bot".to_string(),
                author_type: "bot".to_string(),
            },
        ]);
        let lookup = FakeCredentialLookup::default().with_bot_id(
            "workspace-bot",
            test_credential("resolved-ws", "workspace-bot"),
        );

        let credential = lookup_notion_credential(&lookup, &payload).unwrap();

        assert_eq!(credential.workspace_id, "resolved-ws");
        assert_eq!(lookup.calls(), vec!["bot:workspace-bot"]);
    }

    #[test]
    fn credential_lookup_falls_back_to_bot_when_workspace_not_found() {
        let mut payload = make_test_payload(None);
        payload.workspace_id = "missing-ws".to_string();
        payload.accessible_by = Some(vec![NotionWebhookAuthor {
            id: "workspace-bot".to_string(),
            author_type: "bot".to_string(),
        }]);
        let lookup = FakeCredentialLookup::default().with_bot_id(
            "workspace-bot",
            test_credential("resolved-ws", "workspace-bot"),
        );

        let credential = lookup_notion_credential(&lookup, &payload).unwrap();

        assert_eq!(credential.workspace_id, "resolved-ws");
        assert_eq!(
            lookup.calls(),
            vec!["workspace:missing-ws", "bot:workspace-bot"]
        );
    }

    #[test]
    fn test_deserialize_comment_created() {
        let json = r#"{
            "id": "event-123",
            "type": "comment.created",
            "integration_id": "abc-123",
            "workspace_id": "ws-456",
            "authors": [{"id": "user-1", "type": "person"}],
            "data": {
                "id": "comment-1",
                "parent": {"type": "page_id", "page_id": "page-1"},
                "created_by": {"id": "user-1", "type": "person", "name": "Alice"},
                "rich_text": [{"type": "text", "plain_text": "Hello"}]
            }
        }"#;

        let payload: NotionWebhookPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.event_type, Some(NotionEventType::CommentCreated));
        assert_eq!(payload.integration_id, "abc-123");
        assert_eq!(payload.extract_comment_text(), "Hello");
    }

    #[test]
    fn test_deserialize_comment_created_without_workspace_id() {
        let json = r#"{
            "id": "event-123",
            "type": "comment.created",
            "integration_id": "public-integration",
            "accessible_by": [
                {"id": "workspace-bot", "type": "bot"},
                {"id": "person-1", "type": "person"}
            ],
            "entity": {
                "id": "comment-from-entity",
                "type": "comment"
            },
            "data": {
                "page_id": "page-1"
            }
        }"#;

        let payload: NotionWebhookPayload = serde_json::from_str(json).unwrap();

        assert_eq!(payload.workspace_id, "");
        assert_eq!(
            payload.get_comment_id(),
            Some("comment-from-entity".to_string())
        );
        assert_eq!(
            payload.credential_lookup_bot_ids(),
            vec![
                "workspace-bot".to_string(),
                "public-integration".to_string()
            ]
        );
    }

    #[test]
    fn test_deserialize_unknown_event_type() {
        let json = r#"{
            "id": "event-456",
            "type": "page.created",
            "integration_id": "abc",
            "workspace_id": "ws"
        }"#;

        let payload: NotionWebhookPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.event_type, Some(NotionEventType::Unknown));
    }
}
