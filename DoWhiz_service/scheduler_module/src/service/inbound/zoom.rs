use std::path::Path;
use std::time::Duration;

use tracing::{info, warn};

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

pub(crate) fn process_zoom_message(
    config: &ServiceConfig,
    user_store: &UserStore,
    index_store: &IndexStore,
    account_store: &AccountStore,
    message: &crate::channel::InboundMessage,
) -> Result<(), BoxError> {
    info!("processing Zoom message");

    // Get zoom_user_id and meeting_uuid from metadata
    let zoom_user_id = message
        .metadata
        .zoom_user_id
        .as_deref()
        .unwrap_or(&message.sender);
    let meeting_uuid = message
        .metadata
        .zoom_meeting_uuid
        .as_deref()
        .unwrap_or("unknown");

    info!(
        "Zoom message from user {} in meeting {}: {:?}",
        zoom_user_id, meeting_uuid, message.text_body
    );

    // Create user based on zoom_user_id
    let user = user_store.get_or_create_user("zoom", zoom_user_id)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    user_store.ensure_user_dirs(&user_paths)?;

    // Thread key: meeting_uuid for grouping conversations within a meeting
    let thread_key = format!("zoom:{}", meeting_uuid);

    // Create/get workspace for this thread
    let workspace = ensure_thread_workspace(
        &user_paths,
        &user.user_id,
        &thread_key,
        &config.employee_profile,
        config.skills_source_dir.as_deref(),
    )?;

    // Bump thread state
    let thread_state_path = default_thread_state_path(&workspace);
    let thread_state =
        bump_thread_state(&thread_state_path, &thread_key, message.message_id.clone())?;

    // Save the incoming Zoom message to workspace
    append_zoom_message(
        &workspace,
        message,
        thread_state.last_email_seq.try_into().unwrap_or(u32::MAX),
    )?;

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

    // Create RunTask to process the message
    // reply_to[0] = zoom_user_id (for account lookup), reply_to[1] = meeting_uuid
    let run_task = RunTaskTask {
        workspace_dir: workspace.clone(),
        input_email_dir: std::path::PathBuf::from("incoming_email"),
        input_attachments_dir: std::path::PathBuf::from("incoming_attachments"),
        memory_dir: std::path::PathBuf::from("memory"),
        reference_dir: std::path::PathBuf::from("references"),
        model_name,
        runner: config.employee_profile.runner.clone(),
        codex_disabled: config.codex_disabled,
        reply_to: vec![zoom_user_id.to_string(), meeting_uuid.to_string()],
        reply_from: None,
        archive_root: Some(user_paths.mail_root.clone()),
        thread_id: Some(thread_key.clone()),
        thread_epoch: Some(thread_state.epoch),
        thread_state_path: Some(thread_state_path.clone()),
        channel: Channel::Zoom,
        slack_team_id: None,
        employee_id: Some(config.employee_profile.id.clone()),
        requester_identifier_type: None,
        requester_identifier: None,
        account_id: None,
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
    let task_id = scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))?;
    index_store.sync_user_tasks(&user.user_id, scheduler.tasks())?;

    info!(
        "scheduler tasks enqueued user_id={} task_id={} message_id={:?} workspace={} thread_epoch={}",
        user.user_id,
        task_id,
        message.message_id,
        workspace.display(),
        thread_state.epoch
    );

    // If the Zoom user has linked their account, also write to account-level tasks.db
    if let Ok(Some(account)) = account_store.get_account_by_identifier("zoom", zoom_user_id) {
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
                    match account_scheduler.add_one_shot_in_with_id(
                        task_id,
                        Duration::from_secs(0),
                        TaskKind::RunTask(run_task_for_account),
                    ) {
                        Ok(()) => {
                            info!(
                                "also enqueued task to account-level storage account={} task_id={}",
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

/// Append a Zoom message to the workspace inbox.
fn append_zoom_message(
    workspace: &Path,
    message: &crate::channel::InboundMessage,
    seq: u32,
) -> Result<(), BoxError> {
    let incoming_dir = workspace.join("incoming_email");
    std::fs::create_dir_all(&incoming_dir)?;

    // Save text content as .txt file
    if let Some(ref text) = message.text_body {
        let txt_filename = format!("{:04}_zoom.txt", seq);
        let sender_name = message.sender_name.as_deref().unwrap_or(&message.sender);
        let meeting_uuid = message
            .metadata
            .zoom_meeting_uuid
            .as_deref()
            .unwrap_or("unknown");
        let content = format!(
            "From: {} ({})\nMeeting: {}\nDate: {}\n\n{}",
            sender_name,
            message.sender,
            meeting_uuid,
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
    use crate::channel::{ChannelMetadata, InboundMessage};
    use tempfile::TempDir;

    fn make_zoom_message(
        sender: &str,
        text: &str,
        zoom_user_id: Option<&str>,
        meeting_uuid: Option<&str>,
    ) -> InboundMessage {
        let mut metadata = ChannelMetadata::default();
        metadata.zoom_user_id = zoom_user_id.map(|s| s.to_string());
        metadata.zoom_meeting_uuid = meeting_uuid.map(|s| s.to_string());

        InboundMessage {
            channel: Channel::Zoom,
            sender: sender.to_string(),
            sender_name: Some("Test User".to_string()),
            recipient: "proto".to_string(),
            subject: None,
            text_body: Some(text.to_string()),
            html_body: None,
            thread_id: "test_thread".to_string(),
            message_id: Some("zoom_msg_123".to_string()),
            attachments: vec![],
            reply_to: vec![],
            raw_payload: vec![],
            metadata,
        }
    }

    #[test]
    fn test_append_zoom_message_creates_file() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();

        let message = make_zoom_message(
            "zoom_user:U123",
            "Hey Proto, create a new repo",
            Some("U123"),
            Some("meeting_abc123"),
        );

        append_zoom_message(workspace, &message, 1).expect("should succeed");

        let incoming_dir = workspace.join("incoming_email");
        assert!(incoming_dir.exists());

        let file_path = incoming_dir.join("0001_zoom.txt");
        assert!(file_path.exists());

        let content = std::fs::read_to_string(&file_path).expect("read file");
        assert!(content.contains("From: Test User"));
        assert!(content.contains("zoom_user:U123"));
        assert!(content.contains("Meeting: meeting_abc123"));
        assert!(content.contains("Hey Proto, create a new repo"));
    }

    #[test]
    fn test_append_zoom_message_no_text_body() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();

        let mut message =
            make_zoom_message("zoom_user:U123", "", Some("U123"), Some("meeting_abc"));
        message.text_body = None;

        append_zoom_message(workspace, &message, 1).expect("should succeed");

        let incoming_dir = workspace.join("incoming_email");
        assert!(incoming_dir.exists());

        // No file created when text_body is None
        let file_path = incoming_dir.join("0001_zoom.txt");
        assert!(!file_path.exists());
    }

    #[test]
    fn test_append_zoom_message_fallback_meeting_uuid() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();

        // No meeting_uuid in metadata - should fallback to "unknown"
        let message = make_zoom_message("zoom_user:U123", "Test message", Some("U123"), None);

        append_zoom_message(workspace, &message, 5).expect("should succeed");

        let file_path = workspace.join("incoming_email").join("0005_zoom.txt");
        let content = std::fs::read_to_string(&file_path).expect("read file");
        assert!(content.contains("Meeting: unknown"));
    }

    #[test]
    fn test_append_zoom_message_seq_formatting() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = temp.path();

        let message = make_zoom_message("user", "msg", Some("U1"), Some("mtg"));

        // Test different sequence numbers for formatting
        append_zoom_message(workspace, &message, 1).expect("seq 1");
        append_zoom_message(workspace, &message, 99).expect("seq 99");
        append_zoom_message(workspace, &message, 1234).expect("seq 1234");

        let incoming_dir = workspace.join("incoming_email");
        assert!(incoming_dir.join("0001_zoom.txt").exists());
        assert!(incoming_dir.join("0099_zoom.txt").exists());
        assert!(incoming_dir.join("1234_zoom.txt").exists());
    }
}
