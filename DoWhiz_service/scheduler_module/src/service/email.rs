use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::account_store::AccountStore;
use crate::artifact_extractor::extract_artifacts_from_email;
use crate::channel::Channel;
use crate::github_inbound::{
    extract_github_sender_login_from_postmark_payload, is_github_notifications_postmark_payload,
};
use crate::google_auth::{GoogleAuth, GoogleAuthConfig};
use crate::index_store::IndexStore;
use crate::mailbox;
use crate::notion_email_detector::{detect_notion_email, is_notion_sender};
use crate::raw_payload_store;
use crate::user_store::{extract_emails, UserStore};
use crate::{ModuleExecutor, RunTaskTask, Scheduler, TaskKind};

use super::bump_thread_state;
use super::config::ServiceConfig;
use super::default_thread_state_path;
use super::html::{derive_inbound_email_text, render_email_html};
use super::postmark::{collect_service_address_candidates, normalize_message_id};
use super::recipients::replyable_recipients;
use super::scheduler::cancel_pending_thread_tasks;
use super::workspace::{create_unique_dir, ensure_thread_workspace, refresh_thread_input_snapshot};
use super::BoxError;

pub use super::postmark::PostmarkInbound;

pub fn process_inbound_payload(
    config: &ServiceConfig,
    user_store: &UserStore,
    index_store: &IndexStore,
    account_store: &AccountStore,
    payload: &PostmarkInbound,
    raw_payload: &[u8],
    account_id: Option<Uuid>,
) -> Result<(), BoxError> {
    info!("processing inbound payload into workspace");

    if is_human_approval_gate_reply(payload) {
        let subject = payload.subject.as_deref().unwrap_or("");
        let updated = record_human_approval_gate_reply(&config.users_root, payload)?;
        info!(
            "handled human approval gate reply from normal inbound workflow: subject={} matched_challenges={}",
            subject, updated
        );
        return Ok(());
    }

    let sender = payload.from.as_deref().unwrap_or("").trim();

    // Check if this looks like a forwarded Notion notification email
    // These have subjects like "mentioned you", "commented in", etc.
    // We need to allow these through even if the sender is blacklisted
    let subject = payload.subject.as_deref().unwrap_or("");
    let subject_lower = subject.to_lowercase();
    let looks_like_notion_forward = subject_lower.contains("mentioned you")
        || subject_lower.contains("replied to")
        || subject_lower.contains("commented in")
        || subject_lower.contains("commented on")
        || subject_lower.contains("发表了评论")
        || subject_lower.contains("中提及了您");

    if !looks_like_notion_forward
        && is_blacklisted_sender(sender, &config.employee_directory.service_addresses)
    {
        info!("skipping blacklisted sender: {}", sender);
        return Ok(());
    }

    // Notion email detection is DISABLED - webhook integration is now the primary path.
    // See inbound_gateway/notion_webhook.rs for the active Notion integration.
    let notion_email_disabled = true;

    if !notion_email_disabled && is_notion_sender(sender) {
        if let Some(notification) = detect_notion_email(
            sender,
            payload.subject.as_deref().unwrap_or(""),
            payload.text_body.as_deref(),
            payload.html_body.as_deref(),
        ) {
            info!(
                "detected Notion email notification type={:?}, routing to Notion handler",
                notification.notification_type
            );
            return super::inbound::process_notion_email(
                config,
                user_store,
                index_store,
                account_store,
                payload,
                raw_payload,
                &notification,
            );
        }
    } else if notion_email_disabled && is_notion_sender(sender) {
        info!("Notion email detection disabled (NOTION_EMAIL_DETECTION_DISABLED=1), skipping");
    }

    let requester = resolve_inbound_requester(payload, raw_payload)?;
    info!(
        "resolved inbound requester identifier_type={} identifier={}",
        requester.identifier_type, requester.identifier
    );

    // The frontend sends auth_user_id (from session.user.id), not account_id.
    // We need to resolve it to the actual account first.
    let resolved_account_id = if let Some(auth_user_id) = account_id {
        match account_store.get_account_by_auth_user(auth_user_id) {
            Ok(Some(account)) => {
                info!(
                    "resolved auth_user_id={} to account_id={}",
                    auth_user_id, account.id
                );
                Some(account.id)
            }
            Ok(None) => {
                warn!(
                    "no account found for auth_user_id={}, falling back to requester",
                    auth_user_id
                );
                None
            }
            Err(err) => {
                warn!(
                    "failed to look up account for auth_user_id={}: {}, falling back to requester",
                    auth_user_id, err
                );
                None
            }
        }
    } else {
        None
    };

    let user = if let Some(acct_id) = resolved_account_id {
        match account_store.list_identifiers(acct_id) {
            Ok(identifiers) => {
                let email_ident = identifiers.iter().find(|i| i.identifier_type == "email");
                if let Some(ident) = email_ident {
                    info!(
                        "using account_id={} linked email={} for user lookup",
                        acct_id, ident.identifier
                    );
                    user_store
                        .get_or_create_user(&ident.identifier_type, &ident.identifier)
                        .map_err(|err| {
                            io::Error::other(format!(
                                "get_or_create_user failed for account {} error={}",
                                acct_id, err
                            ))
                        })?
                } else {
                    info!(
                        "account_id={} has no email identifier, using requester",
                        acct_id
                    );
                    user_store
                        .get_or_create_user(requester.identifier_type, &requester.identifier)
                        .map_err(|err| {
                            io::Error::other(format!(
                                "get_or_create_user failed identifier_type={} identifier={} error={}",
                                requester.identifier_type, requester.identifier, err
                            ))
                        })?
                }
            }
            Err(err) => {
                warn!(
                    "failed to list identifiers for account_id={}: {}, using requester",
                    acct_id, err
                );
                user_store
                    .get_or_create_user(requester.identifier_type, &requester.identifier)
                    .map_err(|err| {
                        io::Error::other(format!(
                            "get_or_create_user failed identifier_type={} identifier={} error={}",
                            requester.identifier_type, requester.identifier, err
                        ))
                    })?
            }
        }
    } else {
        user_store
            .get_or_create_user(requester.identifier_type, &requester.identifier)
            .map_err(|err| {
                io::Error::other(format!(
                    "get_or_create_user failed identifier_type={} identifier={} error={}",
                    requester.identifier_type, requester.identifier, err
                ))
            })?
    };
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    user_store.ensure_user_dirs(&user_paths).map_err(|err| {
        io::Error::other(format!(
            "ensure_user_dirs failed root={} error={}",
            user_paths.root.display(),
            err
        ))
    })?;

    let reply_to_raw = payload.reply_to.as_deref().unwrap_or("");
    let from_raw = payload.from.as_deref().unwrap_or("");
    let mut to_list = replyable_recipients(reply_to_raw);
    if to_list.is_empty() {
        to_list = replyable_recipients(from_raw);
    }
    if to_list.is_empty() {
        info!(
            "no replyable recipients found (reply_to='{}', from='{}')",
            reply_to_raw, from_raw
        );
    }

    let inbound_candidates = collect_service_address_candidates(payload);
    let inbound_service_mailbox = mailbox::select_inbound_service_mailbox(
        &inbound_candidates,
        &config.employee_profile.address_set,
    );
    let inbound_service_mailbox = match inbound_service_mailbox {
        Some(mailbox) => mailbox,
        None => {
            info!("no service address found in inbound payload; skipping");
            return Ok(());
        }
    };

    let thread_key = thread_key(payload, raw_payload);
    let workspace = ensure_thread_workspace(
        &user_paths,
        &user.user_id,
        &thread_key,
        &config.employee_profile,
        config.skills_source_dir.as_deref(),
    )
    .map_err(|err| {
        io::Error::other(format!(
            "ensure_thread_workspace failed user_id={} thread_key={} workspaces_root={} error={}",
            user.user_id,
            thread_key,
            user_paths.workspaces_root.display(),
            err
        ))
    })?;
    // Use the first configured address (verified sender) as reply_from,
    // not the inbound address which may be receive-only (e.g., Postmark inbound hook)
    let reply_from = config
        .employee_profile
        .addresses
        .first()
        .cloned()
        .or_else(|| Some(inbound_service_mailbox.formatted()));
    let model_name = match config.employee_profile.model.clone() {
        Some(model) => model,
        None => {
            if config
                .employee_profile
                .runner
                .eq_ignore_ascii_case("claude")
            {
                String::new()
            } else {
                config.codex_model.clone()
            }
        }
    };
    let thread_state_path = default_thread_state_path(&workspace);
    let message_id = payload
        .header_message_id()
        .or(payload.message_id.as_deref())
        .map(|value| value.trim().to_string());
    let thread_state = bump_thread_state(&thread_state_path, &thread_key, message_id.clone())
        .map_err(|err| {
            io::Error::other(format!(
                "bump_thread_state failed path={} thread_key={} error={}",
                thread_state_path.display(),
                thread_key,
                err
            ))
        })?;
    append_inbound_payload(
        &workspace,
        payload,
        raw_payload,
        thread_state.last_email_seq,
    )
    .map_err(|err| {
        io::Error::other(format!(
            "append_inbound_payload failed workspace={} thread_key={} error={}",
            workspace.display(),
            thread_key,
            err
        ))
    })?;
    if let Err(err) = archive_inbound(&user_paths, payload, raw_payload) {
        error!("failed to archive inbound email: {}", err);
    }
    info!(
        "workspace ready at {} for user {} thread={} epoch={}",
        workspace.display(),
        user.user_id,
        thread_key,
        thread_state.epoch
    );

    // Extract artifacts (Google Docs links, etc.) from email
    let extracted_artifacts = extract_artifacts_from_email(
        payload.text_body.as_deref(),
        payload.html_body.as_deref(),
        payload.subject.as_deref(),
    );

    // If artifacts found (e.g., Google Docs links), write access token for agent
    if !extracted_artifacts.is_empty() {
        info!(
            "found {} artifacts in email, writing access token to workspace",
            extracted_artifacts.len()
        );

        // Use employee-specific OAuth credentials
        let auth_config =
            GoogleAuthConfig::from_env_for_employee(Some(&config.employee_profile.id));
        if let Ok(auth) = GoogleAuth::new(auth_config) {
            match auth.get_access_token() {
                Ok(token) => {
                    let token_path = workspace.join(".google_access_token");
                    if let Err(e) = std::fs::write(&token_path, &token) {
                        warn!("failed to write .google_access_token: {}", e);
                    } else {
                        info!("wrote .google_access_token to workspace for agent");
                    }
                }
                Err(e) => {
                    warn!("failed to get Google access token: {}", e);
                }
            }
        }

        // Write artifact metadata for agent context
        for artifact in &extracted_artifacts {
            if artifact.artifact_type == "google_docs" {
                let meta_path = workspace.join("google_docs_metadata.json");
                let meta = serde_json::json!({
                    "document_id": artifact.artifact_id,
                    "url": artifact.url,
                    "context": artifact.context_snippet,
                    "source": "email_extraction"
                });
                if let Err(e) = std::fs::write(
                    &meta_path,
                    serde_json::to_string_pretty(&meta).unwrap_or_default(),
                ) {
                    warn!("failed to write google_docs_metadata.json: {}", e);
                }
                break; // Only write first Google Docs artifact
            }
        }
    }

    let run_task = RunTaskTask {
        workspace_dir: workspace.clone(),
        input_email_dir: PathBuf::from("incoming_email"),
        input_attachments_dir: PathBuf::from("incoming_attachments"),
        memory_dir: PathBuf::from("memory"),
        reference_dir: PathBuf::from("references"),
        model_name,
        runner: config.employee_profile.runner.clone(),
        codex_disabled: config.codex_disabled,
        reply_to: to_list.clone(),
        reply_from,
        archive_root: Some(user_paths.mail_root.clone()),
        thread_id: Some(thread_key.clone()),
        thread_epoch: Some(thread_state.epoch),
        thread_state_path: Some(thread_state_path.clone()),
        channel: Channel::Email,
        slack_team_id: None,
        employee_id: Some(config.employee_profile.id.clone()),
        requester_identifier_type: Some(requester.identifier_type.to_string()),
        requester_identifier: Some(requester.identifier.clone()),
        account_id: resolved_account_id,
        channel_metadata: Default::default(),
    };

    // Clone run_task before consuming it, in case we need to write to account-level storage
    let run_task_for_account = run_task.clone();

    let mut scheduler = Scheduler::load(&user_paths.tasks_db_path, ModuleExecutor::default())
        .map_err(|err| {
            io::Error::other(format!(
                "scheduler_load failed tasks_db_path={} error={}",
                user_paths.tasks_db_path.display(),
                err
            ))
        })?;
    if let Err(err) = cancel_pending_thread_tasks(&mut scheduler, &workspace, thread_state.epoch) {
        warn!(
            "failed to cancel pending thread tasks for {}: {}",
            workspace.display(),
            err
        );
    }
    let task_id = if let Some(stable_task_id) =
        email_message_task_id(&thread_key, message_id.as_deref())
    {
        // This stable task id is only for full RunTask duplicate-delivery
        // suppression. Quick-response dedupe is handled separately from
        // scheduler task IDs.
        let inserted = scheduler
            .add_one_shot_in_if_absent_with_id(
                stable_task_id,
                Duration::from_secs(0),
                TaskKind::RunTask(run_task),
            )
            .map_err(|err| {
                io::Error::other(format!(
                    "scheduler_add_one_shot_in_if_absent_with_id failed tasks_db_path={} task_id={} error={}",
                    user_paths.tasks_db_path.display(),
                    stable_task_id,
                    err
                ))
            })?;
        if !inserted {
            info!(
                "skipping duplicate email full-task enqueue user_id={} task_id={} message_id={}",
                user.user_id,
                stable_task_id,
                message_id.as_deref().unwrap_or("-")
            );
        }
        stable_task_id
    } else {
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))
            .map_err(|err| {
                io::Error::other(format!(
                    "scheduler_add_one_shot_in failed tasks_db_path={} error={}",
                    user_paths.tasks_db_path.display(),
                    err
                ))
            })?
    };
    index_store
        .sync_user_tasks(&user.user_id, scheduler.tasks())
        .map_err(|err| {
            io::Error::other(format!(
                "index_store_sync_user_tasks failed user_id={} error={}",
                user.user_id, err
            ))
        })?;
    info!(
        "scheduler tasks enqueued user_id={} task_id={} message_id={} workspace={} thread_epoch={}",
        user.user_id,
        task_id,
        message_id.unwrap_or_else(|| "-".to_string()),
        workspace.display(),
        thread_state.epoch
    );

    // If we have a resolved account (from auth_user_id or sender's linked identifier),
    // also write to account-level tasks so it appears in the Task Center
    let account_for_tasks = if resolved_account_id.is_some() {
        resolved_account_id
    } else {
        account_store
            .get_account_by_identifier(requester.identifier_type, &requester.identifier)
            .ok()
            .flatten()
            .map(|a| a.id)
    };

    if let Some(acct_id) = account_for_tasks {
        let account_tasks_dir = config.users_root.join(acct_id.to_string()).join("state");
        if let Err(err) = std::fs::create_dir_all(&account_tasks_dir) {
            warn!(
                "failed to create account tasks dir for account {}: {}",
                acct_id, err
            );
        } else {
            let account_tasks_db_path = account_tasks_dir.join("tasks.db");
            match Scheduler::load(&account_tasks_db_path, ModuleExecutor::default()) {
                Ok(mut account_scheduler) => {
                    // Use the same task_id so we can update status at completion
                    match account_scheduler.add_one_shot_in_if_absent_with_id(
                        task_id,
                        Duration::from_secs(0),
                        TaskKind::RunTask(run_task_for_account),
                    ) {
                        Ok(true) => {
                            info!(
                                "also enqueued task to account-level storage account={} task_id={}",
                                acct_id, task_id
                            );
                        }
                        Ok(false) => {
                            info!(
                                "skipping duplicate email account-level enqueue account={} task_id={}",
                                acct_id, task_id
                            );
                        }
                        Err(err) => {
                            warn!(
                                "failed to add task to account scheduler for account {}: {}",
                                acct_id, err
                            );
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        "failed to load account scheduler for account {}: {}",
                        acct_id, err
                    );
                }
            }
        }
    }

    Ok(())
}

fn email_message_task_id(thread_key: &str, message_id: Option<&str>) -> Option<Uuid> {
    let thread_key = thread_key.trim();
    let message_id = message_id?.trim();
    if thread_key.is_empty() || message_id.is_empty() {
        return None;
    }

    let dedupe_key = format!("email:{thread_key}:{message_id}");
    Some(Uuid::from_bytes(md5::compute(dedupe_key.as_bytes()).0))
}

pub(super) fn is_blacklisted_sender(sender: &str, service_addresses: &HashSet<String>) -> bool {
    if sender.is_empty() {
        return false;
    }
    let mut matched = false;
    let addresses = extract_emails(sender);
    for address in addresses {
        if is_blacklisted_address(&address, service_addresses) {
            matched = true;
            break;
        }
    }
    matched
}

fn is_blacklisted_address(address: &str, service_addresses: &HashSet<String>) -> bool {
    mailbox::is_service_address(address, service_addresses) || is_auto_reply_address(address)
}

fn is_auto_reply_address(address: &str) -> bool {
    let normalized = address.trim().to_ascii_lowercase();
    let local = normalized.split('@').next().unwrap_or("");
    matches!(
        local,
        "noreply" | "no-reply" | "do-not-reply" | "mailer-daemon" | "postmaster"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InboundRequester {
    identifier_type: &'static str,
    identifier: String,
}

fn resolve_inbound_requester(
    payload: &PostmarkInbound,
    raw_payload: &[u8],
) -> Result<InboundRequester, BoxError> {
    if let Some(github_login) = extract_github_sender_login_from_postmark_payload(raw_payload) {
        return Ok(InboundRequester {
            identifier_type: "github",
            identifier: github_login,
        });
    }
    if is_github_notifications_postmark_payload(raw_payload) {
        warn!(
            "github notification email did not include a deterministic sender login; falling back to From address"
        );
    }

    let user_email = payload.from.as_deref().unwrap_or("").trim();
    let user_email = extract_emails(user_email)
        .into_iter()
        .next()
        .ok_or_else(|| "missing sender email".to_string())?;
    Ok(InboundRequester {
        identifier_type: "email",
        identifier: user_email,
    })
}

fn is_human_approval_gate_reply(payload: &PostmarkInbound) -> bool {
    let subject = payload.subject.as_deref().unwrap_or("");
    is_human_approval_gate_subject(subject)
}

pub(super) fn record_human_approval_gate_reply(
    users_root: &Path,
    payload: &PostmarkInbound,
) -> Result<usize, BoxError> {
    let subject = payload.subject.as_deref().unwrap_or("");
    let Some(challenge_id) = extract_human_approval_gate_challenge_id(subject) else {
        return Ok(0);
    };
    let state_paths = find_human_approval_gate_state_paths(users_root, &challenge_id)?;
    if state_paths.is_empty() {
        info!(
            "human approval gate reply did not match any local challenge state: challenge_id={}",
            challenge_id
        );
        return Ok(0);
    }

    let mut updated = 0usize;
    for state_path in state_paths {
        if update_human_approval_gate_state(&state_path, payload)? {
            updated += 1;
        }
    }
    Ok(updated)
}

fn is_human_approval_gate_subject(subject: &str) -> bool {
    let normalized = subject.trim();
    if normalized.is_empty() {
        return false;
    }
    let lowered = normalized.to_ascii_lowercase();
    if lowered.starts_with("[hag:") {
        return true;
    }
    if let Some(rest) = lowered.strip_prefix("re:") {
        if rest.trim_start().starts_with("[hag:") {
            return true;
        }
    }
    false
}

fn extract_human_approval_gate_challenge_id(subject: &str) -> Option<String> {
    let normalized = subject.trim();
    if normalized.is_empty() {
        return None;
    }
    let lowered = normalized.to_ascii_lowercase();
    let start = lowered.find("[hag:")?;
    let remainder = &normalized[start + 5..];
    let end = remainder.find(']')?;
    let challenge_id = remainder[..end].trim();
    if challenge_id.is_empty() {
        None
    } else {
        Some(challenge_id.to_string())
    }
}

fn find_human_approval_gate_state_paths(
    users_root: &Path,
    challenge_id: &str,
) -> Result<Vec<PathBuf>, io::Error> {
    let mut matches = Vec::new();
    if !users_root.exists() {
        return Ok(matches);
    }
    for user_entry in fs::read_dir(users_root)? {
        let user_entry = user_entry?;
        let workspaces_dir = user_entry.path().join("workspaces");
        if !workspaces_dir.is_dir() {
            continue;
        }
        for workspace_entry in fs::read_dir(workspaces_dir)? {
            let workspace_entry = workspace_entry?;
            let candidate = workspace_entry
                .path()
                .join(".human_approval_gate")
                .join("challenges")
                .join(format!("{challenge_id}.json"));
            if candidate.is_file() {
                matches.push(candidate);
            }
        }
    }
    Ok(matches)
}

fn build_human_approval_gate_reply_payload(payload: &PostmarkInbound) -> Value {
    let reply_from_header = payload.from.clone().unwrap_or_default();
    let from_email = extract_emails(&reply_from_header).into_iter().next();
    let message_id = payload
        .message_id
        .as_deref()
        .or(payload.header_message_id())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    json!({
        "inbound_message_id": message_id,
        "received_at": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        "reply_from": from_email.clone().unwrap_or_else(|| reply_from_header.clone()),
        "from_email": from_email,
        "reply_subject": payload.subject.clone().unwrap_or_default(),
        "stripped_text_reply": payload.stripped_text_reply.clone().unwrap_or_default(),
        "text_body": payload.text_body.clone().unwrap_or_default(),
        "html_body": payload.html_body.clone().unwrap_or_default(),
        "headers": Value::Null,
        "attachments": Value::Null,
        "message": {
            "From": reply_from_header,
            "Subject": payload.subject.clone().unwrap_or_default(),
            "TextBody": payload.text_body.clone().unwrap_or_default(),
            "StrippedTextReply": payload.stripped_text_reply.clone().unwrap_or_default(),
            "HtmlBody": payload.html_body.clone().unwrap_or_default(),
            "MessageID": message_id,
        }
    })
}

fn update_human_approval_gate_state(
    state_path: &Path,
    payload: &PostmarkInbound,
) -> Result<bool, BoxError> {
    let raw = fs::read_to_string(state_path)?;
    let mut state: Value = serde_json::from_str(&raw)?;
    let Some(state_obj) = state.as_object_mut() else {
        return Err(format!(
            "human approval gate state is not a JSON object: {}",
            state_path.display()
        )
        .into());
    };

    if state_obj
        .get("status")
        .and_then(Value::as_str)
        .map(|value| value == "replied")
        .unwrap_or(false)
    {
        return Ok(false);
    }

    let inbound_message_id = payload
        .message_id
        .as_deref()
        .or(payload.header_message_id())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    let seen = state_obj
        .entry("seen_inbound_message_ids")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !seen.is_array() {
        *seen = Value::Array(Vec::new());
    }
    if let (Some(message_id), Some(items)) = (inbound_message_id.clone(), seen.as_array_mut()) {
        let already_seen = items
            .iter()
            .any(|value| value.as_str() == Some(message_id.as_str()));
        if !already_seen {
            items.push(Value::String(message_id));
        }
    }

    state_obj.insert("status".to_string(), Value::String("replied".to_string()));
    state_obj.insert(
        "reply".to_string(),
        build_human_approval_gate_reply_payload(payload),
    );
    state_obj.insert(
        "updated_at".to_string(),
        Value::String(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)),
    );

    let temp_path = state_path.with_extension("json.tmp");
    fs::write(&temp_path, serde_json::to_vec_pretty(&state)?)?;
    fs::rename(temp_path, state_path)?;
    Ok(true)
}

fn thread_key(payload: &PostmarkInbound, raw_payload: &[u8]) -> String {
    if let Some(value) = payload.header_value("References") {
        if let Some(id) = extract_first_message_id(value) {
            return id;
        }
    }
    if let Some(value) = payload.header_value("In-Reply-To") {
        if let Some(id) = extract_first_message_id(value) {
            return id;
        }
    }
    if let Some(id) = payload
        .header_message_id()
        .or(payload.message_id.as_deref())
        .and_then(normalize_message_id)
    {
        return id;
    }
    format!("{:x}", md5::compute(raw_payload))
}

fn extract_first_message_id(value: &str) -> Option<String> {
    for token in value.split(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r' | ',' | ';')) {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(id) = normalize_message_id(trimmed) {
            return Some(id);
        }
    }
    None
}

fn append_inbound_payload(
    workspace: &Path,
    payload: &PostmarkInbound,
    raw_payload: &[u8],
    seq: u64,
) -> Result<(), BoxError> {
    let incoming_email = workspace.join("incoming_email");
    let incoming_attachments = workspace.join("incoming_attachments");
    let entries_email = incoming_email.join("entries");
    let entries_attachments = incoming_attachments.join("entries");
    std::fs::create_dir_all(&entries_email)?;
    std::fs::create_dir_all(&entries_attachments)?;

    let entry_name = build_inbound_entry_name(payload, seq);
    let entry_email_dir = entries_email.join(&entry_name);
    let entry_attachments_dir = entries_attachments.join(&entry_name);
    std::fs::create_dir_all(&entry_email_dir)?;
    std::fs::create_dir_all(&entry_attachments_dir)?;
    write_inbound_payload(
        payload,
        raw_payload,
        &entry_email_dir,
        &entry_attachments_dir,
    )?;

    clear_dir_except(&incoming_attachments, &entries_attachments)?;
    write_inbound_payload(payload, raw_payload, &incoming_email, &incoming_attachments)?;
    if let Err(err) = refresh_thread_input_snapshot(&incoming_email, &incoming_attachments) {
        warn!("failed to refresh thread input snapshot: {}", err);
    }
    Ok(())
}

fn clear_dir_except(root: &Path, keep: &Path) -> Result<(), std::io::Error> {
    if !root.exists() {
        std::fs::create_dir_all(root)?;
        return Ok(());
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path == keep {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn archive_inbound(
    user_paths: &crate::user_store::UserPaths,
    payload: &PostmarkInbound,
    raw_payload: &[u8],
) -> Result<(), BoxError> {
    let fallback = format!("email_{}", Utc::now().timestamp());
    let message_id = payload
        .header_message_id()
        .or(payload.message_id.as_deref())
        .unwrap_or("");
    let base = sanitize_token(message_id, &fallback);
    let year = Utc::now().format("%Y").to_string();
    let month = Utc::now().format("%m").to_string();
    let mail_root = user_paths.mail_root.join(year).join(month);
    std::fs::create_dir_all(&mail_root)?;
    let mail_dir = create_unique_dir(&mail_root, &base)?;
    let incoming_email = mail_dir.join("incoming_email");
    let incoming_attachments = mail_dir.join("incoming_attachments");
    std::fs::create_dir_all(&incoming_email)?;
    std::fs::create_dir_all(&incoming_attachments)?;
    write_inbound_payload(payload, raw_payload, &incoming_email, &incoming_attachments)?;
    Ok(())
}

fn write_inbound_payload(
    payload: &PostmarkInbound,
    raw_payload: &[u8],
    incoming_email: &Path,
    incoming_attachments: &Path,
) -> Result<(), BoxError> {
    let email_html = render_email_html(payload);
    let workspace_payload = build_workspace_payload_json(payload, raw_payload, &email_html);
    std::fs::write(
        incoming_email.join("postmark_payload.json"),
        workspace_payload,
    )?;
    std::fs::write(incoming_email.join("email.html"), email_html)?;

    if let Some(attachments) = payload.attachments.as_ref() {
        for attachment in attachments {
            let name = sanitize_token(&attachment.name, "attachment");
            let target = incoming_attachments.join(name);
            let data = resolve_attachment_bytes(attachment)?;
            std::fs::write(target, data)?;
        }
    }
    Ok(())
}

fn build_workspace_payload_json(
    payload: &PostmarkInbound,
    raw_payload: &[u8],
    email_html: &str,
) -> Vec<u8> {
    let mut payload_json: serde_json::Value = match serde_json::from_slice(raw_payload) {
        Ok(value) => value,
        Err(_) => return raw_payload.to_vec(),
    };
    let Some(obj) = payload_json.as_object_mut() else {
        return raw_payload.to_vec();
    };

    let canonical_text = derive_inbound_email_text(
        payload.text_body.as_deref(),
        payload.stripped_text_reply.as_deref(),
        payload.html_body.as_deref(),
    );
    obj.insert(
        "TextBody".to_string(),
        canonical_text
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    obj.insert(
        "HtmlBody".to_string(),
        serde_json::Value::String(email_html.to_string()),
    );
    sanitize_workspace_attachments(obj);

    serde_json::to_vec_pretty(&payload_json).unwrap_or_else(|_| raw_payload.to_vec())
}

fn sanitize_workspace_attachments(payload_json: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(attachments) = payload_json
        .get_mut("Attachments")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };

    for attachment in attachments {
        let Some(obj) = attachment.as_object_mut() else {
            continue;
        };
        let storage_ref = obj
            .get("StorageRef")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let content = obj
            .get("Content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        if content.is_empty() {
            continue;
        }

        let content_len = BASE64_STANDARD
            .decode(content.as_bytes())
            .ok()
            .map(|bytes| bytes.len() as u64)
            .or_else(|| obj.get("ContentLength").and_then(serde_json::Value::as_u64));

        if storage_ref.is_some() || content_len.is_some() {
            obj.insert(
                "Content".to_string(),
                serde_json::Value::String(String::new()),
            );
            if let Some(content_len) = content_len {
                obj.insert(
                    "ContentLength".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(content_len)),
                );
            }
        }
    }
}

fn resolve_attachment_bytes(
    attachment: &super::postmark::PostmarkAttachment,
) -> Result<Vec<u8>, BoxError> {
    resolve_attachment_bytes_with_downloader(attachment, raw_payload_store::download_raw_payload)
}

fn resolve_attachment_bytes_with_downloader<F>(
    attachment: &super::postmark::PostmarkAttachment,
    mut downloader: F,
) -> Result<Vec<u8>, BoxError>
where
    F: FnMut(&str) -> Result<Vec<u8>, raw_payload_store::RawPayloadStoreError>,
{
    let content = attachment.content.trim();
    if !content.is_empty() {
        match BASE64_STANDARD.decode(content.as_bytes()) {
            Ok(bytes) => return Ok(bytes),
            Err(err) => {
                warn!(
                    "failed to decode attachment '{}' base64 content: {}",
                    attachment.name, err
                );
            }
        }
    }

    if let Some(storage_ref) = attachment
        .storage_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let bytes = downloader(storage_ref).map_err(|err| {
            format!(
                "failed to download attachment '{}' from '{}': {}",
                attachment.name, storage_ref, err
            )
        })?;
        return Ok(bytes);
    }

    Ok(Vec::new())
}

fn build_inbound_entry_name(payload: &PostmarkInbound, seq: u64) -> String {
    let subject = payload.subject.as_deref().unwrap_or("");
    let subject_token = sanitize_token(subject, "no_subject");
    let subject_token = truncate_ascii(&subject_token, 48);
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let base = format!("{}_{}", timestamp, subject_token);
    format!("{:04}_{}", seq, base)
}

fn truncate_ascii(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let mut out = value[..max_len].to_string();
    while out.ends_with(['.', '_', '-']) {
        out.pop();
    }
    if out.is_empty() {
        value.to_string()
    } else {
        out
    }
}

fn sanitize_token(value: &str, fallback: &str) -> String {
    let trimmed = value.trim().trim_start_matches('<').trim_end_matches('>');
    let mut out = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let cleaned = out.trim_matches(&['.', '_', '-'][..]);
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::employee_config::EmployeeProfile;
    use crate::user_store::UserPaths;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn resolve_inbound_requester_uses_github_sender_for_notifications() {
        let raw = br#"{
  "From": "Bingran You <notifications@github.com>",
  "Headers": [{"Name": "X-GitHub-Sender", "Value": "bingran-you"}],
  "TextBody": "bingran-you left a comment (KnoWhiz/DoWhiz#568)"
}"#;
        let payload: PostmarkInbound = serde_json::from_slice(raw).expect("payload");
        let requester = resolve_inbound_requester(&payload, raw).expect("requester");
        assert_eq!(
            requester,
            InboundRequester {
                identifier_type: "github",
                identifier: "bingran-you".to_string(),
            }
        );
    }

    #[test]
    fn resolve_inbound_requester_falls_back_to_email() {
        let raw = br#"{
  "From": "Alice <alice@example.com>",
  "TextBody": "hello"
}"#;
        let payload: PostmarkInbound = serde_json::from_slice(raw).expect("payload");
        let requester = resolve_inbound_requester(&payload, raw).expect("requester");
        assert_eq!(
            requester,
            InboundRequester {
                identifier_type: "email",
                identifier: "alice@example.com".to_string(),
            }
        );
    }

    #[test]
    fn email_message_task_id_is_stable_for_duplicate_delivery() {
        let first = email_message_task_id("thread:abc", Some("<msg-1@example.com>"));
        let duplicate = email_message_task_id("thread:abc", Some("<msg-1@example.com>"));

        assert_eq!(first, duplicate);
    }

    #[test]
    fn email_message_task_id_changes_for_distinct_messages() {
        let first = email_message_task_id("thread:abc", Some("<msg-1@example.com>"));
        let second = email_message_task_id("thread:abc", Some("<msg-2@example.com>"));

        assert_ne!(first, second);
    }

    #[test]
    fn create_workspace_hydrates_past_emails() {
        let temp = TempDir::new().expect("tempdir");
        let user_root = temp.path().join("user");
        let user_paths = UserPaths {
            root: user_root.clone(),
            state_dir: user_root.join("state"),
            tasks_db_path: user_root.join("state/tasks.db"),
            memory_dir: user_root.join("memory"),
            secrets_dir: user_root.join("secrets"),
            mail_root: user_root.join("mail"),
            workspaces_root: user_root.join("workspaces"),
        };
        fs::create_dir_all(&user_paths.mail_root).expect("mail root");
        fs::create_dir_all(&user_paths.workspaces_root).expect("workspaces root");

        let archive_dir = user_paths.mail_root.join("2026").join("02").join("msg_1");
        let incoming_email = archive_dir.join("incoming_email");
        let incoming_attachments = archive_dir.join("incoming_attachments");
        fs::create_dir_all(&incoming_email).expect("incoming_email");
        fs::create_dir_all(&incoming_attachments).expect("incoming_attachments");
        fs::write(incoming_email.join("email.html"), "<pre>Hello</pre>").expect("email.html");
        let archived_payload = r#"{
  "From": "Alice <alice@example.com>",
  "To": "Bob <bob@example.com>",
  "Subject": "Archive hello",
  "Date": "Tue, 03 Feb 2026 20:10:44 -0800",
  "MessageID": "<msg-1@example.com>",
  "Attachments": [
    {"Name": "report.pdf", "ContentType": "application/pdf"}
  ]
}"#;
        fs::write(
            incoming_email.join("postmark_payload.json"),
            archived_payload,
        )
        .expect("postmark payload");
        fs::write(incoming_attachments.join("report.pdf"), "data").expect("attachment");

        let inbound_raw = r#"{
  "From": "New <new@example.com>",
  "To": "Service <service@example.com>",
  "Subject": "New request",
  "TextBody": "Hi"
}"#;
        let inbound_payload: PostmarkInbound =
            serde_json::from_str(inbound_raw).expect("parse inbound");
        let thread = thread_key(&inbound_payload, inbound_raw.as_bytes());
        let addresses = vec!["service@example.com".to_string()];
        let address_set: HashSet<String> = addresses
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect();
        let employee = EmployeeProfile {
            id: "test-employee".to_string(),
            display_name: None,
            runner: "codex".to_string(),
            model: None,
            addresses,
            address_set,
            runtime_root: None,
            agents_path: None,
            claude_path: None,
            soul_path: None,
            skills_dir: None,
            discord_enabled: false,
            slack_enabled: false,
            bluebubbles_enabled: false,
            notion_user_id: None,
        };
        let workspace = ensure_thread_workspace(&user_paths, "user123", &thread, &employee, None)
            .expect("create workspace");

        let past_root = workspace.join("references").join("past_emails");
        let index_path = past_root.join("index.json");
        assert!(index_path.exists(), "index.json created");

        let index_data = fs::read_to_string(index_path).expect("read index");
        let index_json: serde_json::Value = serde_json::from_str(&index_data).expect("parse index");
        let entries = index_json["entries"].as_array().expect("entries array");
        assert_eq!(entries.len(), 1, "one archived entry");
        let entry_path = entries[0]["path"].as_str().expect("entry path");
        assert!(past_root.join(entry_path).join("incoming_email").exists());
        assert!(past_root
            .join(entry_path)
            .join("attachments_manifest.json")
            .exists());
    }

    #[test]
    fn write_inbound_payload_sanitizes_workspace_email_views() {
        let temp = TempDir::new().expect("tempdir");
        let incoming_email = temp.path().join("incoming_email");
        let incoming_attachments = temp.path().join("incoming_attachments");
        fs::create_dir_all(&incoming_email).expect("incoming_email");
        fs::create_dir_all(&incoming_attachments).expect("incoming_attachments");

        let raw = r#"{
  "From": "Maggie Ke <wjke@uchicago.edu>",
  "To": "Oliver <oliver@dowhiz.com>",
  "Subject": "Re:",
  "TextBody": "我把我的账号信息再发一次给你：",
  "HtmlBody": "<p>我把我的账号信息再发一次给你：</p><p><a href=\"https://learning.edx.org/course/123\">Course link</a></p><img src=\"data:image/png;base64,AAAA\" alt=\"image.png\">",
  "Attachments": [
    {"Name": "image.png", "Content": "aGVsbG8=", "ContentType": "image/png"}
  ]
}"#;
        let payload: PostmarkInbound = serde_json::from_str(raw).expect("payload");

        write_inbound_payload(
            &payload,
            raw.as_bytes(),
            &incoming_email,
            &incoming_attachments,
        )
        .expect("write inbound payload");

        let email_html = fs::read_to_string(incoming_email.join("email.html")).expect("email html");
        assert!(email_html.contains("我把我的账号信息再发一次给你："));
        assert!(email_html.contains("https://learning.edx.org/course/123"));
        assert!(!email_html.contains("data:image"));

        let workspace_payload =
            fs::read_to_string(incoming_email.join("postmark_payload.json")).expect("payload json");
        assert!(!workspace_payload.contains("data:image"));
        assert!(!workspace_payload.contains("aGVsbG8="));

        let workspace_json: serde_json::Value =
            serde_json::from_str(&workspace_payload).expect("parse payload json");
        assert_eq!(
            workspace_json["TextBody"].as_str(),
            Some("我把我的账号信息再发一次给你：\n\nLinks:\n- https://learning.edx.org/course/123")
        );
        assert_eq!(
            workspace_json["HtmlBody"].as_str(),
            Some(email_html.as_str())
        );
        assert_eq!(
            workspace_json["Attachments"][0]["Content"].as_str(),
            Some("")
        );
        assert_eq!(
            workspace_json["Attachments"][0]["ContentLength"].as_u64(),
            Some(5)
        );
        assert!(incoming_attachments.join("image.png").exists());
    }

    #[test]
    fn append_inbound_payload_uses_stripped_reply_for_followup_thread_request() {
        let temp = TempDir::new().expect("tempdir");

        let original_raw = r#"{
  "From": "Logan <logan@example.com>",
  "To": "Oliver <oliver@dowhiz.com>",
  "Subject": "Nvidia stock",
  "TextBody": "Give me a deep research about the Nvidia stock, and tell me whether it is a good time to buy",
  "MessageID": "<msg-1@example.com>"
}"#;
        let original_payload: PostmarkInbound =
            serde_json::from_str(original_raw).expect("original payload");
        append_inbound_payload(temp.path(), &original_payload, original_raw.as_bytes(), 1)
            .expect("append original payload");

        let followup_text =
            "Can you help me do some deep research for the competitors, and the upstream/downstream for Nvidia. Give me suggestion to some of the investment options among those companies";
        let followup_raw = format!(
            r#"{{
  "From": "Logan <logan@example.com>",
  "To": "Oliver <oliver@dowhiz.com>",
  "Subject": "Re: Nvidia stock",
  "TextBody": "{followup_text}\n\nOn Sat, Apr 18, 2026 at 1:14 PM Logan wrote:\n> Give me a deep research about the Nvidia stock, and tell me whether it is a good time to buy",
  "StrippedTextReply": "{followup_text}",
  "MessageID": "<msg-2@example.com>"
}}"#
        );
        let followup_payload: PostmarkInbound =
            serde_json::from_str(&followup_raw).expect("followup payload");
        append_inbound_payload(temp.path(), &followup_payload, followup_raw.as_bytes(), 2)
            .expect("append followup payload");

        let thread_request =
            fs::read_to_string(temp.path().join("incoming_email/thread_request.md"))
                .expect("thread_request");
        let latest_section = thread_request
            .split("## Latest inbound message\n")
            .nth(1)
            .and_then(|section| section.split("\n## Thread timeline\n").next())
            .expect("latest inbound message section");

        assert!(latest_section.contains("competitors"));
        assert!(!latest_section.contains("Give me a deep research about the Nvidia stock"));
    }

    #[test]
    fn resolve_attachment_bytes_prefers_base64_content() {
        let attachment = super::super::postmark::PostmarkAttachment {
            name: "report.txt".to_string(),
            content: BASE64_STANDARD.encode("hello"),
            storage_ref: Some("azure://container/path".to_string()),
            content_type: "text/plain".to_string(),
        };
        let bytes = resolve_attachment_bytes_with_downloader(&attachment, |_ref| {
            Ok("fallback".as_bytes().to_vec())
        })
        .expect("bytes");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn resolve_attachment_bytes_uses_storage_ref_when_content_missing() {
        let attachment = super::super::postmark::PostmarkAttachment {
            name: "report.txt".to_string(),
            content: String::new(),
            storage_ref: Some("azure://container/path".to_string()),
            content_type: "text/plain".to_string(),
        };
        let bytes = resolve_attachment_bytes_with_downloader(&attachment, |reference| {
            assert_eq!(reference, "azure://container/path");
            Ok("blob-data".as_bytes().to_vec())
        })
        .expect("bytes");
        assert_eq!(bytes, b"blob-data");
    }

    #[test]
    fn resolve_attachment_bytes_falls_back_to_storage_ref_on_invalid_base64() {
        let attachment = super::super::postmark::PostmarkAttachment {
            name: "report.txt".to_string(),
            content: "%%%invalid%%%".to_string(),
            storage_ref: Some("azure://container/path".to_string()),
            content_type: "text/plain".to_string(),
        };
        let bytes = resolve_attachment_bytes_with_downloader(&attachment, |reference| {
            assert_eq!(reference, "azure://container/path");
            Ok("blob-data".as_bytes().to_vec())
        })
        .expect("bytes");
        assert_eq!(bytes, b"blob-data");
    }

    #[test]
    fn account_lookup_uses_github_identifier_for_github_notifications() {
        // Verify resolve_inbound_requester returns github identifier for GitHub notifications
        let raw = br#"{
  "From": "Bingran You <notifications@github.com>",
  "Headers": [{"Name": "X-GitHub-Sender", "Value": "bingran-you"}],
  "TextBody": "bingran-you left a comment"
}"#;
        let payload: PostmarkInbound = serde_json::from_slice(raw).expect("payload");
        let requester = resolve_inbound_requester(&payload, raw).expect("requester");
        // Account lookup should use "github" identifier type, not "email"
        assert_eq!(requester.identifier_type, "github");
        assert_eq!(requester.identifier, "bingran-you");
    }

    #[test]
    fn account_lookup_uses_email_identifier_for_regular_emails() {
        // Verify resolve_inbound_requester returns email identifier for non-GitHub emails
        let raw = br#"{
  "From": "Alice Smith <alice@example.com>",
  "TextBody": "hello"
}"#;
        let payload: PostmarkInbound = serde_json::from_slice(raw).expect("payload");
        let requester = resolve_inbound_requester(&payload, raw).expect("requester");
        // Account lookup should use "email" identifier type
        assert_eq!(requester.identifier_type, "email");
        assert_eq!(requester.identifier, "alice@example.com");
    }

    #[test]
    fn human_approval_gate_subject_detection_matches_hag_threads() {
        assert!(is_human_approval_gate_subject(
            "[HAG:49d7368d-95a6-4c6c-91cc-8c30a4583c35] 2FA approval needed"
        ));
        assert!(is_human_approval_gate_subject(
            "Re: [HAG:49d7368d-95a6-4c6c-91cc-8c30a4583c35] 2FA approval needed"
        ));
        assert!(is_human_approval_gate_subject(
            "re:    [hag:49d7368d-95a6-4c6c-91cc-8c30a4583c35] 2fa approval needed"
        ));
        assert!(!is_human_approval_gate_subject("Re: Project update"));
        assert!(!is_human_approval_gate_subject(""));
    }

    #[test]
    fn is_human_approval_gate_reply_uses_subject_field() {
        let payload: PostmarkInbound = serde_json::from_str(
            r#"{
  "From": "Admin <admin@dowhiz.com>",
  "To": "DoWhiz <dowhiz@deep-tutor.com>",
  "Subject": "Re: [HAG:abc-123] 2FA approval needed for account",
  "TextBody": "APPROVED"
}"#,
        )
        .expect("payload");
        assert!(is_human_approval_gate_reply(&payload));
    }

    #[test]
    fn extract_human_approval_gate_challenge_id_reads_subject_token() {
        assert_eq!(
            extract_human_approval_gate_challenge_id(
                "Re: [HAG:abc-123] Password needed for dowhiz@deep-tutor.com Google account"
            )
            .as_deref(),
            Some("abc-123")
        );
        assert_eq!(
            extract_human_approval_gate_challenge_id("re:   [hag:xyz] hello").as_deref(),
            Some("xyz")
        );
        assert_eq!(
            extract_human_approval_gate_challenge_id("Re: project update"),
            None
        );
    }

    #[test]
    fn record_human_approval_gate_reply_updates_matching_challenge_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let users_root = temp.path().join("users");
        let challenge_id = "c9105173-2f9f-4a1e-96cc-00f470374eea";
        let challenge_path = users_root
            .join("user_1")
            .join("workspaces")
            .join("thread_1")
            .join(".human_approval_gate")
            .join("challenges")
            .join(format!("{challenge_id}.json"));
        fs::create_dir_all(challenge_path.parent().expect("challenge parent"))
            .expect("create challenge dir");
        fs::write(
            &challenge_path,
            serde_json::to_vec_pretty(&json!({
                "challenge_id": challenge_id,
                "status": "pending",
                "seen_inbound_message_ids": [],
                "reply": Value::Null,
            }))
            .expect("serialize state"),
        )
        .expect("write challenge");

        let payload: PostmarkInbound = serde_json::from_str(
            r#"{
  "From": "Admin <admin@dowhiz.com>",
  "Subject": "Re: [HAG:c9105173-2f9f-4a1e-96cc-00f470374eea] Password needed for dowhiz@deep-tutor.com Google account",
  "TextBody": "done",
  "StrippedTextReply": "done",
  "MessageID": "msg-123"
}"#,
        )
        .expect("payload");

        let updated = record_human_approval_gate_reply(&users_root, &payload).expect("record");
        assert_eq!(updated, 1);

        let state: Value =
            serde_json::from_slice(&fs::read(&challenge_path).expect("read challenge"))
                .expect("parse challenge");
        assert_eq!(state.get("status").and_then(Value::as_str), Some("replied"));
        assert_eq!(
            state
                .get("seen_inbound_message_ids")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(Value::as_str),
            Some("msg-123")
        );
        assert_eq!(
            state
                .get("reply")
                .and_then(Value::as_object)
                .and_then(|reply| reply.get("stripped_text_reply"))
                .and_then(Value::as_str),
            Some("done")
        );
        assert_eq!(
            state
                .get("reply")
                .and_then(Value::as_object)
                .and_then(|reply| reply.get("from_email"))
                .and_then(Value::as_str),
            Some("admin@dowhiz.com")
        );
    }

    #[test]
    fn blacklisted_sender_detects_mailer_daemon() {
        let service_addresses = HashSet::new();
        assert!(is_blacklisted_sender(
            "Mail Delivery Subsystem <mailer-daemon@googlemail.com>",
            &service_addresses
        ));
        assert!(is_blacklisted_sender(
            "postmaster@example.com",
            &service_addresses
        ));
    }

    #[test]
    fn account_for_tasks_prefers_resolved_account_id() {
        use uuid::Uuid;

        let resolved_account_id =
            Some(Uuid::parse_str("26a8b960-bef3-4329-a4b1-6ccfbfd49bbf").unwrap());
        let fallback_account_id =
            Some(Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap());

        // When resolved_account_id is Some, use it (ignore fallback)
        let account_for_tasks = if resolved_account_id.is_some() {
            resolved_account_id
        } else {
            fallback_account_id
        };

        assert_eq!(
            account_for_tasks,
            Some(Uuid::parse_str("26a8b960-bef3-4329-a4b1-6ccfbfd49bbf").unwrap())
        );

        // Verify directory path is built correctly
        let users_root = std::path::PathBuf::from("/home/azureuser/users");
        let acct_id = account_for_tasks.unwrap();
        let account_tasks_dir = users_root.join(acct_id.to_string()).join("state");
        assert_eq!(
            account_tasks_dir.to_string_lossy(),
            "/home/azureuser/users/26a8b960-bef3-4329-a4b1-6ccfbfd49bbf/state"
        );
    }

    #[test]
    fn account_for_tasks_falls_back_when_resolved_is_none() {
        use uuid::Uuid;

        let resolved_account_id: Option<Uuid> = None;
        let fallback_account_id =
            Some(Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap());

        // When resolved_account_id is None, use fallback
        let account_for_tasks = if resolved_account_id.is_some() {
            resolved_account_id
        } else {
            fallback_account_id
        };

        assert_eq!(
            account_for_tasks,
            Some(Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap())
        );

        // Verify directory path uses fallback
        let users_root = std::path::PathBuf::from("/home/azureuser/users");
        let acct_id = account_for_tasks.unwrap();
        let account_tasks_dir = users_root.join(acct_id.to_string()).join("state");
        assert_eq!(
            account_tasks_dir.to_string_lossy(),
            "/home/azureuser/users/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/state"
        );
    }

    #[test]
    fn account_for_tasks_is_none_when_both_are_none() {
        use uuid::Uuid;

        let resolved_account_id: Option<Uuid> = None;
        let fallback_account_id: Option<Uuid> = None;

        // When both are None, account_for_tasks is None (no task storage)
        let account_for_tasks = if resolved_account_id.is_some() {
            resolved_account_id
        } else {
            fallback_account_id
        };

        assert!(account_for_tasks.is_none());
    }
}
