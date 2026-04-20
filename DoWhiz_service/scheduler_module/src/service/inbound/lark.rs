use std::path::Path;
use std::time::Duration;

use tracing::{info, warn};
use uuid::Uuid;

use crate::account_store::AccountStore;
use crate::channel::Channel;
use crate::index_store::IndexStore;
use crate::user_store::UserStore;
use crate::{ModuleExecutor, RunTaskTask, Scheduler, TaskKind};

use super::super::bump_thread_state;
use super::super::config::ServiceConfig;
use super::super::default_thread_state_path;
use super::super::scheduler::cancel_pending_thread_tasks;
use super::super::workspace::ensure_thread_workspace;
use super::super::BoxError;

pub(crate) fn process_lark_event(
    config: &ServiceConfig,
    user_store: &UserStore,
    index_store: &IndexStore,
    account_store: &AccountStore,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
) -> Result<(), BoxError> {
    info!("processing Lark event");

    info!(
        "Lark message from {} in chat {:?}: {:?}",
        message.sender, message.metadata.lark_chat_id, message.text_body
    );

    let chat_id = message
        .metadata
        .lark_chat_id
        .as_deref()
        .unwrap_or("default");

    let user = user_store.get_or_create_user("lark", &message.sender)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    user_store.ensure_user_dirs(&user_paths)?;

    // Thread key: chat_id + user_id for grouping conversations
    let thread_key = format!("lark:{}:{}", chat_id, message.sender);

    let workspace = ensure_thread_workspace(
        &user_paths,
        &user.user_id,
        &thread_key,
        &config.employee_profile,
        config.skills_source_dir.as_deref(),
    )?;

    let thread_state_path = default_thread_state_path(&workspace);
    let thread_state =
        bump_thread_state(&thread_state_path, &thread_key, message.message_id.clone())?;

    append_lark_message(
        &workspace,
        message,
        raw_payload,
        thread_state.last_email_seq.try_into().unwrap_or(u32::MAX),
    )?;

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

    let run_task = RunTaskTask {
        workspace_dir: workspace.clone(),
        input_email_dir: std::path::PathBuf::from("incoming_email"),
        input_attachments_dir: std::path::PathBuf::from("incoming_attachments"),
        memory_dir: std::path::PathBuf::from("memory"),
        reference_dir: std::path::PathBuf::from("references"),
        model_name,
        runner: config.employee_profile.runner.clone(),
        codex_disabled: config.codex_disabled,
        reply_to: vec![message.sender.clone()],
        reply_from: None,
        archive_root: Some(user_paths.mail_root.clone()),
        thread_id: Some(thread_key.clone()),
        thread_epoch: Some(thread_state.epoch),
        thread_state_path: Some(thread_state_path.clone()),
        channel: Channel::Lark,
        slack_team_id: None,
        employee_id: Some(config.employee_profile.id.clone()),
        requester_identifier_type: None,
        requester_identifier: None,
        account_id: None,
        channel_metadata: message.metadata.clone(),
    };

    // Clone run_task before consuming it, in case we need to write to account-level storage
    let run_task_for_account = run_task.clone();

    let mut scheduler = Scheduler::load(&user_paths.tasks_db_path, ModuleExecutor::default())?;
    if let Err(err) = cancel_pending_thread_tasks(&mut scheduler, &workspace, thread_state.epoch) {
        warn!(
            "failed to cancel pending thread tasks for {}: {}",
            workspace.display(),
            err
        );
    }
    let task_id = if let Some(stable_task_id) = lark_message_task_id(message) {
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
                "skipping duplicate lark full-task enqueue user_id={} task_id={} message_id={:?}",
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

    // If the Lark user has linked their account, also write to account-level tasks.db
    // message.sender contains the Lark open_id
    if let Ok(Some(account)) = account_store.get_account_by_identifier("lark", &message.sender) {
        let account_tasks_dir = config.users_root.join(account.id.to_string()).join("state");
        if let Err(err) = std::fs::create_dir_all(&account_tasks_dir) {
            warn!(
                "failed to create account tasks dir for account {}: {}",
                account.id, err
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
                                account.id, task_id
                            );
                        }
                        Ok(false) => {
                            info!(
                                "skipping duplicate lark account-level enqueue account={} task_id={}",
                                account.id, task_id
                            );
                        }
                        Err(err) => {
                            warn!(
                                "failed to add task to account scheduler for account {}: {}",
                                account.id, err
                            );
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        "failed to load account scheduler for account {}: {}",
                        account.id, err
                    );
                }
            }
        }
    }

    Ok(())
}

fn lark_message_task_id(message: &crate::channel::InboundMessage) -> Option<Uuid> {
    let chat_id = message.metadata.lark_chat_id.as_deref()?.trim();
    let message_id = message
        .message_id
        .as_deref()
        .or(message.metadata.lark_message_id.as_deref())?
        .trim();
    let thread_id = message.thread_id.trim();
    if chat_id.is_empty() || message_id.is_empty() || thread_id.is_empty() {
        return None;
    }

    let tenant_key = message
        .metadata
        .lark_tenant_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    let dedupe_key = format!("lark:{tenant_key}:{chat_id}:{thread_id}:{message_id}");
    Some(Uuid::from_bytes(md5::compute(dedupe_key.as_bytes()).0))
}

/// Append a Lark message to the workspace inbox.
pub(super) fn append_lark_message(
    workspace: &Path,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
    seq: u32,
) -> Result<(), BoxError> {
    let incoming_dir = workspace.join("incoming_email");
    std::fs::create_dir_all(&incoming_dir)?;

    // Save raw JSON payload for debugging
    let json_filename = format!("{:04}_lark.json", seq);
    std::fs::write(incoming_dir.join(&json_filename), raw_payload)?;

    // Save text content as .txt file
    if let Some(ref text) = message.text_body {
        let txt_filename = format!("{:04}_lark.txt", seq);
        let content = format!(
            "From: {}\nDate: {}\n\n{}",
            message.sender,
            chrono::Utc::now().to_rfc3339(),
            text
        );
        std::fs::write(incoming_dir.join(&txt_filename), content)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, ChannelMetadata, InboundMessage};
    use tempfile::tempdir;

    fn make_test_message(sender: &str, text: Option<&str>) -> InboundMessage {
        InboundMessage {
            channel: Channel::Lark,
            sender: sender.to_string(),
            sender_name: Some("Test User".to_string()),
            recipient: "oc_chat123".to_string(),
            subject: None,
            text_body: text.map(|s| s.to_string()),
            html_body: None,
            thread_id: format!("lark:oc_chat123:{}", sender),
            message_id: Some("msg_123".to_string()),
            attachments: vec![],
            reply_to: vec![sender.to_string()],
            raw_payload: vec![],
            metadata: ChannelMetadata {
                lark_app_id: Some("cli_xxx".to_string()),
                lark_tenant_key: Some("tenant_xxx".to_string()),
                lark_open_id: Some(sender.to_string()),
                lark_chat_id: Some("oc_chat123".to_string()),
                lark_message_id: Some("msg_123".to_string()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn append_lark_message_creates_incoming_dir() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();
        let message = make_test_message("ou_user1", Some("Hello"));
        let raw = b"{}";

        append_lark_message(workspace, &message, raw, 1).unwrap();

        assert!(workspace.join("incoming_email").exists());
    }

    #[test]
    fn append_lark_message_writes_json_file() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();
        let message = make_test_message("ou_user1", Some("Hello"));
        let raw = br#"{"event": "test"}"#;

        append_lark_message(workspace, &message, raw, 1).unwrap();

        let json_path = workspace.join("incoming_email/0001_lark.json");
        assert!(json_path.exists());
        let content = std::fs::read_to_string(json_path).unwrap();
        assert_eq!(content, r#"{"event": "test"}"#);
    }

    #[test]
    fn append_lark_message_writes_txt_file() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();
        let message = make_test_message("ou_sender", Some("Test message content"));
        let raw = b"{}";

        append_lark_message(workspace, &message, raw, 1).unwrap();

        let txt_path = workspace.join("incoming_email/0001_lark.txt");
        assert!(txt_path.exists());
        let content = std::fs::read_to_string(txt_path).unwrap();
        assert!(content.contains("From: ou_sender"));
        assert!(content.contains("Test message content"));
    }

    #[test]
    fn append_lark_message_skips_txt_when_no_text() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();
        let message = make_test_message("ou_user", None); // No text body
        let raw = b"{}";

        append_lark_message(workspace, &message, raw, 1).unwrap();

        let json_path = workspace.join("incoming_email/0001_lark.json");
        let txt_path = workspace.join("incoming_email/0001_lark.txt");

        assert!(json_path.exists());
        assert!(!txt_path.exists()); // Should not create txt file
    }

    #[test]
    fn append_lark_message_uses_sequence_number() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();
        let message = make_test_message("ou_user", Some("Hi"));
        let raw = b"{}";

        append_lark_message(workspace, &message, raw, 42).unwrap();

        assert!(workspace.join("incoming_email/0042_lark.json").exists());
        assert!(workspace.join("incoming_email/0042_lark.txt").exists());
    }

    #[test]
    fn append_lark_message_handles_high_sequence() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();
        let message = make_test_message("ou_user", Some("Hi"));
        let raw = b"{}";

        append_lark_message(workspace, &message, raw, 9999).unwrap();

        assert!(workspace.join("incoming_email/9999_lark.json").exists());
        assert!(workspace.join("incoming_email/9999_lark.txt").exists());
    }

    #[test]
    fn lark_message_task_id_is_stable_for_duplicate_delivery() {
        let mut first = make_test_message("ou_sender", Some("Hello"));
        first.raw_payload = br#"{"event_id":"evt-1"}"#.to_vec();

        let mut duplicate = make_test_message("ou_sender", Some("Hello"));
        duplicate.raw_payload = br#"{"event_id":"evt-2"}"#.to_vec();

        assert_eq!(
            lark_message_task_id(&first),
            lark_message_task_id(&duplicate)
        );
    }

    #[test]
    fn lark_message_task_id_changes_for_distinct_messages() {
        let first = make_test_message("ou_sender", Some("Hello"));
        let mut second = make_test_message("ou_sender", Some("Hello again"));
        second.message_id = Some("msg_456".to_string());
        second.metadata.lark_message_id = Some("msg_456".to_string());

        assert_ne!(lark_message_task_id(&first), lark_message_task_id(&second));
    }
}
