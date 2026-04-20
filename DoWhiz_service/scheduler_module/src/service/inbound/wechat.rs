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

pub(crate) fn process_wechat_event(
    config: &ServiceConfig,
    user_store: &UserStore,
    index_store: &IndexStore,
    account_store: &AccountStore,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
) -> Result<(), BoxError> {
    info!("processing WeChat event");

    info!(
        "WeChat message from {} in corp {:?}: {:?}",
        message.sender, message.metadata.wechat_corp_id, message.text_body
    );

    // Get corp ID for thread grouping
    let corp_id = message
        .metadata
        .wechat_corp_id
        .as_deref()
        .unwrap_or("default");

    let user = user_store.get_or_create_user("wechat", &message.sender)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    user_store.ensure_user_dirs(&user_paths)?;

    // Thread key: corp_id + user_id for grouping conversations
    let thread_key = format!("wechat:{}:{}", corp_id, message.sender);

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

    // Save the incoming WeChat message to workspace
    append_wechat_message(
        &workspace,
        message,
        raw_payload,
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
        channel: Channel::WeChat,
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
    let task_id = if let Some(stable_task_id) = wechat_message_task_id(message) {
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
                "skipping duplicate wechat full-task enqueue user_id={} task_id={} message_id={:?}",
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

    // If the WeChat user has linked their account, also write to account-level tasks.db
    // Identifier format: {corp_id}_{user_id} to match OAuth linking
    let wechat_identifier = format!("{}_{}", corp_id, message.sender);
    if let Ok(Some(account)) = account_store.get_account_by_identifier("wechat", &wechat_identifier)
    {
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
                                "skipping duplicate wechat account-level enqueue account={} task_id={}",
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

fn wechat_message_task_id(message: &crate::channel::InboundMessage) -> Option<Uuid> {
    let corp_id = message.metadata.wechat_corp_id.as_deref()?.trim();
    let message_id = message.message_id.as_deref()?.trim();
    let thread_id = message.thread_id.trim();
    if corp_id.is_empty() || message_id.is_empty() || thread_id.is_empty() {
        return None;
    }

    let dedupe_key = format!("wechat:{corp_id}:{thread_id}:{message_id}");
    Some(Uuid::from_bytes(md5::compute(dedupe_key.as_bytes()).0))
}

/// Append a WeChat message to the workspace inbox.
pub(super) fn append_wechat_message(
    workspace: &Path,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
    seq: u32,
) -> Result<(), BoxError> {
    let incoming_dir = workspace.join("incoming_email");
    std::fs::create_dir_all(&incoming_dir)?;

    // Save raw XML payload for debugging
    let xml_filename = format!("{:04}_wechat.xml", seq);
    std::fs::write(incoming_dir.join(&xml_filename), raw_payload)?;

    // Save text content as .txt file
    if let Some(ref text) = message.text_body {
        let txt_filename = format!("{:04}_wechat.txt", seq);
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

    fn build_message(message_id: &str) -> InboundMessage {
        InboundMessage {
            channel: Channel::WeChat,
            sender: "wechat-user".to_string(),
            sender_name: None,
            recipient: "corp-1".to_string(),
            subject: None,
            text_body: Some("hello".to_string()),
            html_body: None,
            thread_id: "wechat:corp-1:wechat-user".to_string(),
            message_id: Some(message_id.to_string()),
            attachments: Vec::new(),
            reply_to: vec!["wechat-user".to_string()],
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                wechat_corp_id: Some("corp-1".to_string()),
                wechat_user_id: Some("wechat-user".to_string()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn wechat_message_task_id_is_stable_for_duplicate_delivery() {
        let mut first = build_message("msg-1");
        first.raw_payload = br#"<xml><MsgId>msg-1</MsgId></xml>"#.to_vec();
        let mut duplicate = build_message("msg-1");
        duplicate.raw_payload = br#"<xml><MsgId>msg-1</MsgId><Retry>1</Retry></xml>"#.to_vec();

        assert_eq!(
            wechat_message_task_id(&first),
            wechat_message_task_id(&duplicate)
        );
    }

    #[test]
    fn wechat_message_task_id_changes_for_distinct_messages() {
        let first = build_message("msg-1");
        let second = build_message("msg-2");

        assert_ne!(
            wechat_message_task_id(&first),
            wechat_message_task_id(&second)
        );
    }
}
