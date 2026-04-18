use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::account_store::lookup_account_by_channel;
use crate::adapters::bluebubbles::send_quick_bluebubbles_response;
use crate::adapters::google_common::GoogleCommentsClient;
use crate::adapters::telegram::send_quick_telegram_response;
use crate::adapters::wechat::WeChatOutboundAdapter;
use crate::adapters::wechat_mp::WeChatMpOutboundAdapter;
use crate::adapters::whatsapp::send_quick_whatsapp_response;
use crate::blob_store::get_blob_store;
use crate::channel::Channel;
use crate::channel::OutboundMessage;
use crate::google_auth::{GoogleAuth, GoogleAuthConfig};
use crate::memory_diff::{MemoryDiff, SectionChange};
use crate::memory_queue::{global_memory_queue, MemoryWriteRequest};
use crate::message_router::{MessageRouter, RouterDecision};
use crate::slack_store::{
    emit_slack_channel_not_found_alert, SlackRuntimeCredentialSource, SlackStore,
};
use crate::user_store::UserStore;
use uuid::Uuid;

use super::super::config::ServiceConfig;
use super::super::BoxError;
use super::discord_context::build_discord_router_context;
use super::persist_discord_ingest_context;
use super::slack::{build_slack_router_context, persist_slack_ingest_context};

const QUICK_RESPONSE_CLAIMS_DIR: &str = "quick_response_claims";
const QUICK_RESPONSE_INFLIGHT_STALE_SECS: i64 = 5 * 60;
const QUICK_RESPONSE_SENT_TTL_SECS: i64 = 30 * 24 * 60 * 60;
const WECHAT_MP_QUICK_RESPONSE_MAX_BYTES: usize = 1800;
const WECHAT_MP_QUICK_RESPONSE_TRUNCATED_SUFFIX: &str = "\n\n[truncated]";
const WECHAT_MP_QUICK_FALLBACK_RESPONSE: &str =
    "我是 DoWhiz 助手，可以帮你做问答、任务拆解和执行规划。你可以直接告诉我你现在最想解决的问题。";
const WECHAT_MP_PROFILE_QUICK_RESPONSE: &str =
    "我是 DoWhiz 助手，可以帮你快速解答问题、梳理任务、制定执行计划，并持续跟进你的进展。你可以直接告诉我你现在想解决什么。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum QuickResponseClaimStatus {
    InFlight,
    Sent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuickResponseClaimRecord {
    scope_key: String,
    message_id: String,
    status: QuickResponseClaimStatus,
    updated_at_unix_secs: i64,
}

#[derive(Debug)]
struct QuickResponseClaim {
    path: PathBuf,
    scope_dir: PathBuf,
    record: QuickResponseClaimRecord,
}

#[derive(Debug)]
enum QuickResponseClaimResult {
    Acquired(QuickResponseClaim),
    AlreadySent,
    InFlight,
}

#[derive(Debug)]
enum QuickResponseSendGate {
    Untracked,
    Acquired(QuickResponseClaim),
    Suppressed,
}

/// Read memo.md from a user's memory directory (local file)
fn read_user_memo_local(memory_dir: &Path) -> Option<String> {
    let memo_path = memory_dir.join("memo.md");
    std::fs::read_to_string(&memo_path).ok()
}

/// Read memo from Azure Blob Storage (unified account)
fn read_user_memo_blob(runtime: &tokio::runtime::Handle, account_id: Uuid) -> Option<String> {
    let blob_store = get_blob_store()?;
    match runtime.block_on(blob_store.read_memo(account_id)) {
        Ok(content) => Some(content),
        Err(e) => {
            warn!(
                "Failed to read memo from blob for account {}: {}",
                account_id, e
            );
            None
        }
    }
}

/// Read memo - tries blob storage first if account exists, falls back to local
fn read_user_memo(
    runtime: &tokio::runtime::Handle,
    account_id: Option<Uuid>,
    memory_dir: &Path,
) -> Option<String> {
    if let Some(account_id) = account_id {
        if let Some(content) = read_user_memo_blob(runtime, account_id) {
            return Some(content);
        }
    }
    read_user_memo_local(memory_dir)
}

/// Write memory update via queue - uses blob storage if account_id is set
fn write_memory_update(
    account_id: Option<Uuid>,
    user_id: &str,
    memory_dir: &Path,
    update: &str,
) -> Result<(), String> {
    // Create a simple diff that appends to the "Notes" section
    let diff = MemoryDiff {
        changed_sections: std::collections::HashMap::from([(
            "Notes".to_string(),
            SectionChange::Added(vec![update.to_string()]),
        )]),
    };

    let request = MemoryWriteRequest {
        account_id,
        user_id: user_id.to_string(),
        user_memory_dir: memory_dir.to_path_buf(),
        diff,
    };

    global_memory_queue()
        .submit(request)
        .map_err(|e| e.to_string())
}

fn quick_response_claims_root(state_dir: &Path) -> PathBuf {
    state_dir.join(QUICK_RESPONSE_CLAIMS_DIR)
}

fn quick_response_claim_hash(input: &str) -> String {
    format!("{:x}", md5::compute(input.as_bytes()))
}

fn quick_response_scope_dir(state_dir: &Path, scope_key: &str) -> PathBuf {
    quick_response_claims_root(state_dir).join(quick_response_claim_hash(scope_key))
}

fn quick_response_claim_path(state_dir: &Path, scope_key: &str, message_id: &str) -> PathBuf {
    quick_response_scope_dir(state_dir, scope_key)
        .join(format!("{}.json", quick_response_claim_hash(message_id)))
}

fn quick_response_claim_record(
    scope_key: &str,
    message_id: &str,
    status: QuickResponseClaimStatus,
) -> QuickResponseClaimRecord {
    QuickResponseClaimRecord {
        scope_key: scope_key.to_string(),
        message_id: message_id.to_string(),
        status,
        updated_at_unix_secs: now_unix_secs(),
    }
}

fn quick_response_claim_file_age_secs(path: &Path, now_unix_secs: i64) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let modified_unix_secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some(now_unix_secs.saturating_sub(modified_unix_secs))
}

fn quick_response_claim_record_expired(
    record: &QuickResponseClaimRecord,
    now_unix_secs: i64,
) -> bool {
    let age_secs = now_unix_secs.saturating_sub(record.updated_at_unix_secs);
    match record.status {
        QuickResponseClaimStatus::InFlight => age_secs > QUICK_RESPONSE_INFLIGHT_STALE_SECS,
        QuickResponseClaimStatus::Sent => age_secs > QUICK_RESPONSE_SENT_TTL_SECS,
    }
}

fn load_quick_response_claim_record(
    path: &Path,
) -> Result<Option<QuickResponseClaimRecord>, BoxError> {
    match std::fs::read(path) {
        Ok(raw) => Ok(Some(serde_json::from_slice::<QuickResponseClaimRecord>(
            &raw,
        )?)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn try_create_quick_response_claim(
    path: &Path,
    record: &QuickResponseClaimRecord,
) -> Result<bool, BoxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let serialized = serde_json::to_vec_pretty(record)?;
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
    {
        Ok(mut file) => {
            if let Err(err) = file.write_all(&serialized) {
                let _ = std::fs::remove_file(path);
                return Err(err.into());
            }
            Ok(true)
        }
        Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(err) => Err(err.into()),
    }
}

fn write_quick_response_claim_record(
    path: &Path,
    record: &QuickResponseClaimRecord,
) -> Result<(), BoxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let serialized = serde_json::to_vec_pretty(record)?;
    std::fs::write(&tmp, serialized)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn remove_quick_response_claim_file(path: &Path) -> Result<(), BoxError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn prune_quick_response_scope_dir(scope_dir: &Path, now_unix_secs: i64) -> Result<(), BoxError> {
    let entries = match std::fs::read_dir(scope_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warn!(
                    "failed to enumerate quick response scope dir {}: {}",
                    scope_dir.display(),
                    err
                );
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let should_remove = match load_quick_response_claim_record(&path) {
            Ok(Some(record)) => quick_response_claim_record_expired(&record, now_unix_secs),
            Ok(None) => false,
            Err(err) => {
                warn!(
                    "failed to inspect quick response claim {} during prune: {}",
                    path.display(),
                    err
                );
                quick_response_claim_file_age_secs(&path, now_unix_secs)
                    .map(|age_secs| age_secs > QUICK_RESPONSE_INFLIGHT_STALE_SECS)
                    .unwrap_or(false)
            }
        };

        if should_remove {
            remove_quick_response_claim_file(&path)?;
        }
    }

    if let Ok(mut remaining) = std::fs::read_dir(scope_dir) {
        if remaining.next().is_none() {
            let _ = std::fs::remove_dir(scope_dir);
        }
    }

    Ok(())
}

fn claim_quick_response_send(
    state_dir: &Path,
    scope_key: &str,
    message_id: &str,
) -> Result<QuickResponseClaimResult, BoxError> {
    let scope_dir = quick_response_scope_dir(state_dir, scope_key);
    let path = quick_response_claim_path(state_dir, scope_key, message_id);
    let record =
        quick_response_claim_record(scope_key, message_id, QuickResponseClaimStatus::InFlight);

    loop {
        let now = now_unix_secs();
        prune_quick_response_scope_dir(&scope_dir, now)?;

        if try_create_quick_response_claim(&path, &record)? {
            return Ok(QuickResponseClaimResult::Acquired(QuickResponseClaim {
                path,
                scope_dir,
                record,
            }));
        }

        match load_quick_response_claim_record(&path) {
            Ok(Some(existing)) => {
                if quick_response_claim_record_expired(&existing, now) {
                    remove_quick_response_claim_file(&path)?;
                    continue;
                }
                return Ok(match existing.status {
                    QuickResponseClaimStatus::Sent => QuickResponseClaimResult::AlreadySent,
                    QuickResponseClaimStatus::InFlight => QuickResponseClaimResult::InFlight,
                });
            }
            Ok(None) => continue,
            Err(err) => {
                warn!(
                    "failed to read quick response claim {}: {}",
                    path.display(),
                    err
                );
                if quick_response_claim_file_age_secs(&path, now)
                    .map(|age_secs| age_secs > QUICK_RESPONSE_INFLIGHT_STALE_SECS)
                    .unwrap_or(false)
                {
                    remove_quick_response_claim_file(&path)?;
                    continue;
                }
                return Ok(QuickResponseClaimResult::InFlight);
            }
        }
    }
}

fn mark_quick_response_sent(claim: &QuickResponseClaim) -> Result<(), BoxError> {
    let record = quick_response_claim_record(
        &claim.record.scope_key,
        &claim.record.message_id,
        QuickResponseClaimStatus::Sent,
    );
    write_quick_response_claim_record(&claim.path, &record)?;
    prune_quick_response_scope_dir(&claim.scope_dir, record.updated_at_unix_secs)?;
    Ok(())
}

fn release_quick_response_claim(claim: &QuickResponseClaim) -> Result<(), BoxError> {
    remove_quick_response_claim_file(&claim.path)?;
    prune_quick_response_scope_dir(&claim.scope_dir, now_unix_secs())?;
    Ok(())
}

fn start_quick_response_send(
    state_dir: &Path,
    scope_key: Option<&str>,
    message_id: Option<&str>,
    channel_label: &str,
    employee_id: &str,
    sender: &str,
) -> Result<QuickResponseSendGate, BoxError> {
    let (Some(scope_key), Some(message_id)) = (scope_key, message_id) else {
        return Ok(QuickResponseSendGate::Untracked);
    };

    match claim_quick_response_send(state_dir, scope_key, message_id)? {
        QuickResponseClaimResult::Acquired(claim) => Ok(QuickResponseSendGate::Acquired(claim)),
        QuickResponseClaimResult::AlreadySent => {
            info!(
                "{} quick response dedupe hit employee={} sender={} scope={} message_id={}",
                channel_label, employee_id, sender, scope_key, message_id
            );
            Ok(QuickResponseSendGate::Suppressed)
        }
        QuickResponseClaimResult::InFlight => {
            info!(
                "{} quick response send already in flight employee={} sender={} scope={} message_id={}",
                channel_label, employee_id, sender, scope_key, message_id
            );
            Ok(QuickResponseSendGate::Suppressed)
        }
    }
}

fn slack_quick_response_scope_key(message: &crate::channel::InboundMessage) -> Option<String> {
    let channel_id = message.metadata.slack_channel_id.as_deref()?;
    let team = message
        .metadata
        .slack_team_id
        .as_deref()
        .unwrap_or("unknown");
    Some(format!(
        "slack:{}:{}:{}",
        team, channel_id, message.thread_id
    ))
}

fn slack_inbound_message_id(message: &crate::channel::InboundMessage) -> Option<&str> {
    message.message_id.as_deref()
}

fn discord_quick_response_scope_key(message: &crate::channel::InboundMessage) -> Option<String> {
    let channel_id = message.metadata.discord_channel_id?;
    let guild = message
        .metadata
        .discord_guild_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "dm".to_string());
    Some(format!(
        "discord:{}:{}:{}",
        guild, channel_id, message.thread_id
    ))
}

fn discord_inbound_message_id(message: &crate::channel::InboundMessage) -> Option<&str> {
    message
        .message_id
        .as_deref()
        .or(message.metadata.discord_message_id.as_deref())
}

fn lark_quick_response_scope_key(message: &crate::channel::InboundMessage) -> Option<String> {
    let chat_id = message.metadata.lark_chat_id.as_deref()?;
    let tenant_key = message
        .metadata
        .lark_tenant_key
        .as_deref()
        .unwrap_or("unknown");
    Some(format!(
        "lark:{}:{}:{}",
        tenant_key, chat_id, message.thread_id
    ))
}

fn lark_inbound_message_id(message: &crate::channel::InboundMessage) -> Option<&str> {
    message
        .message_id
        .as_deref()
        .or(message.metadata.lark_message_id.as_deref())
}

fn wechat_quick_response_scope_key(message: &crate::channel::InboundMessage) -> Option<String> {
    let corp_id = message.metadata.wechat_corp_id.as_deref()?;
    Some(format!("wechat:{}:{}", corp_id, message.thread_id))
}

fn wechat_inbound_message_id(message: &crate::channel::InboundMessage) -> Option<&str> {
    message.message_id.as_deref()
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn try_quick_response_slack(
    config: &ServiceConfig,
    user_store: &UserStore,
    slack_store: &SlackStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };
    let channel_id = match message.metadata.slack_channel_id.as_deref() {
        Some(value) => value,
        None => return Ok(false),
    };

    // Look up unified account first, fall back to legacy user_store
    let account_id = lookup_account_by_channel(&Channel::Slack, &message.sender);
    let user = user_store.get_or_create_user("slack", &message.sender)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);
    let dedupe_scope = slack_quick_response_scope_key(message);
    let inbound_message_id = slack_inbound_message_id(message);

    let cleaned_text = text
        .split_whitespace()
        .filter(|word| !(word.starts_with("<@") && word.ends_with(">")))
        .collect::<Vec<_>>()
        .join(" ");
    let slack_context = match build_slack_router_context(config, user_store, message) {
        Ok(context) => context,
        Err(err) => {
            warn!("Failed to build Slack router context: {}", err);
            None
        }
    };

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision = runtime.block_on(message_router.classify(
        &cleaned_text,
        memory.as_deref(),
        employee_name,
        slack_context.as_deref(),
    ));
    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            let quick_response_claim = match start_quick_response_send(
                &user_paths.state_dir,
                dedupe_scope.as_deref(),
                inbound_message_id,
                "slack",
                &config.employee_profile.id,
                &message.sender,
            )? {
                QuickResponseSendGate::Untracked => None,
                QuickResponseSendGate::Acquired(claim) => Some(claim),
                QuickResponseSendGate::Suppressed => return Ok(true),
            };

            // Write memory update if present (to blob storage if account linked)
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                } else if let Some(aid) = account_id {
                    info!("Updated memory for unified account {}", aid);
                } else {
                    info!("Updated memory for legacy user {}", user.user_id);
                }
            }

            let token = resolve_slack_bot_token(
                config,
                slack_store,
                message.metadata.slack_team_id.as_deref(),
            );
            let sent = if let Some(token) = token {
                let thread_ts = Some(message.thread_id.as_str());
                runtime
                    .block_on(send_quick_slack_response(
                        &token,
                        channel_id,
                        message.metadata.slack_team_id.as_deref(),
                        Some(&config.employee_profile.id),
                        thread_ts,
                        &response,
                    ))
                    .is_ok()
            } else {
                false
            };

            if sent {
                if let Some(claim) = quick_response_claim.as_ref() {
                    if let Err(err) = mark_quick_response_sent(claim) {
                        warn!(
                            "failed to mark slack quick response claim sent scope={} message_id={}: {}",
                            claim.record.scope_key, claim.record.message_id, err
                        );
                    }
                }
                if let Err(err) =
                    persist_slack_ingest_context(config, user_store, message, &message.raw_payload)
                {
                    warn!("Failed to persist Slack context after quick reply: {}", err);
                }
                return Ok(true);
            }

            if let Some(claim) = quick_response_claim.as_ref() {
                if let Err(err) = release_quick_response_claim(claim) {
                    warn!(
                        "failed to release slack quick response claim scope={} message_id={}: {}",
                        claim.record.scope_key, claim.record.message_id, err
                    );
                }
            }
            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

fn resolve_slack_bot_token(
    config: &ServiceConfig,
    slack_store: &SlackStore,
    team_id: Option<&str>,
) -> Option<String> {
    if let Some(resolved) = slack_store
        .resolve_installation_for_runtime_details(team_id, Some(&config.employee_profile.id))
    {
        if !resolved.installation.bot_token.trim().is_empty() {
            match resolved.source {
                SlackRuntimeCredentialSource::StoredInstallation => info!(
                    "quick response using Slack installation for employee {} team {}",
                    config.employee_profile.id, resolved.installation.team_id
                ),
                SlackRuntimeCredentialSource::EmployeeEnv => info!(
                    "quick response using employee Slack env credentials for employee {} team {}",
                    config.employee_profile.id, resolved.installation.team_id
                ),
                SlackRuntimeCredentialSource::GlobalEnv => info!(
                    "quick response using global Slack env credentials for employee {}",
                    config.employee_profile.id
                ),
            }
            return Some(resolved.installation.bot_token);
        }
    }
    None
}

fn resolve_discord_bot_token(config: &ServiceConfig) -> Option<String> {
    // First, try per-employee token (e.g., LITTLE_BEAR_DISCORD_BOT_TOKEN)
    let emp_upper = config.employee_profile.id.to_uppercase().replace('-', "_");
    let emp_token_key = format!("{}_DISCORD_BOT_TOKEN", emp_upper);
    if let Ok(token) = std::env::var(&emp_token_key) {
        if !token.trim().is_empty() {
            info!(
                "quick response using {} for employee {}",
                emp_token_key, config.employee_profile.id
            );
            return Some(token);
        }
    }

    // Fall back to global DISCORD_BOT_TOKEN or config
    config.discord_bot_token.clone()
}

pub(crate) fn try_quick_response_bluebubbles(
    config: &ServiceConfig,
    user_store: &UserStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };
    let Some(chat_guid) = message.metadata.bluebubbles_chat_guid.as_deref() else {
        return Ok(false);
    };
    let Some(url) = config.bluebubbles_url.as_deref() else {
        return Ok(false);
    };
    let Some(password) = config.bluebubbles_password.as_deref() else {
        return Ok(false);
    };

    // Normalize phone number (strip + prefix) for lookup
    let normalized_phone = message.sender.trim_start_matches('+');

    // Look up unified account first, fall back to legacy user_store
    let account_id = lookup_account_by_channel(&Channel::BlueBubbles, normalized_phone);
    let user = user_store.get_or_create_user("phone", normalized_phone)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision =
        runtime.block_on(message_router.classify(text, memory.as_deref(), employee_name, None));
    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            // Write memory update if present (to blob storage if account linked)
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                } else if let Some(aid) = account_id {
                    info!("Updated memory for unified account {}", aid);
                } else {
                    info!("Updated memory for legacy user {}", user.user_id);
                }
            }

            if runtime
                .block_on(send_quick_bluebubbles_response(
                    url, password, chat_guid, &response,
                ))
                .is_ok()
            {
                return Ok(true);
            }
            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

pub(crate) fn try_quick_response_discord(
    config: &ServiceConfig,
    user_store: &UserStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };
    if text.trim().is_empty() && !message.attachments.is_empty() {
        return Ok(false);
    }
    let channel_id = match message.metadata.discord_channel_id {
        Some(value) => value,
        None => return Ok(false),
    };
    let message_id = message.message_id.as_deref();
    let Some(token) = resolve_discord_bot_token(config) else {
        return Ok(false);
    };

    // Look up unified account first, fall back to legacy user_store
    let account_id = lookup_account_by_channel(&Channel::Discord, &message.sender);
    let user = user_store.get_or_create_user("discord", &message.sender)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);
    let dedupe_scope = discord_quick_response_scope_key(message);
    let inbound_message_id = discord_inbound_message_id(message);

    let router_context = match build_discord_router_context(config, message, raw_payload) {
        Ok(context) => Some(context),
        Err(err) => {
            warn!("Failed to build Discord router context: {}", err);
            None
        }
    };
    let router_message = router_context
        .as_ref()
        .map(|context| context.message.as_str())
        .unwrap_or(text);
    let extra_context = router_context
        .as_ref()
        .map(|context| context.context.as_str());

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision = runtime.block_on(message_router.classify(
        router_message,
        memory.as_deref(),
        employee_name,
        extra_context,
    ));
    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            let quick_response_claim = match start_quick_response_send(
                &user_paths.state_dir,
                dedupe_scope.as_deref(),
                inbound_message_id,
                "discord",
                &config.employee_profile.id,
                &message.sender,
            )? {
                QuickResponseSendGate::Untracked => None,
                QuickResponseSendGate::Acquired(claim) => Some(claim),
                QuickResponseSendGate::Suppressed => return Ok(true),
            };

            // Write memory update if present (to blob storage if account linked)
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                } else if let Some(aid) = account_id {
                    info!("Updated memory for unified account {}", aid);
                } else {
                    info!("Updated memory for legacy user {}", user.user_id);
                }
            }

            let sent =
                send_quick_discord_response_simple(&token, channel_id, message_id, &response)
                    .is_ok();
            if sent {
                if let Some(claim) = quick_response_claim.as_ref() {
                    if let Err(err) = mark_quick_response_sent(claim) {
                        warn!(
                            "failed to mark discord quick response claim sent scope={} message_id={}: {}",
                            claim.record.scope_key, claim.record.message_id, err
                        );
                    }
                }
                if let Err(err) = persist_discord_ingest_context(
                    config,
                    user_store,
                    message,
                    raw_payload,
                    router_context.as_ref().map(|context| &context.snapshot),
                ) {
                    warn!(
                        "Failed to persist Discord context after quick reply: {}",
                        err
                    );
                }
                return Ok(true);
            }
            if let Some(claim) = quick_response_claim.as_ref() {
                if let Err(err) = release_quick_response_claim(claim) {
                    warn!(
                        "failed to release discord quick response claim scope={} message_id={}: {}",
                        claim.record.scope_key, claim.record.message_id, err
                    );
                }
            }
            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

pub(crate) fn try_quick_response_telegram(
    config: &ServiceConfig,
    user_store: &UserStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };
    let Some(chat_id) = message.metadata.telegram_chat_id else {
        return Ok(false);
    };
    let Some(token) = config.telegram_bot_token.as_deref() else {
        return Ok(false);
    };

    // Look up unified account first, fall back to legacy user_store
    let account_id = lookup_account_by_channel(&Channel::Telegram, &message.sender);
    let user = user_store.get_or_create_user("telegram", &message.sender)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision =
        runtime.block_on(message_router.classify(text, memory.as_deref(), employee_name, None));
    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            // Write memory update if present (to blob storage if account linked)
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                } else if let Some(aid) = account_id {
                    info!("Updated memory for unified account {}", aid);
                } else {
                    info!("Updated memory for legacy user {}", user.user_id);
                }
            }

            if runtime
                .block_on(send_quick_telegram_response(token, chat_id, &response))
                .is_ok()
            {
                return Ok(true);
            }
            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

/// Send a quick response via Slack for locally-handled queries (async version).
async fn send_quick_slack_response(
    bot_token: &str,
    channel: &str,
    team_id: Option<&str>,
    employee_id: Option<&str>,
    thread_ts: Option<&str>,
    response_text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let api_base =
        std::env::var("SLACK_API_BASE_URL").unwrap_or_else(|_| "https://slack.com/api".to_string());
    let url = format!("{}/chat.postMessage", api_base.trim_end_matches('/'));

    let mut request = serde_json::json!({
        "channel": channel,
        "text": response_text,
        "mrkdwn": true
    });

    if let Some(ts) = thread_ts {
        request["thread_ts"] = serde_json::Value::String(ts.to_string());
    }

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", bot_token))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if body.contains("channel_not_found") {
            emit_slack_channel_not_found_alert(
                "quick_response_http",
                employee_id,
                team_id,
                Some(channel),
            );
        }
        return Err(format!("Slack API returned {}: {}", status, body).into());
    }

    let api_response: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if api_response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let error = api_response
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("unknown error");
        if error == "channel_not_found" {
            emit_slack_channel_not_found_alert(
                "quick_response_api",
                employee_id,
                team_id,
                Some(channel),
            );
        }
        return Err(format!("Slack API error: {}", error).into());
    }

    Ok(())
}

fn send_quick_discord_response_simple(
    bot_token: &str,
    channel_id: u64,
    message_id: Option<&str>,
    response_text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use crate::adapters::discord::DiscordOutboundAdapter;
    use crate::channel::{Channel, ChannelMetadata, OutboundAdapter, OutboundMessage};

    let adapter = DiscordOutboundAdapter::new(bot_token.to_string());

    let message = OutboundMessage {
        channel: Channel::Discord,
        from: None,
        to: vec![channel_id.to_string()],
        cc: vec![],
        bcc: vec![],
        subject: String::new(),
        text_body: response_text.to_string(),
        html_body: String::new(),
        html_path: None,
        attachments_dir: None,
        thread_id: message_id.map(|value| value.to_string()),
        metadata: ChannelMetadata {
            discord_channel_id: Some(channel_id),
            ..Default::default()
        },
    };

    let result = adapter.send(&message)?;
    if !result.success {
        return Err(result
            .error
            .unwrap_or_else(|| "discord send failed".to_string())
            .into());
    }
    Ok(())
}

pub(crate) fn try_quick_response_whatsapp(
    config: &ServiceConfig,
    user_store: &UserStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };
    let Some(phone_number) = message.metadata.whatsapp_phone_number.as_deref() else {
        return Ok(false);
    };
    let Some(access_token) = config.whatsapp_access_token.as_deref() else {
        return Ok(false);
    };
    let Some(phone_number_id) = config.whatsapp_phone_number_id.as_deref() else {
        return Ok(false);
    };

    // Normalize phone number (strip + prefix) for lookup
    let normalized_phone = message.sender.trim_start_matches('+');

    // Look up unified account first, fall back to legacy user_store
    let account_id = lookup_account_by_channel(&Channel::WhatsApp, normalized_phone);
    let user = user_store.get_or_create_user("whatsapp", normalized_phone)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision =
        runtime.block_on(message_router.classify(text, memory.as_deref(), employee_name, None));
    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            // Write memory update if present (to blob storage if account linked)
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                } else if let Some(aid) = account_id {
                    info!("Updated memory for unified account {}", aid);
                } else {
                    info!("Updated memory for legacy user {}", user.user_id);
                }
            }

            if runtime
                .block_on(send_quick_whatsapp_response(
                    access_token,
                    phone_number_id,
                    phone_number,
                    &response,
                ))
                .is_ok()
            {
                return Ok(true);
            }
            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

/// Try to handle a Google Workspace (Docs/Sheets/Slides) comment with the upstream classifier.
/// Returns Ok(true) if the message was handled, Ok(false) if it should go to the full pipeline.
pub(crate) fn try_quick_response_google_workspace(
    config: &ServiceConfig,
    user_store: &UserStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };

    // Extract file_id and comment_id based on channel
    let (file_id, comment_id) = match message.channel {
        Channel::GoogleDocs => {
            let file_id = message.metadata.google_docs_document_id.as_deref();
            let comment_id = message.metadata.google_docs_comment_id.as_deref();
            (file_id, comment_id)
        }
        Channel::GoogleSheets => {
            let file_id = message.metadata.google_sheets_spreadsheet_id.as_deref();
            let comment_id = message.metadata.google_sheets_comment_id.as_deref();
            (file_id, comment_id)
        }
        Channel::GoogleSlides => {
            let file_id = message.metadata.google_slides_presentation_id.as_deref();
            let comment_id = message.metadata.google_slides_comment_id.as_deref();
            (file_id, comment_id)
        }
        _ => return Ok(false),
    };

    let (file_id, comment_id) = match (file_id, comment_id) {
        (Some(f), Some(c)) => (f, c),
        _ => return Ok(false),
    };

    // Look up user and memory
    let channel_key = match message.channel {
        Channel::GoogleDocs => "gdocs",
        Channel::GoogleSheets => "gsheets",
        Channel::GoogleSlides => "gslides",
        _ => return Ok(false),
    };
    let account_id = lookup_account_by_channel(&message.channel, &message.sender);
    let user = user_store.get_or_create_user(channel_key, &message.sender)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision =
        runtime.block_on(message_router.classify(text, memory.as_deref(), employee_name, None));

    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            // Write memory update if present
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                }
            }

            // Send quick response via Google API
            if send_quick_google_workspace_response(file_id, comment_id, &response).is_ok() {
                info!(
                    "google workspace quick response sent: channel={:?} file_id={} comment_id={}",
                    message.channel, file_id, comment_id
                );
                return Ok(true);
            }
            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

/// Send a quick response to a Google Workspace comment using service account auth.
fn send_quick_google_workspace_response(
    file_id: &str,
    comment_id: &str,
    response: &str,
) -> Result<(), BoxError> {
    let auth_config = GoogleAuthConfig::from_env();
    let auth = GoogleAuth::new(auth_config)
        .map_err(|e| format!("Failed to initialize Google auth: {}", e))?;

    let client = GoogleCommentsClient::new(
        auth,
        std::collections::HashSet::new(),
        |_| false, // No mention checking needed for quick response
    );

    client
        .reply_to_comment(file_id, comment_id, response)
        .map_err(|e| format!("Failed to reply to comment: {}", e))?;

    Ok(())
}

/// Try to handle a WeChat message with the upstream classifier.
/// Returns Ok(true) if the message was handled, Ok(false) if it should go to the full pipeline.
pub(crate) fn try_quick_response_wechat(
    config: &ServiceConfig,
    user_store: &UserStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };

    // WeChat user ID is the sender
    let user_id = &message.sender;

    // Look up user and memory
    let account_id = lookup_account_by_channel(&Channel::WeChat, user_id);
    let user = user_store.get_or_create_user("wechat", user_id)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);
    let dedupe_scope = wechat_quick_response_scope_key(message);
    let inbound_message_id = wechat_inbound_message_id(message);

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision =
        runtime.block_on(message_router.classify(text, memory.as_deref(), employee_name, None));

    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            let quick_response_claim = match start_quick_response_send(
                &user_paths.state_dir,
                dedupe_scope.as_deref(),
                inbound_message_id,
                "wechat",
                &config.employee_profile.id,
                user_id,
            )? {
                QuickResponseSendGate::Untracked => None,
                QuickResponseSendGate::Acquired(claim) => Some(claim),
                QuickResponseSendGate::Suppressed => return Ok(true),
            };

            // Write memory update if present
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                }
            }

            // Send quick response via WeChat API
            if send_quick_wechat_response(user_id, &response).is_ok() {
                if let Some(claim) = quick_response_claim.as_ref() {
                    if let Err(err) = mark_quick_response_sent(claim) {
                        warn!(
                            "failed to mark wechat quick response claim sent scope={} message_id={}: {}",
                            claim.record.scope_key, claim.record.message_id, err
                        );
                    }
                }
                info!("wechat quick response sent: user_id={}", user_id);
                return Ok(true);
            }
            if let Some(claim) = quick_response_claim.as_ref() {
                if let Err(err) = release_quick_response_claim(claim) {
                    warn!(
                        "failed to release wechat quick response claim scope={} message_id={}: {}",
                        claim.record.scope_key, claim.record.message_id, err
                    );
                }
            }
            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

/// Send a quick response to a WeChat user.
fn send_quick_wechat_response(user_id: &str, response: &str) -> Result<(), BoxError> {
    use crate::channel::{ChannelMetadata, OutboundAdapter};

    let adapter = WeChatOutboundAdapter::from_env()
        .map_err(|e| format!("Failed to create WeChat adapter: {}", e))?;

    let message = OutboundMessage {
        channel: Channel::WeChat,
        from: None, // WeChat uses agent_id from env, not from message
        to: vec![user_id.to_string()],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: String::new(),
        text_body: response.to_string(),
        html_body: String::new(),
        html_path: None,
        attachments_dir: None,
        thread_id: None,
        metadata: ChannelMetadata::default(),
    };

    let result = adapter
        .send(&message)
        .map_err(|e| format!("Failed to send WeChat message: {}", e))?;

    if !result.success {
        return Err(format!("WeChat send failed: {:?}", result.error).into());
    }

    Ok(())
}

/// Try to handle a WeChat Official Account message with the upstream classifier.
/// Returns Ok(true) if the message was handled, Ok(false) if it should go to the full pipeline.
pub(crate) fn try_quick_response_wechat_mp(
    config: &ServiceConfig,
    user_store: &UserStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };

    let open_id = &message.sender;
    if let Some(canned_response) = wechat_mp_profile_quick_response(text) {
        let normalized_response = normalize_wechat_mp_quick_response(canned_response);
        match send_quick_wechat_mp_response(open_id, &normalized_response) {
            Ok(()) => {
                info!(
                    "wechat_mp canned quick response sent: open_id={} bytes={}",
                    open_id,
                    normalized_response.len()
                );
                return Ok(true);
            }
            Err(err) => {
                warn!(
                    "wechat_mp canned quick response failed: open_id={}, error={}",
                    open_id, err
                );
            }
        }
    }

    let account_id = lookup_account_by_channel(&Channel::WeChatMp, open_id);
    let user = user_store.get_or_create_user("wechat_mp", open_id)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision =
        runtime.block_on(message_router.classify(text, memory.as_deref(), employee_name, None));

    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                }
            }

            let normalized_response = normalize_wechat_mp_quick_response(&response);
            match send_quick_wechat_mp_response(open_id, &normalized_response) {
                Ok(()) => {
                    info!(
                        "wechat_mp quick response sent: open_id={} bytes={}",
                        open_id,
                        normalized_response.len()
                    );
                    return Ok(true);
                }
                Err(err) => {
                    warn!(
                        "wechat_mp quick response failed, falling back to canned response: open_id={}, error={}",
                        open_id, err
                    );
                }
            }

            if normalized_response != WECHAT_MP_QUICK_FALLBACK_RESPONSE {
                match send_quick_wechat_mp_response(open_id, WECHAT_MP_QUICK_FALLBACK_RESPONSE) {
                    Ok(()) => {
                        info!(
                            "wechat_mp fallback quick response sent: open_id={}",
                            open_id
                        );
                        return Ok(true);
                    }
                    Err(err) => {
                        warn!(
                            "wechat_mp fallback quick response failed: open_id={}, error={}",
                            open_id, err
                        );
                    }
                }
            }

            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

fn wechat_mp_profile_quick_response(text: &str) -> Option<&'static str> {
    let normalized = text
        .trim()
        .chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(*ch, '，' | ',' | '。' | '.' | '?' | '？'))
        .collect::<String>();
    if normalized.contains("你是谁")
        || normalized.contains("你能做什么")
        || normalized.contains("你会做什么")
        || normalized.contains("你有什么功能")
    {
        return Some(WECHAT_MP_PROFILE_QUICK_RESPONSE);
    }
    None
}

fn normalize_wechat_mp_quick_response(response: &str) -> String {
    let sanitized = response
        .chars()
        .filter(|ch| !ch.is_control() || matches!(*ch, '\n' | '\r' | '\t'))
        .collect::<String>();
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        return WECHAT_MP_QUICK_FALLBACK_RESPONSE.to_string();
    }

    if trimmed.len() <= WECHAT_MP_QUICK_RESPONSE_MAX_BYTES {
        return trimmed.to_string();
    }

    if WECHAT_MP_QUICK_RESPONSE_TRUNCATED_SUFFIX.len() >= WECHAT_MP_QUICK_RESPONSE_MAX_BYTES {
        return truncate_wechat_mp_utf8(trimmed, WECHAT_MP_QUICK_RESPONSE_MAX_BYTES);
    }

    let mut end =
        WECHAT_MP_QUICK_RESPONSE_MAX_BYTES - WECHAT_MP_QUICK_RESPONSE_TRUNCATED_SUFFIX.len();
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    let mut output = trimmed[..end].to_string();
    output.push_str(WECHAT_MP_QUICK_RESPONSE_TRUNCATED_SUFFIX);
    output
}

fn truncate_wechat_mp_utf8(input: &str, max_bytes: usize) -> String {
    let mut end = input.len().min(max_bytes);
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    input[..end].to_string()
}

/// Send a quick response to a WeChat Official Account user.
fn send_quick_wechat_mp_response(open_id: &str, response: &str) -> Result<(), BoxError> {
    use crate::channel::{ChannelMetadata, OutboundAdapter};

    let adapter = WeChatMpOutboundAdapter::from_env()
        .map_err(|e| format!("Failed to create WeChat MP adapter: {}", e))?;

    let message = OutboundMessage {
        channel: Channel::WeChatMp,
        from: None,
        to: vec![open_id.to_string()],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: String::new(),
        text_body: response.to_string(),
        html_body: String::new(),
        html_path: None,
        attachments_dir: None,
        thread_id: None,
        metadata: ChannelMetadata::default(),
    };

    let result = adapter
        .send(&message)
        .map_err(|e| format!("Failed to send WeChat MP message: {}", e))?;

    if !result.success {
        return Err(format!("WeChat MP send failed: {:?}", result.error).into());
    }

    Ok(())
}

/// Try to handle a Lark message with the upstream classifier.
/// Returns Ok(true) if the message was handled, Ok(false) if it should go to the full pipeline.
pub(crate) fn try_quick_response_lark(
    config: &ServiceConfig,
    user_store: &UserStore,
    message_router: &MessageRouter,
    runtime: &tokio::runtime::Handle,
    message: &crate::channel::InboundMessage,
) -> Result<bool, BoxError> {
    let Some(text) = message.text_body.as_deref() else {
        return Ok(false);
    };

    // Lark user ID is the sender (open_id)
    let open_id = &message.sender;

    // Look up unified account first, fall back to legacy user_store
    let account_id = lookup_account_by_channel(&Channel::Lark, open_id);
    let user = user_store.get_or_create_user("lark", open_id)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let memory = read_user_memo(runtime, account_id, &user_paths.memory_dir);
    let dedupe_scope = lark_quick_response_scope_key(message);
    let inbound_message_id = lark_inbound_message_id(message);

    let employee_name = config.employee_profile.display_name.as_deref();
    let decision =
        runtime.block_on(message_router.classify(text, memory.as_deref(), employee_name, None));

    match decision {
        RouterDecision::Simple {
            response,
            memory_update,
        } => {
            let quick_response_claim = match start_quick_response_send(
                &user_paths.state_dir,
                dedupe_scope.as_deref(),
                inbound_message_id,
                "lark",
                &config.employee_profile.id,
                open_id,
            )? {
                QuickResponseSendGate::Untracked => None,
                QuickResponseSendGate::Acquired(claim) => Some(claim),
                QuickResponseSendGate::Suppressed => return Ok(true),
            };

            // Write memory update if present
            if let Some(update) = memory_update {
                if let Err(e) =
                    write_memory_update(account_id, &user.user_id, &user_paths.memory_dir, &update)
                {
                    warn!("Failed to write memory update: {}", e);
                } else if let Some(aid) = account_id {
                    info!("Updated memory for unified account {}", aid);
                } else {
                    info!("Updated memory for legacy user {}", user.user_id);
                }
            }

            // Send quick response via Lark API
            if send_quick_lark_response(open_id, &response).is_ok() {
                if let Some(claim) = quick_response_claim.as_ref() {
                    if let Err(err) = mark_quick_response_sent(claim) {
                        warn!(
                            "failed to mark lark quick response claim sent scope={} message_id={}: {}",
                            claim.record.scope_key, claim.record.message_id, err
                        );
                    }
                }
                info!("lark quick response sent: open_id={}", open_id);
                return Ok(true);
            }
            if let Some(claim) = quick_response_claim.as_ref() {
                if let Err(err) = release_quick_response_claim(claim) {
                    warn!(
                        "failed to release lark quick response claim scope={} message_id={}: {}",
                        claim.record.scope_key, claim.record.message_id, err
                    );
                }
            }
            Ok(false)
        }
        RouterDecision::Complex | RouterDecision::Passthrough => Ok(false),
    }
}

/// Send a quick response to a Lark user.
fn send_quick_lark_response(open_id: &str, response: &str) -> Result<(), BoxError> {
    use crate::adapters::lark::LarkOutboundAdapter;
    use crate::channel::{ChannelMetadata, OutboundAdapter};

    let adapter = LarkOutboundAdapter::from_env()
        .map_err(|e| format!("Failed to create Lark adapter: {}", e))?;

    let message = OutboundMessage {
        channel: Channel::Lark,
        from: None,
        to: vec![open_id.to_string()],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: String::new(),
        text_body: response.to_string(),
        html_body: String::new(),
        html_path: None,
        attachments_dir: None,
        thread_id: None,
        metadata: ChannelMetadata::default(),
    };

    let result = adapter
        .send(&message)
        .map_err(|e| format!("Failed to send Lark message: {}", e))?;

    if !result.success {
        return Err(format!("Lark send failed: {:?}", result.error).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, ChannelMetadata, InboundMessage};
    use tempfile::TempDir;

    fn build_slack_message(
        thread_id: &str,
        message_id: Option<&str>,
        team_id: Option<&str>,
        channel_id: Option<&str>,
    ) -> InboundMessage {
        InboundMessage {
            channel: Channel::Slack,
            sender: "U1234".to_string(),
            sender_name: Some("Bingran".to_string()),
            recipient: "C1234".to_string(),
            subject: None,
            text_body: Some("Hi".to_string()),
            html_body: None,
            thread_id: thread_id.to_string(),
            message_id: message_id.map(str::to_string),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                slack_team_id: team_id.map(str::to_string),
                slack_channel_id: channel_id.map(str::to_string),
                ..Default::default()
            },
        }
    }

    fn build_discord_message(
        thread_id: &str,
        message_id: Option<&str>,
        metadata_message_id: Option<&str>,
        guild_id: Option<u64>,
        channel_id: Option<u64>,
    ) -> InboundMessage {
        InboundMessage {
            channel: Channel::Discord,
            sender: "1234".to_string(),
            sender_name: Some("Bingran".to_string()),
            recipient: "oliver-bot".to_string(),
            subject: None,
            text_body: Some("Hi".to_string()),
            html_body: None,
            thread_id: thread_id.to_string(),
            message_id: message_id.map(str::to_string),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                discord_guild_id: guild_id,
                discord_channel_id: channel_id,
                discord_message_id: metadata_message_id.map(str::to_string),
                ..Default::default()
            },
        }
    }

    fn build_lark_message(
        thread_id: &str,
        message_id: Option<&str>,
        metadata_message_id: Option<&str>,
        tenant_key: Option<&str>,
        chat_id: Option<&str>,
    ) -> InboundMessage {
        InboundMessage {
            channel: Channel::Lark,
            sender: "ou_test_user".to_string(),
            sender_name: Some("Bingran".to_string()),
            recipient: "oc_test_chat".to_string(),
            subject: None,
            text_body: Some("Hi".to_string()),
            html_body: None,
            thread_id: thread_id.to_string(),
            message_id: message_id.map(str::to_string),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                lark_tenant_key: tenant_key.map(str::to_string),
                lark_chat_id: chat_id.map(str::to_string),
                lark_message_id: metadata_message_id.map(str::to_string),
                ..Default::default()
            },
        }
    }

    fn build_wechat_scope_message(
        thread_id: &str,
        message_id: Option<&str>,
        corp_id: Option<&str>,
    ) -> InboundMessage {
        InboundMessage {
            channel: Channel::WeChat,
            sender: "zhangsan".to_string(),
            sender_name: Some("张三".to_string()),
            recipient: "wwcorp".to_string(),
            subject: None,
            text_body: Some("你好".to_string()),
            html_body: None,
            thread_id: thread_id.to_string(),
            message_id: message_id.map(str::to_string),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                wechat_corp_id: corp_id.map(str::to_string),
                ..Default::default()
            },
        }
    }

    fn write_test_quick_response_claim_record(
        path: &Path,
        scope_key: &str,
        message_id: &str,
        status: QuickResponseClaimStatus,
        updated_at_unix_secs: i64,
    ) -> Result<(), BoxError> {
        let record = QuickResponseClaimRecord {
            scope_key: scope_key.to_string(),
            message_id: message_id.to_string(),
            status,
            updated_at_unix_secs,
        };
        write_quick_response_claim_record(path, &record)
    }

    #[test]
    fn quick_response_claim_transitions_from_inflight_to_sent() -> Result<(), BoxError> {
        let temp = TempDir::new()?;
        let state_dir = temp.path().join("state");
        let scope = "discord:guild-1:42:thread-1";
        let message_id = "msg-1";

        let claim = match claim_quick_response_send(&state_dir, scope, message_id)? {
            QuickResponseClaimResult::Acquired(claim) => claim,
            other => panic!("expected acquired claim, got {:?}", other),
        };

        assert!(matches!(
            claim_quick_response_send(&state_dir, scope, message_id)?,
            QuickResponseClaimResult::InFlight
        ));

        mark_quick_response_sent(&claim)?;

        assert!(matches!(
            claim_quick_response_send(&state_dir, scope, message_id)?,
            QuickResponseClaimResult::AlreadySent
        ));

        let path = quick_response_claim_path(&state_dir, scope, message_id);
        let record = load_quick_response_claim_record(&path)?
            .expect("quick response claim record should exist");
        assert_eq!(record.status, QuickResponseClaimStatus::Sent);

        Ok(())
    }

    #[test]
    fn quick_response_claim_release_allows_retry() -> Result<(), BoxError> {
        let temp = TempDir::new()?;
        let state_dir = temp.path().join("state");
        let scope = "discord:guild-1:42:thread-1";
        let message_id = "msg-2";

        let claim = match claim_quick_response_send(&state_dir, scope, message_id)? {
            QuickResponseClaimResult::Acquired(claim) => claim,
            other => panic!("expected acquired claim, got {:?}", other),
        };

        assert!(matches!(
            claim_quick_response_send(&state_dir, scope, message_id)?,
            QuickResponseClaimResult::InFlight
        ));

        release_quick_response_claim(&claim)?;

        assert!(matches!(
            claim_quick_response_send(&state_dir, scope, message_id)?,
            QuickResponseClaimResult::Acquired(_)
        ));

        Ok(())
    }

    #[test]
    fn quick_response_claim_reclaims_expired_records() -> Result<(), BoxError> {
        let temp = TempDir::new()?;
        let state_dir = temp.path().join("state");
        let scope = "slack:T123:C123:thread-1";

        let stale_inflight_path = quick_response_claim_path(&state_dir, scope, "msg-stale");
        write_test_quick_response_claim_record(
            &stale_inflight_path,
            scope,
            "msg-stale",
            QuickResponseClaimStatus::InFlight,
            now_unix_secs() - QUICK_RESPONSE_INFLIGHT_STALE_SECS - 1,
        )?;

        assert!(matches!(
            claim_quick_response_send(&state_dir, scope, "msg-stale")?,
            QuickResponseClaimResult::Acquired(_)
        ));

        let expired_sent_path = quick_response_claim_path(&state_dir, scope, "msg-sent");
        write_test_quick_response_claim_record(
            &expired_sent_path,
            scope,
            "msg-sent",
            QuickResponseClaimStatus::Sent,
            now_unix_secs() - QUICK_RESPONSE_SENT_TTL_SECS - 1,
        )?;

        assert!(matches!(
            claim_quick_response_send(&state_dir, scope, "msg-sent")?,
            QuickResponseClaimResult::Acquired(_)
        ));

        Ok(())
    }

    #[test]
    fn discord_scope_key_and_message_id_fallback_work() {
        let message = build_discord_message("thread-1", None, Some("meta-msg-id"), None, Some(42));

        let scope = discord_quick_response_scope_key(&message).expect("scope key");
        assert_eq!(scope, "discord:dm:42:thread-1");

        let message_id = discord_inbound_message_id(&message).expect("message id");
        assert_eq!(message_id, "meta-msg-id");
    }

    #[test]
    fn slack_scope_key_uses_team_channel_thread() {
        let message = build_slack_message(
            "1712345678.000100",
            Some("1712345678.000100"),
            Some("T123"),
            Some("C123"),
        );

        let scope = slack_quick_response_scope_key(&message).expect("scope key");
        assert_eq!(scope, "slack:T123:C123:1712345678.000100");

        let message_id = slack_inbound_message_id(&message).expect("message id");
        assert_eq!(message_id, "1712345678.000100");
    }

    #[test]
    fn lark_scope_key_and_message_id_fallback_work() {
        let message = build_lark_message(
            "lark:oc_test_chat:ou_test_user",
            None,
            Some("om_message"),
            Some("tenant-1"),
            Some("oc_test_chat"),
        );

        let scope = lark_quick_response_scope_key(&message).expect("scope key");
        assert_eq!(
            scope,
            "lark:tenant-1:oc_test_chat:lark:oc_test_chat:ou_test_user"
        );

        let message_id = lark_inbound_message_id(&message).expect("message id");
        assert_eq!(message_id, "om_message");
    }

    #[test]
    fn wechat_scope_key_and_message_id_work() {
        let message = build_wechat_scope_message(
            "wechat:wwcorp:zhangsan",
            Some("wechat-msg-1"),
            Some("wwcorp"),
        );

        let scope = wechat_quick_response_scope_key(&message).expect("scope key");
        assert_eq!(scope, "wechat:wwcorp:wechat:wwcorp:zhangsan");

        let message_id = wechat_inbound_message_id(&message).expect("message id");
        assert_eq!(message_id, "wechat-msg-1");
    }

    #[test]
    fn normalize_wechat_mp_quick_response_falls_back_on_empty() {
        let response = normalize_wechat_mp_quick_response(" \n\t ");
        assert_eq!(response, WECHAT_MP_QUICK_FALLBACK_RESPONSE);
    }

    #[test]
    fn normalize_wechat_mp_quick_response_removes_control_chars() {
        let response = normalize_wechat_mp_quick_response("abc\u{0}def\u{1f}ghi");
        assert_eq!(response, "abcdefghi");
    }

    #[test]
    fn normalize_wechat_mp_quick_response_truncates_long_output() {
        let long = "x".repeat(WECHAT_MP_QUICK_RESPONSE_MAX_BYTES + 128);
        let response = normalize_wechat_mp_quick_response(&long);
        assert!(response.len() <= WECHAT_MP_QUICK_RESPONSE_MAX_BYTES);
        assert!(response.ends_with(WECHAT_MP_QUICK_RESPONSE_TRUNCATED_SUFFIX));
    }

    #[test]
    fn wechat_mp_profile_quick_response_matches_intro_question() {
        let response = wechat_mp_profile_quick_response("你是谁？你能做什么？");
        assert_eq!(response, Some(WECHAT_MP_PROFILE_QUICK_RESPONSE));
    }

    #[test]
    fn wechat_mp_profile_quick_response_ignores_unrelated_text() {
        let response = wechat_mp_profile_quick_response("今天上海天气怎么样");
        assert_eq!(response, None);
    }

    fn build_google_docs_message(
        document_id: Option<&str>,
        comment_id: Option<&str>,
        text: Option<&str>,
    ) -> InboundMessage {
        InboundMessage {
            channel: Channel::GoogleDocs,
            sender: "user@example.com".to_string(),
            sender_name: Some("Test User".to_string()),
            recipient: "bot@example.com".to_string(),
            subject: None,
            text_body: text.map(str::to_string),
            html_body: None,
            thread_id: "thread-1".to_string(),
            message_id: Some("msg-1".to_string()),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                google_docs_document_id: document_id.map(str::to_string),
                google_docs_comment_id: comment_id.map(str::to_string),
                ..Default::default()
            },
        }
    }

    fn build_google_sheets_message(
        spreadsheet_id: Option<&str>,
        comment_id: Option<&str>,
        text: Option<&str>,
    ) -> InboundMessage {
        InboundMessage {
            channel: Channel::GoogleSheets,
            sender: "user@example.com".to_string(),
            sender_name: Some("Test User".to_string()),
            recipient: "bot@example.com".to_string(),
            subject: None,
            text_body: text.map(str::to_string),
            html_body: None,
            thread_id: "thread-1".to_string(),
            message_id: Some("msg-1".to_string()),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                google_sheets_spreadsheet_id: spreadsheet_id.map(str::to_string),
                google_sheets_comment_id: comment_id.map(str::to_string),
                ..Default::default()
            },
        }
    }

    fn build_google_slides_message(
        presentation_id: Option<&str>,
        comment_id: Option<&str>,
        text: Option<&str>,
    ) -> InboundMessage {
        InboundMessage {
            channel: Channel::GoogleSlides,
            sender: "user@example.com".to_string(),
            sender_name: Some("Test User".to_string()),
            recipient: "bot@example.com".to_string(),
            subject: None,
            text_body: text.map(str::to_string),
            html_body: None,
            thread_id: "thread-1".to_string(),
            message_id: Some("msg-1".to_string()),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                google_slides_presentation_id: presentation_id.map(str::to_string),
                google_slides_comment_id: comment_id.map(str::to_string),
                ..Default::default()
            },
        }
    }

    #[test]
    fn google_docs_message_has_correct_metadata() {
        let message =
            build_google_docs_message(Some("doc-123"), Some("comment-456"), Some("Hello!"));

        assert_eq!(message.channel, Channel::GoogleDocs);
        assert_eq!(
            message.metadata.google_docs_document_id,
            Some("doc-123".to_string())
        );
        assert_eq!(
            message.metadata.google_docs_comment_id,
            Some("comment-456".to_string())
        );
        assert_eq!(message.text_body, Some("Hello!".to_string()));
    }

    #[test]
    fn google_sheets_message_has_correct_metadata() {
        let message = build_google_sheets_message(
            Some("sheet-123"),
            Some("comment-789"),
            Some("Check this cell"),
        );

        assert_eq!(message.channel, Channel::GoogleSheets);
        assert_eq!(
            message.metadata.google_sheets_spreadsheet_id,
            Some("sheet-123".to_string())
        );
        assert_eq!(
            message.metadata.google_sheets_comment_id,
            Some("comment-789".to_string())
        );
    }

    #[test]
    fn google_slides_message_has_correct_metadata() {
        let message = build_google_slides_message(
            Some("slides-123"),
            Some("comment-abc"),
            Some("Nice slide!"),
        );

        assert_eq!(message.channel, Channel::GoogleSlides);
        assert_eq!(
            message.metadata.google_slides_presentation_id,
            Some("slides-123".to_string())
        );
        assert_eq!(
            message.metadata.google_slides_comment_id,
            Some("comment-abc".to_string())
        );
    }

    #[test]
    fn google_workspace_message_without_text_returns_none() {
        let message = build_google_docs_message(
            Some("doc-123"),
            Some("comment-456"),
            None, // No text body
        );

        assert!(message.text_body.is_none());
    }

    #[test]
    fn google_workspace_message_missing_file_id() {
        let message = build_google_docs_message(
            None, // Missing document ID
            Some("comment-456"),
            Some("Hello!"),
        );

        assert!(message.metadata.google_docs_document_id.is_none());
    }

    #[test]
    fn google_workspace_message_missing_comment_id() {
        let message = build_google_docs_message(
            Some("doc-123"),
            None, // Missing comment ID
            Some("Hello!"),
        );

        assert!(message.metadata.google_docs_comment_id.is_none());
    }

    fn build_wechat_message(
        sender: &str,
        text: Option<&str>,
        corp_id: Option<&str>,
    ) -> InboundMessage {
        InboundMessage {
            channel: Channel::WeChat,
            sender: sender.to_string(),
            sender_name: Some("Test User".to_string()),
            recipient: "bot".to_string(),
            subject: None,
            text_body: text.map(str::to_string),
            html_body: None,
            thread_id: format!("wechat:{}:{}", corp_id.unwrap_or("corp"), sender),
            message_id: Some("msg-1".to_string()),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                wechat_corp_id: corp_id.map(str::to_string),
                ..Default::default()
            },
        }
    }

    #[test]
    fn wechat_message_has_correct_metadata() {
        let message = build_wechat_message("user123", Some("Hello!"), Some("corp456"));

        assert_eq!(message.channel, Channel::WeChat);
        assert_eq!(message.sender, "user123");
        assert_eq!(message.text_body, Some("Hello!".to_string()));
        assert_eq!(message.metadata.wechat_corp_id, Some("corp456".to_string()));
    }

    #[test]
    fn wechat_message_without_text_returns_none() {
        let message = build_wechat_message("user123", None, Some("corp456"));

        assert!(message.text_body.is_none());
    }

    #[test]
    fn wechat_message_thread_id_format() {
        let message = build_wechat_message("user123", Some("Hi"), Some("corp456"));

        assert_eq!(message.thread_id, "wechat:corp456:user123");
    }
}
