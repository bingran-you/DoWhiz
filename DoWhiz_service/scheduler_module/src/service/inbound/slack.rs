use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use tracing::{info, warn};
use uuid::Uuid;

use crate::account_store::AccountStore;
use crate::adapters::slack::SlackEventWrapper;
use crate::channel::{Channel, InboundAdapter};
use crate::index_store::IndexStore;
use crate::slack_store::{resolve_slack_bot_token_for_runtime, SlackStore};
use crate::thread_state::ThreadState;
use crate::user_store::{UserPaths, UserRecord, UserStore};
use crate::{ModuleExecutor, RunTaskTask, Scheduler, TaskKind};

use super::super::bump_thread_state;
use super::super::config::ServiceConfig;
use super::super::default_thread_state_path;
use super::super::scheduler::cancel_pending_thread_tasks;
use super::super::workspace::{ensure_thread_workspace, refresh_thread_input_snapshot};
use super::super::write_slack_chat_history_scope_file;
use super::super::BoxError;

const SLACK_ROUTER_CONTEXT_CHAR_LIMIT: usize = 4_000;
const SLACK_ROUTER_RECENT_COUNT: usize = 6;

#[derive(Debug, Clone)]
pub(crate) struct PersistedSlackContext {
    pub(crate) user: UserRecord,
    pub(crate) user_paths: UserPaths,
    pub(crate) workspace: std::path::PathBuf,
    pub(crate) thread_key: String,
    pub(crate) thread_state_path: std::path::PathBuf,
    pub(crate) thread_state: ThreadState,
}

pub(crate) fn process_slack_event(
    config: &ServiceConfig,
    user_store: &UserStore,
    index_store: &IndexStore,
    slack_store: &SlackStore,
    account_store: &AccountStore,
    raw_payload: &[u8],
) -> Result<(), BoxError> {
    use crate::adapters::slack::SlackInboundAdapter;

    info!("processing slack event");

    // Parse wrapper first to get team_id
    let wrapper: SlackEventWrapper = serde_json::from_slice(raw_payload)?;

    // Look up bot_user_id from SlackStore (with fallback to env var)
    let team_id = wrapper.team_id.as_deref().unwrap_or("");
    let mut bot_user_ids = HashSet::new();
    if let Some(installation) = slack_store
        .resolve_installation_for_runtime(Some(team_id), Some(&config.employee_profile.id))
    {
        if !installation.bot_user_id.is_empty() {
            bot_user_ids.insert(installation.bot_user_id);
        }
    } else if let Some(ref bot_id) = config.slack_bot_user_id {
        // Legacy fallback
        bot_user_ids.insert(bot_id.clone());
    }
    let adapter = SlackInboundAdapter::new(bot_user_ids);

    // Check if this is a bot message (should be ignored)
    if let Some(ref event) = wrapper.event {
        if adapter.is_bot_message(event) {
            info!("ignoring bot message from user {:?}", event.user);
            return Ok(());
        }
    }

    let message = adapter.parse(raw_payload)?;

    info!(
        "slack message from {} in channel {:?}: {:?}",
        message.sender, message.metadata.slack_channel_id, message.text_body
    );

    let persisted = persist_slack_ingest_context(config, user_store, &message, raw_payload)?;
    let channel_id = message
        .metadata
        .slack_channel_id
        .as_ref()
        .ok_or("missing slack_channel_id")?;
    let user = persisted.user;
    let user_paths = persisted.user_paths;
    let workspace = persisted.workspace;
    let thread_key = persisted.thread_key;
    let thread_state_path = persisted.thread_state_path;
    let thread_state = persisted.thread_state;

    // Determine model and runner
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

    info!(
        "workspace ready at {} for user {} thread={} epoch={}",
        workspace.display(),
        user.user_id,
        thread_key,
        thread_state.epoch
    );

    let linked_account_id = match account_store.get_account_by_identifier("slack", &message.sender)
    {
        Ok(Some(account)) => Some(account.id),
        Ok(None) => None,
        Err(err) => {
            warn!(
                "failed to resolve slack sender {} to billing account: {}",
                message.sender, err
            );
            None
        }
    };

    // Create RunTask to process the message
    let run_task = RunTaskTask {
        workspace_dir: workspace.clone(),
        input_email_dir: std::path::PathBuf::from("incoming_email"),
        input_attachments_dir: std::path::PathBuf::from("incoming_attachments"),
        memory_dir: std::path::PathBuf::from("memory"),
        reference_dir: std::path::PathBuf::from("references"),
        model_name,
        runner: config.employee_profile.runner.clone(),
        codex_disabled: config.codex_disabled,
        // reply_to[0] = user_id (for account lookup), reply_to[1] = channel_id
        reply_to: vec![message.sender.clone(), channel_id.clone()],
        reply_from: None, // Slack uses bot token, not a "from" address
        archive_root: Some(user_paths.mail_root.clone()),
        thread_id: Some(thread_key.clone()),
        thread_epoch: Some(thread_state.epoch),
        thread_state_path: Some(thread_state_path.clone()),
        channel: Channel::Slack,
        slack_team_id: message.metadata.slack_team_id.clone(),
        employee_id: Some(config.employee_profile.id.clone()),
        requester_identifier_type: Some("slack".to_string()),
        requester_identifier: Some(message.sender.clone()),
        account_id: linked_account_id,
        channel_metadata: message.metadata.clone(),
    };

    // Clone run_task before consuming it, in case we need to write to account-level storage
    let run_task_for_account = run_task.clone();

    // Schedule the task
    let mut scheduler = Scheduler::load(&user_paths.tasks_db_path, ModuleExecutor::default())?;
    if let Err(err) = cancel_pending_thread_tasks(&mut scheduler, &workspace, thread_state.epoch) {
        warn!(
            "failed to cancel pending thread tasks for {}: {}",
            workspace.display(),
            err
        );
    }
    let task_id = if let Some(stable_task_id) = slack_message_task_id(&message) {
        // This stable task id is only for full RunTask duplicate-delivery
        // suppression. Quick responses are deduped separately via
        // service/inbound/quick_responses.rs claim files.
        let inserted = scheduler.add_one_shot_in_if_absent_with_id(
            stable_task_id,
            Duration::from_secs(0),
            TaskKind::RunTask(run_task),
        )?;
        if !inserted {
            info!(
                "skipping duplicate slack full-task enqueue user_id={} task_id={} message_id={:?}",
                user.user_id, stable_task_id, message.message_id
            );
        }
        stable_task_id
    } else {
        scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))?
    };
    index_store.sync_user_tasks(&user.user_id, scheduler.tasks())?;

    info!(
        "scheduler tasks enqueued user_id={} task_id={} message_id={:?} workspace={} thread_epoch={}",
        user.user_id,
        task_id,
        message.message_id,
        workspace.display(),
        thread_state.epoch
    );

    // If the Slack user has linked their account, also write to account-level tasks.db.
    if let Some(account_id) = linked_account_id {
        let account_tasks_dir = config.users_root.join(account_id.to_string()).join("state");
        if let Err(err) = std::fs::create_dir_all(&account_tasks_dir) {
            warn!(
                "failed to create account tasks dir for account {}: {}",
                account_id, err
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
                                account_id, task_id
                            );
                        }
                        Ok(false) => {
                            info!(
                                "skipping duplicate slack account-level enqueue account={} task_id={}",
                                account_id, task_id
                            );
                        }
                        Err(err) => {
                            warn!(
                                "failed to add task to account scheduler for account {}: {}",
                                account_id, err
                            );
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        "failed to load account scheduler for account {}: {}",
                        account_id, err
                    );
                }
            }
        }
    }

    Ok(())
}

fn ensure_slack_workspace(
    config: &ServiceConfig,
    user_store: &UserStore,
    message: &crate::channel::InboundMessage,
) -> Result<(UserRecord, UserPaths, std::path::PathBuf, String), BoxError> {
    let channel_id = message
        .metadata
        .slack_channel_id
        .as_deref()
        .ok_or("missing slack_channel_id")?;
    let user = user_store.get_or_create_user("slack", &message.sender)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    user_store.ensure_user_dirs(&user_paths)?;

    let thread_key = format!("slack:{}:{}", channel_id, message.thread_id);
    let workspace = ensure_thread_workspace(
        &user_paths,
        &user.user_id,
        &thread_key,
        &config.employee_profile,
        config.skills_source_dir.as_deref(),
    )?;

    Ok((user, user_paths, workspace, thread_key))
}

fn slack_message_task_id(message: &crate::channel::InboundMessage) -> Option<Uuid> {
    let channel_id = message.metadata.slack_channel_id.as_deref()?.trim();
    let message_id = message.message_id.as_deref()?.trim();
    let thread_id = message.thread_id.trim();
    if channel_id.is_empty() || message_id.is_empty() || thread_id.is_empty() {
        return None;
    }

    let team_id = message
        .metadata
        .slack_team_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    let dedupe_key = format!("slack:{team_id}:{channel_id}:{thread_id}:{message_id}");
    Some(Uuid::from_bytes(md5::compute(dedupe_key.as_bytes()).0))
}

pub(crate) fn persist_slack_ingest_context(
    config: &ServiceConfig,
    user_store: &UserStore,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
) -> Result<PersistedSlackContext, BoxError> {
    let (user, user_paths, workspace, thread_key) =
        ensure_slack_workspace(config, user_store, message)?;

    if let Err(err) = write_slack_chat_history_scope_file(config, &workspace, message) {
        warn!(
            "failed to write scoped Slack history grant for {}: {}",
            workspace.display(),
            err
        );
    }

    let thread_state_path = default_thread_state_path(&workspace);
    let thread_state =
        bump_thread_state(&thread_state_path, &thread_key, message.message_id.clone())?;
    append_slack_message(
        &workspace,
        message,
        raw_payload,
        thread_state.last_email_seq,
    )?;
    if let Err(err) =
        hydrate_slack_attachments(config, &workspace, raw_payload, thread_state.last_email_seq)
    {
        warn!(
            "failed to hydrate Slack attachments for {}: {}",
            workspace.display(),
            err
        );
    }

    Ok(PersistedSlackContext {
        user,
        user_paths,
        workspace,
        thread_key,
        thread_state_path,
        thread_state,
    })
}

pub(crate) fn build_slack_router_context(
    config: &ServiceConfig,
    user_store: &UserStore,
    message: &crate::channel::InboundMessage,
) -> Result<Option<String>, BoxError> {
    let (_, _, workspace, _) = ensure_slack_workspace(config, user_store, message)?;
    let incoming_dir = workspace.join("incoming_email");
    let thread_request_path = incoming_dir.join("thread_request.md");
    if let Ok(content) = std::fs::read_to_string(&thread_request_path) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Ok(Some(truncate_slack_router_context(trimmed)));
        }
    }

    Ok(
        render_recent_slack_workspace_messages(&incoming_dir, SLACK_ROUTER_RECENT_COUNT)
            .map(|content| truncate_slack_router_context(&content)),
    )
}

fn truncate_slack_router_context(value: &str) -> String {
    let mut truncated = value
        .chars()
        .take(SLACK_ROUTER_CONTEXT_CHAR_LIMIT)
        .collect::<String>();
    if value.chars().count() > SLACK_ROUTER_CONTEXT_CHAR_LIMIT {
        truncated.push_str("\n\n(Truncated.)");
    }
    truncated
}

#[derive(Debug, serde::Deserialize)]
struct SlackWorkspaceMetaLite {
    #[serde(default)]
    sender: Option<String>,
    #[serde(default)]
    sender_name: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
}

fn render_recent_slack_workspace_messages(
    incoming_dir: &Path,
    max_messages: usize,
) -> Option<String> {
    let entries = std::fs::read_dir(incoming_dir).ok()?;
    let mut message_files = entries
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with("_slack_message.txt"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    message_files.sort();
    if message_files.is_empty() {
        return None;
    }

    let start = message_files.len().saturating_sub(max_messages);
    let mut lines = Vec::new();
    lines.push("Recent Slack thread context from earlier messages:".to_string());
    for path in message_files.into_iter().skip(start) {
        let text = std::fs::read_to_string(&path).ok()?;
        let prefix = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.split('_').next())
            .unwrap_or_default();
        let meta_path = incoming_dir.join(format!("{prefix}_slack_meta.json"));
        let meta = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<SlackWorkspaceMetaLite>(&raw).ok());
        let sender = meta
            .as_ref()
            .and_then(|value| value.sender_name.as_deref())
            .or_else(|| meta.as_ref().and_then(|value| value.sender.as_deref()))
            .unwrap_or("Slack user");
        let timestamp = meta
            .as_ref()
            .and_then(|value| value.timestamp.as_deref())
            .unwrap_or("");
        let label = if timestamp.is_empty() {
            sender.to_string()
        } else {
            format!("{sender} at {timestamp}")
        };
        lines.push(format!("- {}: {}", label, text.trim()));
    }

    Some(lines.join("\n"))
}

/// Save an incoming Slack message to the workspace.
pub(super) fn append_slack_message(
    workspace: &Path,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
    seq: u64,
) -> Result<(), BoxError> {
    let incoming_dir = workspace.join("incoming_email");
    let incoming_attachments = workspace.join("incoming_attachments");
    let entries_email = incoming_dir.join("entries");
    let entries_attachments = incoming_attachments.join("entries");
    std::fs::create_dir_all(&incoming_dir)?;
    std::fs::create_dir_all(&entries_email)?;
    std::fs::create_dir_all(&entries_attachments)?;

    // Save the raw JSON payload
    let raw_path = incoming_dir.join(format!("{:05}_slack_raw.json", seq));
    std::fs::write(&raw_path, raw_payload)?;

    // Save message text as a simple text file (similar to email body)
    let text_path = incoming_dir.join(format!("{:05}_slack_message.txt", seq));
    let text_content = message.text_body.clone().unwrap_or_default();
    std::fs::write(&text_path, &text_content)?;

    // Create a metadata file with sender info
    let meta_path = incoming_dir.join(format!("{:05}_slack_meta.json", seq));
    let meta = serde_json::json!({
        "channel": "slack",
        "sender": message.sender,
        "sender_name": message.sender_name,
        "channel_id": message.metadata.slack_channel_id,
        "team_id": message.metadata.slack_team_id,
        "thread_id": message.thread_id,
        "message_id": message.message_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;

    let entry_name = format!("{:05}_slack", seq);
    let entry_email_dir = entries_email.join(&entry_name);
    let entry_attachments_dir = entries_attachments.join(&entry_name);
    std::fs::create_dir_all(&entry_email_dir)?;
    std::fs::create_dir_all(&entry_attachments_dir)?;
    std::fs::write(
        entry_email_dir.join("postmark_payload.json"),
        serde_json::to_vec_pretty(&build_slack_workspace_payload(message, &text_content))?,
    )?;
    std::fs::write(entry_email_dir.join("email.txt"), &text_content)?;

    if let Err(err) = refresh_thread_input_snapshot(&incoming_dir, &incoming_attachments) {
        warn!(
            "failed to refresh Slack thread input snapshot for {}: {}",
            workspace.display(),
            err
        );
    }

    info!(
        "saved Slack message seq={} to {}",
        seq,
        incoming_dir.display()
    );
    Ok(())
}

fn build_slack_workspace_payload(
    message: &crate::channel::InboundMessage,
    text_content: &str,
) -> serde_json::Value {
    let sender = message
        .sender_name
        .as_deref()
        .map(|name| format!("{name} ({})", message.sender))
        .unwrap_or_else(|| message.sender.clone());
    serde_json::json!({
        "Channel": "Slack",
        "Subject": "Slack thread message",
        "From": sender,
        "To": message.metadata.slack_channel_id,
        "Cc": "",
        "Bcc": "",
        "Date": chrono::Utc::now().to_rfc3339(),
        "MessageID": message.message_id,
        "TextBody": text_content,
        "HtmlBody": serde_json::Value::Null,
        "SlackThreadId": message.thread_id,
        "SlackChannelId": message.metadata.slack_channel_id,
        "SlackTeamId": message.metadata.slack_team_id,
    })
}

fn hydrate_slack_attachments(
    config: &ServiceConfig,
    workspace: &Path,
    raw_payload: &[u8],
    seq: u64,
) -> Result<(), BoxError> {
    let incoming_attachments = workspace.join("incoming_attachments");
    let entries_dir = incoming_attachments.join("entries");
    std::fs::create_dir_all(&entries_dir)?;
    clear_dir_except(&incoming_attachments, &entries_dir)?;

    if raw_payload.is_empty() {
        return Ok(());
    }

    let wrapper: SlackEventWrapper = match serde_json::from_slice(raw_payload) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "failed to parse slack raw payload for attachment hydration: {}",
                err
            );
            return Ok(());
        }
    };

    let team_id = wrapper.team_id.clone();
    let attachments = wrapper
        .event
        .and_then(|event| event.files)
        .unwrap_or_default();
    if attachments.is_empty() {
        return Ok(());
    }

    let Some(bot_token) =
        resolve_slack_bot_token_for_runtime(team_id.as_deref(), Some(&config.employee_profile.id))
    else {
        warn!(
            "unable to download Slack attachments for workspace {}: missing runtime bot token (team_id={})",
            workspace.display(),
            team_id.as_deref().unwrap_or("")
        );
        return Ok(());
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let entry_dir = entries_dir.join(format!("{:05}_slack_attachments", seq));
    std::fs::create_dir_all(&entry_dir)?;

    let mut saved = 0usize;
    let mut used_names = HashSet::new();
    for attachment in attachments {
        let source_name = attachment
            .name
            .as_deref()
            .or(attachment.title.as_deref())
            .unwrap_or("attachment");
        let file_name = ascii_safe_attachment_name(source_name, &mut used_names);
        match download_slack_attachment(&client, &attachment, &bot_token) {
            Ok(bytes) => {
                std::fs::write(incoming_attachments.join(&file_name), &bytes)?;
                std::fs::write(entry_dir.join(&file_name), &bytes)?;
                saved += 1;
            }
            Err(err) => {
                warn!(
                    "failed to download Slack attachment '{}' for workspace {}: {}",
                    source_name,
                    workspace.display(),
                    err
                );
            }
        }
    }

    if saved > 0 {
        info!(
            "saved {} Slack attachment(s) seq={} to {}",
            saved,
            seq,
            incoming_attachments.display()
        );
    }

    if let Err(err) =
        refresh_thread_input_snapshot(&workspace.join("incoming_email"), &incoming_attachments)
    {
        warn!(
            "failed to refresh Slack thread input snapshot after attachment hydration for {}: {}",
            workspace.display(),
            err
        );
    }

    Ok(())
}

fn download_slack_attachment(
    client: &reqwest::blocking::Client,
    attachment: &crate::adapters::slack::SlackFile,
    bot_token: &str,
) -> Result<Vec<u8>, BoxError> {
    let mut candidate_urls = Vec::new();
    if let Some(url) = attachment
        .url_private_download
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        candidate_urls.push(url.to_string());
    }
    if let Some(url) = attachment
        .url_private
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !candidate_urls.iter().any(|existing| existing == url) {
            candidate_urls.push(url.to_string());
        }
    }

    if candidate_urls.is_empty() {
        let attachment_name = attachment
            .name
            .as_deref()
            .or(attachment.title.as_deref())
            .unwrap_or("attachment");
        return Err(format!(
            "no downloadable Slack URL found for attachment '{}'",
            attachment_name
        )
        .into());
    }

    let mut last_error = None;
    for url in candidate_urls {
        match download_slack_attachment_from_url(client, &url, bot_token) {
            Ok(bytes) => return Ok(bytes),
            Err(err) => {
                last_error = Some(format!("{url}: {err}"));
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| "unknown Slack attachment download error".to_string())
        .into())
}

fn download_slack_attachment_from_url(
    client: &reqwest::blocking::Client,
    url: &str,
    bot_token: &str,
) -> Result<Vec<u8>, BoxError> {
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", bot_token))
        .send()?;
    if !response.status().is_success() {
        return Err(format!("slack attachment download returned {}", response.status()).into());
    }

    Ok(response.bytes()?.to_vec())
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

fn ascii_safe_attachment_name(path: &str, used_names: &mut HashSet<String>) -> String {
    let trimmed = path.trim();
    let (stem, extension) = match trimmed.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            (stem, Some(extension))
        }
        _ => (trimmed, None),
    };
    let mut base = sanitize_ascii_attachment_stem(stem);
    if base.is_empty() {
        base = "attachment".to_string();
    }

    let extension = extension
        .map(sanitize_ascii_attachment_extension)
        .filter(|value| !value.is_empty());
    uniquify_attachment_name(base, extension.as_deref(), used_names)
}

fn sanitize_ascii_attachment_stem(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '.' | '_' | '-') {
            output.push(ch);
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    let trimmed = output.trim_matches(['.', '_', '-']);
    truncate_ascii(trimmed, 80)
}

fn sanitize_ascii_attachment_extension(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn uniquify_attachment_name(
    base: String,
    extension: Option<&str>,
    used_names: &mut HashSet<String>,
) -> String {
    let build_name = |suffix: Option<usize>| match (extension, suffix) {
        (Some(ext), Some(idx)) => format!("{base}_{idx}.{ext}"),
        (Some(ext), None) => format!("{base}.{ext}"),
        (None, Some(idx)) => format!("{base}_{idx}"),
        (None, None) => base.clone(),
    };

    let mut candidate = build_name(None);
    if used_names.insert(candidate.clone()) {
        return candidate;
    }

    for idx in 2..10_000 {
        candidate = build_name(Some(idx));
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
    }

    build_name(Some(10_000))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, ChannelMetadata, InboundMessage};
    use crate::employee_config::{EmployeeDirectory, EmployeeProfile};
    use mockito::Server;
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let original = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn build_test_config(root: &Path) -> ServiceConfig {
        let users_root = root.join("users");
        let state_root = root.join("state");

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
            address_set: address_set.clone(),
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
        let mut employee_by_id = HashMap::new();
        employee_by_id.insert(employee.id.clone(), employee.clone());
        let employee_directory = EmployeeDirectory {
            employees: vec![employee.clone()],
            employee_by_id,
            default_employee_id: Some(employee.id.clone()),
            service_addresses: address_set,
        };

        ServiceConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            employee_id: employee.id.clone(),
            employee_config_path: root.join("employee.toml"),
            employee_profile: employee,
            employee_directory,
            workspace_root: root.join("workspaces"),
            scheduler_state_path: state_root.join("tasks.db"),
            processed_ids_path: state_root.join("processed_ids.txt"),
            ingestion_db_url: "postgres://localhost/test".to_string(),
            ingestion_poll_interval: Duration::from_millis(50),
            users_root,
            users_db_path: state_root.join("users.db"),
            task_index_path: state_root.join("task_index.db"),
            codex_model: "gpt-5.4".to_string(),
            codex_disabled: true,
            scheduler_poll_interval: Duration::from_millis(50),
            scheduler_max_concurrency: 1,
            scheduler_user_max_concurrency: 1,
            inbound_body_max_bytes: crate::service::DEFAULT_INBOUND_BODY_MAX_BYTES,
            skills_source_dir: None,
            slack_bot_token: None,
            slack_bot_user_id: None,
            slack_store_path: state_root.join("slack.db"),
            slack_client_id: None,
            slack_client_secret: None,
            slack_redirect_uri: None,
            discord_bot_token: None,
            discord_bot_user_id: None,
            google_docs_enabled: false,
            bluebubbles_url: None,
            bluebubbles_password: None,
            telegram_bot_token: None,
            whatsapp_access_token: None,
            whatsapp_phone_number_id: None,
            whatsapp_verify_token: None,
        }
    }

    fn build_message(text: &str, thread_id: &str, message_id: &str) -> InboundMessage {
        InboundMessage {
            channel: Channel::Slack,
            sender: "U123".to_string(),
            sender_name: Some("Bingran".to_string()),
            recipient: "C123".to_string(),
            subject: None,
            text_body: Some(text.to_string()),
            html_body: None,
            thread_id: thread_id.to_string(),
            message_id: Some(message_id.to_string()),
            attachments: Vec::new(),
            reply_to: vec!["C123".to_string()],
            raw_payload: br#"{"type":"event_callback"}"#.to_vec(),
            metadata: ChannelMetadata {
                slack_channel_id: Some("C123".to_string()),
                slack_team_id: Some("T123".to_string()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn append_slack_message_builds_thread_snapshot() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path();

        append_slack_message(
            workspace,
            &build_message("Root ask", "1700.1", "1700.1"),
            br#"{"ts":"1700.1"}"#,
            1,
        )
        .expect("append root");
        append_slack_message(
            workspace,
            &build_message("Follow-up detail", "1700.1", "1700.2"),
            br#"{"ts":"1700.2","thread_ts":"1700.1"}"#,
            2,
        )
        .expect("append follow-up");

        let incoming = workspace.join("incoming_email");
        let thread_request =
            std::fs::read_to_string(incoming.join("thread_request.md")).expect("thread request");
        let thread_history =
            std::fs::read_to_string(incoming.join("thread_history.md")).expect("thread history");

        assert!(thread_request.contains("Follow-up detail"));
        assert!(thread_request.contains("Root ask"));
        assert!(thread_history.contains("00001_slack"));
        assert!(thread_history.contains("00002_slack"));
        assert!(incoming
            .join("entries/00001_slack/postmark_payload.json")
            .exists());
        assert!(incoming.join("entries/00002_slack/email.txt").exists());
    }

    #[test]
    fn render_recent_slack_workspace_messages_formats_recent_messages() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path();

        append_slack_message(
            workspace,
            &build_message("First context", "1701.1", "1701.1"),
            br#"{"ts":"1701.1"}"#,
            1,
        )
        .expect("append first");
        append_slack_message(
            workspace,
            &build_message("Second context", "1701.1", "1701.2"),
            br#"{"ts":"1701.2","thread_ts":"1701.1"}"#,
            2,
        )
        .expect("append second");

        let rendered = render_recent_slack_workspace_messages(&workspace.join("incoming_email"), 6)
            .expect("rendered");
        assert!(rendered.contains("Recent Slack thread context"));
        assert!(rendered.contains("First context"));
        assert!(rendered.contains("Second context"));
        assert!(rendered.contains("Bingran"));
    }

    #[test]
    fn slack_message_task_id_is_stable_for_duplicate_delivery() {
        let mut first = build_message("Root ask", "1700.1", "1700.1");
        first.raw_payload = br#"{"event_id":"Ev1"}"#.to_vec();

        let mut duplicate = build_message("Root ask", "1700.1", "1700.1");
        duplicate.raw_payload = br#"{"event_id":"Ev2"}"#.to_vec();

        assert_eq!(
            slack_message_task_id(&first),
            slack_message_task_id(&duplicate)
        );
    }

    #[test]
    fn slack_message_task_id_changes_for_distinct_messages() {
        let first = build_message("Root ask", "1700.1", "1700.1");
        let second = build_message("Follow-up detail", "1700.1", "1700.2");

        assert_ne!(
            slack_message_task_id(&first),
            slack_message_task_id(&second)
        );
    }

    #[test]
    fn hydrate_slack_attachments_downloads_file_share_attachments() -> Result<(), BoxError> {
        let _guard = env_lock().lock().expect("env lock");
        let temp = TempDir::new()?;
        let root = temp.path();
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace)?;

        let _employee_token =
            EnvGuard::set("TEST_EMPLOYEE_SLACK_BOT_TOKEN", "xoxb-test-employee-token");
        let config = build_test_config(root);

        let mut server = Server::new();
        let attachment_bytes = b"zip-bytes";
        let attachment_mock = server
            .mock("GET", "/files/SkyShake.zip")
            .match_header("authorization", "Bearer xoxb-test-employee-token")
            .with_status(200)
            .with_body(attachment_bytes.as_slice())
            .create();

        let raw_payload = serde_json::to_vec(&serde_json::json!({
            "type": "event_callback",
            "team_id": "T123",
            "event": {
                "type": "message",
                "subtype": "file_share",
                "channel": "D123",
                "channel_type": "im",
                "user": "U123",
                "text": "Attached project zip",
                "ts": "1700.1",
                "files": [{
                    "id": "F123",
                    "name": "SkyShake.zip",
                    "title": "SkyShake.zip",
                    "mimetype": "application/zip",
                    "url_private_download": format!("{}/files/SkyShake.zip", server.url())
                }]
            },
            "event_id": "Ev123"
        }))?;

        hydrate_slack_attachments(&config, &workspace, &raw_payload, 7)?;

        attachment_mock.assert();

        let attachment_path = workspace.join("incoming_attachments").join("skyshake.zip");
        assert_eq!(fs::read(&attachment_path)?, attachment_bytes);

        let archived_attachment = workspace
            .join("incoming_attachments")
            .join("entries")
            .join("00007_slack_attachments")
            .join("skyshake.zip");
        assert_eq!(fs::read(archived_attachment)?, attachment_bytes);

        let manifest = fs::read_to_string(
            workspace
                .join("incoming_attachments")
                .join("thread_manifest.json"),
        )?;
        assert!(manifest.contains("skyshake.zip"));
        Ok(())
    }
}
