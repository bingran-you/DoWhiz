//! TPM Cron Setup Module
//!
//! Provides shared logic for setting up TPM cron jobs and one-shot triggers for organizations.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

use crate::account_store::AccountStore;
use crate::channel::{Channel, ChannelMetadata};
use crate::index_store::IndexStore;
use crate::user_store::UserStore;
use crate::{ModuleExecutor, RunTaskTask, Scheduler, TaskKind};

/// Result of setting up a TPM cron job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupTpmCronResult {
    pub success: bool,
    pub task_id: String,
    pub user_id: String,
    pub organization: String,
    pub email: String,
    pub cron: String,
    pub workspace_dir: String,
}

/// Result of triggering a one-shot TPM sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerTpmSyncResult {
    pub success: bool,
    pub task_id: String,
    pub user_id: String,
    pub organization: String,
    pub email: String,
    pub workspace_dir: String,
}

/// Error type for TPM cron setup
#[derive(Debug, thiserror::Error)]
pub enum TpmCronError {
    #[error("Failed to connect to account store: {0}")]
    AccountStoreConnection(String),

    #[error("Invalid user ID: {0}")]
    InvalidUserId(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Organization not found: {0}")]
    OrganizationNotFound(String),

    #[error("User {0} is not in organization {1}")]
    UserNotInOrganization(String, String),

    #[error("No verified email found for user {0}")]
    NoVerifiedEmail(String),

    #[error("Failed to create workspace directory: {0}")]
    WorkspaceCreation(String),

    #[error("Failed to write synthetic trigger file: {0}")]
    TriggerFileWrite(String),

    #[error("Failed to load scheduler: {0}")]
    SchedulerLoad(String),

    #[error("Failed to add cron task: {0}")]
    CronTaskAdd(String),

    #[error("Failed to list identifiers: {0}")]
    ListIdentifiers(String),

    #[error("Failed to fetch account: {0}")]
    FetchAccount(String),

    #[error("Failed to fetch organization: {0}")]
    FetchOrganization(String),

    #[error("Failed to add one-shot task: {0}")]
    OneShotTaskAdd(String),

    #[error("Failed to sync to index store: {0}")]
    IndexStoreSync(String),

    #[error("Failed to get or create email user: {0}")]
    UserStoreLookup(String),

    #[error("Failed to ensure user directories: {0}")]
    UserDirsCreation(String),
}

/// Set up a TPM cron job for a user in an organization.
///
/// This creates a daily cron task that triggers the TPM sync workflow.
///
/// # Arguments
/// * `account_store` - The account store to use for lookups
/// * `user_store` - The user store to get/create email user
/// * `index_store` - The index store to sync tasks to
/// * `user_id` - The user's account UUID
/// * `organization` - The organization name
/// * `cron_expr` - Optional cron expression (defaults to "0 0 9 * * MON-FRI")
///
/// # Returns
/// * `Ok(SetupTpmCronResult)` on success with task details
/// * `Err(TpmCronError)` on failure
pub fn setup_tpm_cron(
    account_store: &AccountStore,
    user_store: &UserStore,
    index_store: &IndexStore,
    user_id: Uuid,
    organization: &str,
    cron_expr: Option<&str>,
) -> Result<SetupTpmCronResult, TpmCronError> {
    let cron_expr = cron_expr.unwrap_or("0 0 9 * * MON-FRI");
    let user_id_str = user_id.to_string();

    info!(
        "setup_tpm_cron: user_id={}, organization={}, cron={}",
        user_id, organization, cron_expr
    );

    // Fetch and verify account
    let account = account_store
        .get_account(user_id)
        .map_err(|e| TpmCronError::FetchAccount(e.to_string()))?
        .ok_or_else(|| TpmCronError::AccountNotFound(user_id_str.clone()))?;

    // Fetch and verify organization
    let org = account_store
        .get_organization_by_name(organization)
        .map_err(|e| TpmCronError::FetchOrganization(e.to_string()))?
        .ok_or_else(|| TpmCronError::OrganizationNotFound(organization.to_string()))?;

    // Verify user is in this organization
    if account.organization_id != Some(org.id) {
        return Err(TpmCronError::UserNotInOrganization(
            user_id_str.clone(),
            organization.to_string(),
        ));
    }

    info!(
        "setup_tpm_cron: verified account_id={}, org={}, org_id={}",
        account.id, organization, org.id
    );

    // Get verified email
    let identifiers = account_store
        .list_identifiers(user_id)
        .map_err(|e| TpmCronError::ListIdentifiers(e.to_string()))?;

    let email = identifiers
        .iter()
        .find(|id| id.identifier_type == "email" && id.verified)
        .map(|id| id.identifier.clone())
        .ok_or_else(|| TpmCronError::NoVerifiedEmail(user_id_str.clone()))?;

    // Get or create email user in UserStore (same pattern as email handler)
    let email_user = user_store
        .get_or_create_user("email", &email)
        .map_err(|e| TpmCronError::UserStoreLookup(e.to_string()))?;

    // Use same path structure as email handler
    let users_root = std::env::var("USERS_ROOT").unwrap_or_else(|_| "/tmp/users".to_string());
    let users_root_path = PathBuf::from(&users_root);
    let user_paths = user_store.user_paths(&users_root_path, &email_user.user_id);

    // Ensure user directories exist (same as email handler)
    user_store
        .ensure_user_dirs(&user_paths)
        .map_err(|e| TpmCronError::UserDirsCreation(e.to_string()))?;

    let workspace_dir = user_paths.workspaces_root.join("tpm_cron_placeholder");

    // Create all workspace directories required by RunTaskTask validation
    for subdir in [
        "incoming_email",
        "incoming_attachments",
        "memory",
        "references",
    ] {
        std::fs::create_dir_all(workspace_dir.join(subdir))
            .map_err(|e| TpmCronError::WorkspaceCreation(e.to_string()))?;
    }

    // Write synthetic trigger file
    let now = Utc::now();
    let synthetic_payload = json!({
        "From": "TPM Cron <cron@dowhiz.com>",
        "Subject": format!("TPM Sync for {}", organization),
        "TextBody": format!("This is a scheduled TPM sync for organization '{}'. Run the daily TPM sync workflow for this organization.", organization),
        "Date": now.to_rfc3339()
    });
    let payload_path = workspace_dir.join("incoming_email/postmark_payload.json");
    std::fs::write(&payload_path, synthetic_payload.to_string())
        .map_err(|e| TpmCronError::TriggerFileWrite(e.to_string()))?;

    // Build the RunTaskTask struct (paths must be relative to workspace_dir)
    let run_task = RunTaskTask {
        workspace_dir: workspace_dir.clone(),
        input_email_dir: PathBuf::from("incoming_email"),
        input_attachments_dir: PathBuf::from("incoming_attachments"),
        memory_dir: PathBuf::from("memory"),
        reference_dir: PathBuf::from("references"),
        model_name: "claude-sonnet-4-20250514".to_string(),
        runner: "codex".to_string(),
        codex_disabled: false,
        reply_to: vec![email.clone()],
        reply_from: None,
        archive_root: None,
        thread_id: None,
        thread_epoch: None,
        thread_state_path: None,
        channel: Channel::Email,
        slack_team_id: None,
        employee_id: None,
        requester_identifier_type: Some("email".to_string()),
        requester_identifier: Some(email.clone()),
        account_id: Some(user_id),
        channel_metadata: ChannelMetadata::default(),
    };

    // Load scheduler from email user's tasks.db (same path as email handler)
    let mut scheduler = Scheduler::load(&user_paths.tasks_db_path, ModuleExecutor::default())
        .map_err(|e| TpmCronError::SchedulerLoad(e.to_string()))?;

    let task_id = scheduler
        .add_cron_task(cron_expr, TaskKind::RunTask(run_task))
        .map_err(|e| TpmCronError::CronTaskAdd(e.to_string()))?;

    // Sync to index store using email user_id (same as email handler)
    index_store
        .sync_user_tasks(&email_user.user_id, scheduler.tasks())
        .map_err(|e| TpmCronError::IndexStoreSync(e.to_string()))?;

    info!(
        "setup_tpm_cron: cron task added and synced, task_id={}, email_user_id={}",
        task_id, email_user.user_id
    );

    Ok(SetupTpmCronResult {
        success: true,
        task_id: task_id.to_string(),
        user_id: email_user.user_id,
        organization: organization.to_string(),
        email,
        cron: cron_expr.to_string(),
        workspace_dir: workspace_dir.to_string_lossy().to_string(),
    })
}

/// Trigger an immediate one-shot TPM sync for a user in an organization.
///
/// This creates a task that runs immediately (0 delay) and syncs to the index store
/// so the scheduler worker can pick it up.
///
/// # Arguments
/// * `account_store` - The account store to use for lookups
/// * `user_store` - The user store to get/create email user
/// * `index_store` - The index store to sync tasks to
/// * `user_id` - The user's account UUID
/// * `organization` - The organization name
///
/// # Returns
/// * `Ok(TriggerTpmSyncResult)` on success with task details
/// * `Err(TpmCronError)` on failure
pub fn trigger_tpm_sync(
    account_store: &AccountStore,
    user_store: &UserStore,
    index_store: &IndexStore,
    user_id: Uuid,
    organization: &str,
) -> Result<TriggerTpmSyncResult, TpmCronError> {
    let user_id_str = user_id.to_string();

    info!(
        "trigger_tpm_sync: user_id={}, organization={}",
        user_id, organization
    );

    // Fetch and verify account
    let account = account_store
        .get_account(user_id)
        .map_err(|e| TpmCronError::FetchAccount(e.to_string()))?
        .ok_or_else(|| TpmCronError::AccountNotFound(user_id_str.clone()))?;

    // Fetch and verify organization
    let org = account_store
        .get_organization_by_name(organization)
        .map_err(|e| TpmCronError::FetchOrganization(e.to_string()))?
        .ok_or_else(|| TpmCronError::OrganizationNotFound(organization.to_string()))?;

    // Verify user is in this organization
    if account.organization_id != Some(org.id) {
        return Err(TpmCronError::UserNotInOrganization(
            user_id_str.clone(),
            organization.to_string(),
        ));
    }

    info!(
        "trigger_tpm_sync: verified account_id={}, org={}, org_id={}",
        account.id, organization, org.id
    );

    // Get verified email
    let identifiers = account_store
        .list_identifiers(user_id)
        .map_err(|e| TpmCronError::ListIdentifiers(e.to_string()))?;

    let email = identifiers
        .iter()
        .find(|id| id.identifier_type == "email" && id.verified)
        .map(|id| id.identifier.clone())
        .ok_or_else(|| TpmCronError::NoVerifiedEmail(user_id_str.clone()))?;

    // Get or create email user in UserStore (same pattern as email handler)
    let email_user = user_store
        .get_or_create_user("email", &email)
        .map_err(|e| TpmCronError::UserStoreLookup(e.to_string()))?;

    // Use same path structure as email handler
    let users_root = std::env::var("USERS_ROOT").unwrap_or_else(|_| "/tmp/users".to_string());
    let users_root_path = PathBuf::from(&users_root);
    let user_paths = user_store.user_paths(&users_root_path, &email_user.user_id);

    // Ensure user directories exist (same as email handler)
    user_store
        .ensure_user_dirs(&user_paths)
        .map_err(|e| TpmCronError::UserDirsCreation(e.to_string()))?;

    // Use unique workspace per trigger to avoid blocking on concurrent executions
    let trigger_id = Uuid::new_v4();
    let workspace_dir = user_paths
        .workspaces_root
        .join(format!("tpm_trigger_{}", trigger_id));

    // Create all workspace directories required by RunTaskTask validation
    for subdir in [
        "incoming_email",
        "incoming_attachments",
        "memory",
        "references",
    ] {
        std::fs::create_dir_all(workspace_dir.join(subdir))
            .map_err(|e| TpmCronError::WorkspaceCreation(e.to_string()))?;
    }

    // Write synthetic trigger file
    let now = Utc::now();
    let synthetic_payload = json!({
        "From": "TPM Trigger <trigger@dowhiz.com>",
        "Subject": format!("TPM Sync for {} (Manual Trigger)", organization),
        "TextBody": format!("This is a manually triggered TPM sync for organization '{}'. Run the daily TPM sync workflow for this organization.", organization),
        "Date": now.to_rfc3339()
    });
    let payload_path = workspace_dir.join("incoming_email/postmark_payload.json");
    std::fs::write(&payload_path, synthetic_payload.to_string())
        .map_err(|e| TpmCronError::TriggerFileWrite(e.to_string()))?;

    // Build the RunTaskTask struct (paths must be relative to workspace_dir)
    let run_task = RunTaskTask {
        workspace_dir: workspace_dir.clone(),
        input_email_dir: PathBuf::from("incoming_email"),
        input_attachments_dir: PathBuf::from("incoming_attachments"),
        memory_dir: PathBuf::from("memory"),
        reference_dir: PathBuf::from("references"),
        model_name: "claude-sonnet-4-20250514".to_string(),
        runner: "codex".to_string(),
        codex_disabled: false,
        reply_to: vec![email.clone()],
        reply_from: None,
        archive_root: None,
        thread_id: None,
        thread_epoch: None,
        thread_state_path: None,
        channel: Channel::Email,
        slack_team_id: None,
        employee_id: None,
        requester_identifier_type: Some("email".to_string()),
        requester_identifier: Some(email.clone()),
        account_id: Some(user_id),
        channel_metadata: ChannelMetadata::default(),
    };

    // Clone run_task for dual-write to account storage (for frontend visibility)
    let run_task_for_account = run_task.clone();

    // Load scheduler from email user's tasks.db (for worker execution)
    let mut scheduler = Scheduler::load(&user_paths.tasks_db_path, ModuleExecutor::default())
        .map_err(|e| TpmCronError::SchedulerLoad(e.to_string()))?;

    let task_id = scheduler
        .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))
        .map_err(|e| TpmCronError::OneShotTaskAdd(e.to_string()))?;

    // Sync to index store using email user_id (worker finds tasks here)
    index_store
        .sync_user_tasks(&email_user.user_id, scheduler.tasks())
        .map_err(|e| TpmCronError::IndexStoreSync(e.to_string()))?;

    info!(
        "trigger_tpm_sync: one-shot task added and synced, task_id={}, email_user_id={}",
        task_id, email_user.user_id
    );

    // Dual-write: Also write to account-level storage for frontend visibility
    // (same pattern as discord.rs, wechat.rs, etc.)
    let account_tasks_dir = users_root_path.join(user_id.to_string()).join("state");
    if let Err(err) = std::fs::create_dir_all(&account_tasks_dir) {
        tracing::warn!(
            "failed to create account tasks dir for {}: {}",
            user_id,
            err
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
                            "trigger_tpm_sync: also enqueued to account storage account_id={} task_id={}",
                            user_id, task_id
                        );
                    }
                    Err(err) => {
                        tracing::warn!(
                            "failed to add task to account scheduler for {}: {}",
                            user_id,
                            err
                        );
                    }
                }
            }
            Err(err) => {
                tracing::warn!("failed to load account scheduler for {}: {}", user_id, err);
            }
        }
    }

    Ok(TriggerTpmSyncResult {
        success: true,
        task_id: task_id.to_string(),
        user_id: email_user.user_id,
        organization: organization.to_string(),
        email,
        workspace_dir: workspace_dir.to_string_lossy().to_string(),
    })
}
