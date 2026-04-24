use std::fs;
use std::path::Path;
use std::time::Duration;

use tracing::{info, warn};

use run_task_module::aci_container_store::{
    deregister_aci_container_mongo, list_aci_containers, AciContainerRecord,
};
use run_task_module::{
    delete_aci_container_by_name, download_ephemeral_share_for_recovery,
    poll_aci_container_until_terminal, query_aci_container_status, read_aci_recovery_context,
    AciContainerStatus,
};

use crate::channel::{Channel, ChannelMetadata};
use crate::scheduler::outbound::{
    execute_bluebubbles_send, execute_discord_send, execute_email_send, execute_google_docs_send,
    execute_lark_send, execute_notion_send, execute_slack_send, execute_sms_send,
    execute_telegram_send, execute_wechat_mp_send, execute_wechat_send, execute_whatsapp_send,
};
use crate::scheduler::SendReplyTask;

const OUTBOUND_SENT_MARKER: &str = ".outbound_sent";
const POLL_TIMEOUT_SECS: u64 = 3600; // 1 hour max wait

/// Recover orphaned ACI containers on worker startup.
/// For each container registered in MongoDB:
/// 1. Check if outbound was already sent (marker file)
/// 2. If not, poll until terminal, then propagate results
/// 3. Cleanup (delete from Azure, deregister from MongoDB)
///
/// Recovery runs in parallel - each container gets its own thread from tokio's blocking
/// pool so a slow/stuck container doesn't block others from completing. Tasks are
/// fire-and-forget so one stuck container doesn't block worker startup or other recoveries.
pub async fn recover_orphaned_aci_containers() {
    let containers = list_aci_containers();
    if containers.is_empty() {
        info!("no orphaned ACI containers to recover");
        return;
    }

    info!(
        "found {} potentially orphaned ACI container(s) to recover",
        containers.len()
    );

    // Spawn a blocking task for each container (parallel, fire-and-forget)
    for container in containers {
        let container_name = container.container_name.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(err) = recover_single_container(&container) {
                warn!(
                    "failed to recover container {}: {}",
                    container_name, err
                );
            }
        });
        // No .await - fire and forget
    }

    info!("spawned recovery tasks for all orphaned containers");
}

fn recover_single_container(container: &AciContainerRecord) -> Result<(), String> {
    let workspace = &container.workspace_path;
    let marker_path = workspace.join(OUTBOUND_SENT_MARKER);

    info!(
        "recovering container {} (workspace: {})",
        container.container_name,
        workspace.display()
    );

    // Check if outbound was already sent
    if marker_path.exists() {
        info!(
            "container {} already sent outbound (marker exists), skipping propagation",
            container.container_name
        );
    } else {
        // Check Azure status and poll if needed
        let status =
            query_aci_container_status(&container.container_name, &container.resource_group);

        match status {
            AciContainerStatus::Running => {
                info!(
                    "container {} still running, polling until terminal",
                    container.container_name
                );
                match poll_aci_container_until_terminal(
                    &container.container_name,
                    &container.resource_group,
                    Duration::from_secs(POLL_TIMEOUT_SECS),
                ) {
                    Ok(state) => {
                        info!(
                            "container {} reached terminal state: {}",
                            container.container_name, state
                        );
                    }
                    Err(err) => {
                        warn!(
                            "container {} poll failed: {}, continuing with cleanup",
                            container.container_name, err
                        );
                    }
                }
            }
            AciContainerStatus::Terminal(state) => {
                info!(
                    "container {} already terminal: {}",
                    container.container_name, state
                );
            }
            AciContainerStatus::NotFound => {
                info!(
                    "container {} not found in Azure (already deleted)",
                    container.container_name
                );
            }
            AciContainerStatus::Error(err) => {
                warn!(
                    "error querying container {} status: {}",
                    container.container_name, err
                );
            }
        }

        // Download results from ephemeral share (if used)
        if let Err(err) =
            download_ephemeral_share_for_recovery(&container.container_name, workspace)
        {
            warn!(
                "ephemeral share download failed for container {}: {} (may not have used ephemeral share)",
                container.container_name, err
            );
        }

        // Propagate results to outbound
        if let Err(err) = propagate_results_to_outbound(workspace) {
            warn!(
                "failed to propagate results for container {}: {}",
                container.container_name, err
            );
        } else {
            // Write marker file after successful outbound
            if let Err(err) = fs::write(&marker_path, "sent") {
                warn!(
                    "failed to write outbound marker for container {}: {}",
                    container.container_name, err
                );
            }
        }
    }

    // Cleanup: delete from Azure
    if let Err(err) =
        delete_aci_container_by_name(&container.container_name, &container.resource_group)
    {
        warn!(
            "failed to delete container {} from Azure: {}",
            container.container_name, err
        );
    }

    // Cleanup: deregister from MongoDB
    deregister_aci_container_mongo(&container.container_name);

    info!("finished recovering container {}", container.container_name);

    Ok(())
}

/// Propagate results from workspace to outbound adapter.
/// Reads channel and reply_to from .aci_recovery_context.json written at ACI creation.
fn propagate_results_to_outbound(workspace: &Path) -> Result<(), String> {
    if !workspace.exists() {
        return Err(format!("workspace does not exist: {}", workspace.display()));
    }

    // Read recovery context written when ACI was created
    let context = read_aci_recovery_context(workspace).ok_or_else(|| {
        format!(
            "missing .aci_recovery_context.json in workspace {}",
            workspace.display()
        )
    })?;

    let channel: Channel = context
        .channel
        .parse()
        .map_err(|_| format!("invalid channel in recovery context: {}", context.channel))?;

    if context.reply_to.is_empty() {
        return Err("no reply_to recipients in recovery context".to_string());
    }

    // Determine reply file path based on channel
    let (reply_path, attachments_dir) = match channel {
        Channel::Email | Channel::GoogleDocs | Channel::GoogleSheets | Channel::GoogleSlides => (
            workspace.join("reply_email_draft.html"),
            workspace.join("reply_email_attachments"),
        ),
        _ => (
            workspace.join("reply_message.txt"),
            workspace.join("reply_attachments"),
        ),
    };

    if !reply_path.exists() {
        return Err(format!("reply file not found: {}", reply_path.display()));
    }

    // Build SendReplyTask with minimal fields from recovery context
    let send_task = SendReplyTask {
        channel: channel.clone(),
        subject: "Re: Your request".to_string(),
        html_path: reply_path,
        attachments_dir,
        from: None,
        to: context.reply_to,
        cc: Vec::new(),
        bcc: Vec::new(),
        in_reply_to: None,
        references: None,
        archive_root: None,
        thread_epoch: context.thread_epoch,
        thread_state_path: None,
        employee_id: None,
        channel_metadata: ChannelMetadata::default(),
    };

    // Dispatch to appropriate outbound adapter
    let result = match channel {
        Channel::Email => execute_email_send(&send_task),
        Channel::Slack => execute_slack_send(&send_task),
        Channel::Discord => execute_discord_send(&send_task),
        Channel::Telegram => execute_telegram_send(&send_task),
        Channel::Sms => execute_sms_send(&send_task),
        Channel::BlueBubbles => execute_bluebubbles_send(&send_task),
        Channel::WhatsApp => execute_whatsapp_send(&send_task),
        Channel::WeChat => execute_wechat_send(&send_task),
        Channel::WeChatMp => execute_wechat_mp_send(&send_task),
        Channel::Lark => execute_lark_send(&send_task),
        Channel::GoogleDocs | Channel::GoogleSheets | Channel::GoogleSlides => {
            execute_google_docs_send(&send_task)
        }
        Channel::Notion => execute_notion_send(&send_task),
        Channel::Zoom => {
            warn!("Zoom channel has no direct reply - skipping outbound");
            return Ok(());
        }
    };

    result.map_err(|e| format!("outbound send failed: {}", e))?;

    info!(
        "successfully propagated results from workspace {} via {:?}",
        workspace.display(),
        channel
    );

    Ok(())
}
