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

pub(crate) fn process_wechat_mp_event(
    config: &ServiceConfig,
    user_store: &UserStore,
    index_store: &IndexStore,
    account_store: &AccountStore,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
) -> Result<(), BoxError> {
    info!("processing WeChat MP event");

    let app_id = message
        .metadata
        .wechat_mp_app_id
        .as_deref()
        .unwrap_or("default_app");
    let open_id = message
        .metadata
        .wechat_mp_open_id
        .as_deref()
        .unwrap_or(message.sender.as_str());

    info!(
        "WeChat MP message from open_id={} app_id={} text={:?}",
        open_id, app_id, message.text_body
    );

    let user = user_store.get_or_create_user("wechat_mp", open_id)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    user_store.ensure_user_dirs(&user_paths)?;

    let thread_key = format!("wechat_mp:{}:{}", app_id, open_id);

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

    append_wechat_mp_message(
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
        reply_to: vec![open_id.to_string()],
        reply_from: None,
        archive_root: Some(user_paths.mail_root.clone()),
        thread_id: Some(thread_key.clone()),
        thread_epoch: Some(thread_state.epoch),
        thread_state_path: Some(thread_state_path.clone()),
        channel: Channel::WeChatMp,
        slack_team_id: None,
        employee_id: Some(config.employee_profile.id.clone()),
        requester_identifier_type: None,
        requester_identifier: None,
        account_id: None,
        channel_metadata: message.metadata.clone(),
    };

    let run_task_for_account = run_task.clone();

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

    if let Ok(Some(account)) = account_store.get_account_by_identifier("wechat_mp", open_id) {
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

pub(super) fn append_wechat_mp_message(
    workspace: &Path,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
    seq: u32,
) -> Result<(), BoxError> {
    let incoming_dir = workspace.join("incoming_email");
    std::fs::create_dir_all(&incoming_dir)?;

    let xml_filename = format!("{:04}_wechat_mp.xml", seq);
    std::fs::write(incoming_dir.join(&xml_filename), raw_payload)?;

    if let Some(ref text) = message.text_body {
        let txt_filename = format!("{:04}_wechat_mp.txt", seq);
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
