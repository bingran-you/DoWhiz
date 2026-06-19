//! Inbound handler for Notion comments/mentions.
//!
//! Processes @mentions from Notion email notifications and creates tasks.

use std::path::Path;
use std::time::Duration;

use tracing::{info, warn};
use uuid::Uuid;

use crate::account_store::{Account, AccountStore, BalanceInfo};
use crate::channel::Channel;
use crate::index_store::IndexStore;
use crate::notion_browser::models::NotionMention;
use crate::notion_store::{NotionCredential, NotionStore, NotionStoreError};
use crate::user_store::{extract_emails, UserStore};
use crate::{ModuleExecutor, RunTaskTask, Scheduler, TaskKind};

use super::super::bump_thread_state;
use super::super::config::ServiceConfig;
use super::super::default_thread_state_path;
use super::super::workspace::ensure_thread_workspace;
use crate::service::BoxError;

#[derive(Debug, Clone)]
struct AuthorizedNotionRequester {
    account: Account,
    notion_identifier: String,
    balance_hours: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum NotionAuthorAuthorizationFailure {
    MissingWorkspaceId,
    MissingAuthorId,
    AccountLookupFailed {
        notion_identifier: String,
        error: String,
    },
    NotLinked {
        notion_identifier: String,
    },
    BalanceLookupFailed {
        account_id: Uuid,
        error: String,
    },
    InsufficientBalance {
        account_id: Uuid,
        balance_hours: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum NotionCredentialResolutionFailure {
    NotFound { workspace_id: String },
    LookupFailed { workspace_id: String, error: String },
}

impl std::fmt::Display for NotionCredentialResolutionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { workspace_id } => {
                write!(f, "no OAuth credential for workspace {}", workspace_id)
            }
            Self::LookupFailed {
                workspace_id,
                error,
            } => write!(
                f,
                "failed to look up Notion credential for workspace {}: {}",
                workspace_id, error
            ),
        }
    }
}

impl std::fmt::Display for NotionAuthorAuthorizationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingWorkspaceId => write!(f, "missing workspace id"),
            Self::MissingAuthorId => write!(f, "missing author id"),
            Self::AccountLookupFailed {
                notion_identifier,
                error,
            } => write!(
                f,
                "failed to look up Notion account identifier {}: {}",
                notion_identifier, error
            ),
            Self::NotLinked { notion_identifier } => write!(
                f,
                "Notion author identifier {} is not linked to a DoWhiz account",
                notion_identifier
            ),
            Self::BalanceLookupFailed { account_id, error } => {
                write!(
                    f,
                    "failed to look up balance for account {}: {}",
                    account_id, error
                )
            }
            Self::InsufficientBalance {
                account_id,
                balance_hours,
            } => write!(
                f,
                "account {} has no available hours (balance_hours={})",
                account_id, balance_hours
            ),
        }
    }
}

/// Process an incoming Notion mention/comment.
pub(crate) fn process_notion_message(
    config: &ServiceConfig,
    user_store: &UserStore,
    index_store: &IndexStore,
    account_store: &AccountStore,
    message: &crate::channel::InboundMessage,
    raw_payload: &[u8],
) -> Result<(), BoxError> {
    // Parse the Notion mention from raw payload
    let mention: NotionMention = serde_json::from_slice(raw_payload)
        .map_err(|e| format!("Failed to parse NotionMention: {}", e))?;

    // Extract page/workspace info from metadata
    let workspace_id = message
        .metadata
        .notion_workspace_id
        .as_deref()
        .unwrap_or("unknown");
    let page_id = message
        .metadata
        .notion_page_id
        .as_deref()
        .ok_or("missing notion_page_id")?;
    let page_title = message
        .metadata
        .notion_page_title
        .as_deref()
        .unwrap_or("Untitled");

    let authorized_requester =
        match authorize_notion_requester(account_store, workspace_id, &message.sender) {
            Ok(requester) => requester,
            Err(reason) => {
                warn!(
                    workspace_id,
                    author_id = %message.sender,
                    message_id = ?message.message_id,
                    reason = %reason,
                    "skipping notion task before enqueue"
                );
                return Ok(());
            }
        };

    let requester_email = account_store
        .list_identifiers(authorized_requester.account.id)
        .map(|ids| {
            ids.into_iter()
                .find(|id| id.identifier_type == "email" && id.verified)
                .map(|id| id.identifier)
        })
        .map_err(|err| {
            warn!(
                "failed to look up email identifier for authorized Notion account {}: {}",
                authorized_requester.account.id, err
            );
            err
        })
        .ok()
        .flatten();

    info!(
        "authorized Notion requester account={} notion_identifier={} balance_hours={}",
        authorized_requester.account.id,
        authorized_requester.notion_identifier,
        authorized_requester.balance_hours
    );

    // The credential is workspace-scoped API capability. The requester account above
    // is still the billing/authorization subject.
    let store =
        NotionStore::new().map_err(|e| format!("Failed to connect to NotionStore: {}", e))?;
    let notion_credential = match resolve_notion_workspace_credential_with(workspace_id, |id| {
        match store.get_credential_by_workspace(id) {
            Ok(cred) => Ok(Some(cred)),
            Err(NotionStoreError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }) {
        Ok(cred) => {
            info!(
                "found Notion OAuth credential for workspace {} credential_account={} requester_account={}",
                workspace_id,
                cred.account_id,
                authorized_requester.account.id
            );
            cred
        }
        Err(NotionCredentialResolutionFailure::NotFound { .. }) => {
            warn!(
                    "skipping notion task before enqueue: no OAuth credential for workspace_id={} requester_account={} message_id={:?}",
                    workspace_id,
                    authorized_requester.account.id,
                    message.message_id
                );
            return Ok(());
        }
        Err(reason) => return Err(reason.to_string().into()),
    };

    // Determine user email: prefer the authorized account's verified email, fall back to
    // a workspace-scoped synthetic address for local workspace storage.
    let user_email = requester_email.unwrap_or_else(|| {
        extract_emails(&message.sender)
            .into_iter()
            .next()
            .filter(|email| email != "unknown@unknown.com")
            .unwrap_or_else(|| synthetic_notion_email(&authorized_requester.notion_identifier))
    });

    // Create or get user using the notion identifier (workspace_id:user_id format)
    // This must match the account identifier format so dashboard legacy lookup works.
    let user = user_store.get_or_create_user("notion", &authorized_requester.notion_identifier)?;
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    user_store.ensure_user_dirs(&user_paths)?;

    // Create thread key from workspace:page:notification
    let thread_key = format!("notion:{}:{}:{}", workspace_id, page_id, mention.id);

    // Ensure workspace directory exists
    let workspace = ensure_thread_workspace(
        &user_paths,
        &user.user_id,
        &thread_key,
        &config.employee_profile,
        config.skills_source_dir.as_deref(),
    )?;

    // Bump thread state for sequencing
    let thread_state_path = default_thread_state_path(&workspace);
    let thread_state =
        bump_thread_state(&thread_state_path, &thread_key, message.message_id.clone())?;

    // Save incoming comment to workspace
    append_workspace_notion_comment(
        &workspace,
        message,
        &mention,
        thread_state.last_email_seq,
        page_id,
        page_title,
    )?;

    // Write Notion context to workspace for agent
    write_notion_context_to_workspace(&workspace, &mention, page_id, page_title)?;

    // Write OAuth token to .notion_env so the agent can read/reply through Notion API.
    let env_path = workspace.join(".notion_env");
    if let Err(e) = std::fs::write(
        &env_path,
        format!("NOTION_API_TOKEN={}\n", notion_credential.access_token),
    ) {
        warn!("Failed to write .notion_env: {}", e);
    } else {
        info!(
            "Wrote Notion OAuth token to workspace for workspace_id={}",
            workspace_id
        );
    }

    // Determine model
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

    // Create RunTask
    let run_task = RunTaskTask {
        workspace_dir: workspace.clone(),
        input_email_dir: std::path::PathBuf::from("incoming_email"),
        input_attachments_dir: std::path::PathBuf::from("incoming_attachments"),
        memory_dir: std::path::PathBuf::from("memory"),
        reference_dir: std::path::PathBuf::from("references"),
        model_name,
        runner: config.employee_profile.runner.clone(),
        codex_disabled: config.codex_disabled,
        reply_to: vec![user_email.clone()],
        reply_from: config.employee_profile.addresses.first().cloned(),
        archive_root: None,
        thread_id: Some(thread_key.clone()),
        thread_epoch: Some(thread_state.epoch),
        thread_state_path: Some(thread_state_path.clone()),
        channel: Channel::Notion,
        slack_team_id: None,
        employee_id: Some(config.employee_profile.id.clone()),
        requester_identifier_type: Some("notion".to_string()),
        requester_identifier: Some(authorized_requester.notion_identifier.clone()),
        account_id: Some(authorized_requester.account.id),
        channel_metadata: Default::default(),
    };

    let run_task_for_account = run_task.clone();

    // Schedule the task
    let mut scheduler = Scheduler::load(&user_paths.tasks_db_path, ModuleExecutor::default())?;
    let task_id = if let Some(stable_task_id) = notion_message_task_id(message) {
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
                "skipping duplicate notion full-task enqueue user_id={} task_id={} message_id={:?}",
                user.user_id, stable_task_id, message.message_id
            );
        }
        stable_task_id
    } else {
        scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))?
    };
    index_store.sync_user_tasks(&user.user_id, scheduler.tasks())?;

    info!(
        "scheduler tasks enqueued user_id={} task_id={} message_id={:?} workspace={} thread_epoch={} channel=Notion",
        user.user_id,
        task_id,
        message.message_id,
        workspace.display(),
        thread_state.epoch
    );

    {
        let account = authorized_requester.account;
        info!(
            "Found account {} for Notion user {}",
            account.id, user_email
        );
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
                    match account_scheduler.add_one_shot_in_if_absent_with_id(
                        task_id,
                        Duration::from_secs(0),
                        TaskKind::RunTask(run_task_for_account),
                    ) {
                        Ok(true) => {
                            info!(
                                "also enqueued task to account-level storage account={} task_id={} channel=Notion",
                                account.id, task_id
                            );
                        }
                        Ok(false) => {
                            info!(
                                "skipping duplicate notion account-level enqueue account={} task_id={}",
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

fn authorize_notion_requester(
    account_store: &AccountStore,
    workspace_id: &str,
    author_id: &str,
) -> Result<AuthorizedNotionRequester, NotionAuthorAuthorizationFailure> {
    authorize_notion_requester_with(
        workspace_id,
        author_id,
        |notion_identifier| account_store.get_account_by_identifier("notion", notion_identifier),
        |account_id| account_store.get_balance(account_id),
    )
}

fn resolve_notion_workspace_credential_with<GetCredential, StoreError>(
    workspace_id: &str,
    mut get_credential_by_workspace: GetCredential,
) -> Result<NotionCredential, NotionCredentialResolutionFailure>
where
    GetCredential: FnMut(&str) -> Result<Option<NotionCredential>, StoreError>,
    StoreError: std::fmt::Display,
{
    let workspace_id = workspace_id.trim();
    if workspace_id.is_empty() || workspace_id.eq_ignore_ascii_case("unknown") {
        return Err(NotionCredentialResolutionFailure::NotFound {
            workspace_id: workspace_id.to_string(),
        });
    }

    get_credential_by_workspace(workspace_id)
        .map_err(|err| NotionCredentialResolutionFailure::LookupFailed {
            workspace_id: workspace_id.to_string(),
            error: err.to_string(),
        })?
        .ok_or_else(|| NotionCredentialResolutionFailure::NotFound {
            workspace_id: workspace_id.to_string(),
        })
}

fn authorize_notion_requester_with<GetAccount, GetBalance, StoreError>(
    workspace_id: &str,
    author_id: &str,
    mut get_account_by_notion_identifier: GetAccount,
    mut get_balance: GetBalance,
) -> Result<AuthorizedNotionRequester, NotionAuthorAuthorizationFailure>
where
    GetAccount: FnMut(&str) -> Result<Option<Account>, StoreError>,
    GetBalance: FnMut(Uuid) -> Result<BalanceInfo, StoreError>,
    StoreError: std::fmt::Display,
{
    let notion_identifier = notion_author_identifier(workspace_id, author_id)?;
    let account = get_account_by_notion_identifier(&notion_identifier)
        .map_err(
            |err| NotionAuthorAuthorizationFailure::AccountLookupFailed {
                notion_identifier: notion_identifier.clone(),
                error: err.to_string(),
            },
        )?
        .ok_or_else(|| NotionAuthorAuthorizationFailure::NotLinked {
            notion_identifier: notion_identifier.clone(),
        })?;

    let balance = get_balance(account.id).map_err(|err| {
        NotionAuthorAuthorizationFailure::BalanceLookupFailed {
            account_id: account.id,
            error: err.to_string(),
        }
    })?;
    if balance.balance_hours <= 0.0 {
        return Err(NotionAuthorAuthorizationFailure::InsufficientBalance {
            account_id: account.id,
            balance_hours: balance.balance_hours,
        });
    }

    Ok(AuthorizedNotionRequester {
        account,
        notion_identifier,
        balance_hours: balance.balance_hours,
    })
}

fn notion_author_identifier(
    workspace_id: &str,
    author_id: &str,
) -> Result<String, NotionAuthorAuthorizationFailure> {
    let workspace_id = workspace_id.trim();
    if workspace_id.is_empty() || workspace_id.eq_ignore_ascii_case("unknown") {
        return Err(NotionAuthorAuthorizationFailure::MissingWorkspaceId);
    }

    let author_id = author_id.trim();
    if author_id.is_empty() || author_id.eq_ignore_ascii_case("unknown") {
        return Err(NotionAuthorAuthorizationFailure::MissingAuthorId);
    }

    Ok(format!("{workspace_id}:{author_id}"))
}

fn synthetic_notion_email(notion_identifier: &str) -> String {
    let local_part: String = notion_identifier
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let local_part = local_part.trim_matches('_');
    if local_part.is_empty() {
        "notion_unknown@local".to_string()
    } else {
        format!("notion_{local_part}@local")
    }
}

fn notion_message_task_id(message: &crate::channel::InboundMessage) -> Option<Uuid> {
    let workspace_id = message.metadata.notion_workspace_id.as_deref()?.trim();
    let page_id = message.metadata.notion_page_id.as_deref()?.trim();
    let message_id = message.message_id.as_deref()?.trim();
    let thread_id = message.thread_id.trim();
    if workspace_id.is_empty()
        || page_id.is_empty()
        || message_id.is_empty()
        || thread_id.is_empty()
    {
        return None;
    }

    let dedupe_key = format!("notion:{workspace_id}:{page_id}:{thread_id}:{message_id}");
    Some(Uuid::from_bytes(md5::compute(dedupe_key.as_bytes()).0))
}

/// Write Notion context file for agent to understand how to reply.
fn write_notion_context_to_workspace(
    workspace: &Path,
    mention: &NotionMention,
    page_id: &str,
    page_title: &str,
) -> Result<(), BoxError> {
    let context = serde_json::json!({
        "channel": "notion",
        "workspace_id": mention.workspace_id,
        "workspace_name": mention.workspace_name,
        "page_id": page_id,
        "page_title": page_title,
        "comment_id": mention.comment_id,
        "block_id": mention.block_id,
        "notification_id": mention.id,
        "url": mention.url,
        "reply_instructions": "Use notion_api_cli to read the page and post your reply as a comment. The NOTION_API_TOKEN is available in .notion_env."
    });

    let context_path = workspace.join(".notion_context.json");
    std::fs::write(&context_path, serde_json::to_string_pretty(&context)?)?;

    info!("wrote .notion_context.json to workspace");
    Ok(())
}

/// Save an incoming Notion comment to the workspace.
fn append_workspace_notion_comment(
    workspace: &Path,
    message: &crate::channel::InboundMessage,
    mention: &NotionMention,
    seq: u64,
    page_id: &str,
    page_title: &str,
) -> Result<(), BoxError> {
    let incoming_dir = workspace.join("incoming_email");
    std::fs::create_dir_all(&incoming_dir)?;

    // Save the raw mention JSON
    let raw_path = incoming_dir.join(format!("{:05}_notion_mention.json", seq));
    let raw_json = serde_json::to_string_pretty(&mention)?;
    std::fs::write(&raw_path, &raw_json)?;

    // Create HTML representation for the agent
    let sender_name = message.sender_name.as_deref().unwrap_or(&message.sender);

    // Build conversation thread HTML if available
    let thread_html = if !mention.thread_context.is_empty() {
        let mut html = String::from("<h3>Previous conversation:</h3>\n");
        for comment in &mention.thread_context {
            html.push_str(&format!(
                "<div style=\"margin-bottom: 10px;\">\n<p><strong>{}:</strong></p>\n<p>{}</p>\n</div>\n",
                comment.author_name, comment.text
            ));
        }
        html
    } else {
        String::new()
    };

    let html_content = format!(
        r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Notion Comment</title></head>
<body>
<h2>@mention on: {page_title}</h2>
<p><strong>Workspace:</strong> {workspace_name}</p>
<p><strong>Page ID:</strong> {page_id}</p>
<p><strong>From:</strong> {sender_name} ({sender})</p>
<p><strong>Notification ID:</strong> {notification_id}</p>
<p><strong>URL:</strong> <a href="{url}">{url}</a></p>

<h3>Message:</h3>
<p>{comment_text}</p>

{thread_html}

<hr>
<h3>How to reply:</h3>
<p>Use <code>notion_api_cli</code> to read the page and post your reply. The NOTION_API_TOKEN is in <code>.notion_env</code>.</p>
<p>You can reference the page content from the context in this message.</p>
<hr>
<p><em>Source .notion_env and use notion_api_cli to post comments</em></p>
</body>
</html>"#,
        page_title = page_title,
        workspace_name = mention.workspace_name,
        page_id = page_id,
        sender_name = sender_name,
        sender = message.sender,
        notification_id = mention.id,
        url = mention.url,
        comment_text = mention.comment_text,
        thread_html = thread_html
    );

    let html_path = incoming_dir.join(format!("{:05}_email.html", seq));
    std::fs::write(&html_path, &html_content)?;

    // Create metadata file
    let meta_path = incoming_dir.join(format!("{:05}_notion_meta.json", seq));
    let meta = serde_json::json!({
        "channel": "notion",
        "sender": message.sender,
        "sender_name": message.sender_name,
        "workspace_id": mention.workspace_id,
        "workspace_name": mention.workspace_name,
        "page_id": page_id,
        "page_title": page_title,
        "notification_id": mention.id,
        "comment_id": mention.comment_id,
        "block_id": mention.block_id,
        "url": mention.url,
        "thread_id": message.thread_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;

    info!(
        "saved Notion mention seq={} notification_id={} to {}",
        seq,
        mention.id,
        incoming_dir.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, ChannelMetadata, InboundMessage};
    use chrono::Utc;

    fn build_message(message_id: &str) -> InboundMessage {
        InboundMessage {
            channel: Channel::Notion,
            sender: "notion-user".to_string(),
            sender_name: Some("Notion User".to_string()),
            recipient: "integration".to_string(),
            subject: Some("Notion comment".to_string()),
            text_body: Some("Please take a look".to_string()),
            html_body: None,
            thread_id: "notion:workspace-1:discussion-1".to_string(),
            message_id: Some(message_id.to_string()),
            attachments: Vec::new(),
            reply_to: Vec::new(),
            raw_payload: Vec::new(),
            metadata: ChannelMetadata {
                notion_workspace_id: Some("workspace-1".to_string()),
                notion_page_id: Some("page-1".to_string()),
                notion_comment_id: Some("comment-1".to_string()),
                ..Default::default()
            },
        }
    }

    fn test_account(account_id: Uuid) -> Account {
        Account {
            id: account_id,
            auth_user_id: Uuid::new_v4(),
            created_at: Utc::now(),
            tokens_to_hours: Some(0.0),
            purchased_hours: Some(1.0),
            organization_id: None,
            organization_accept_status: None,
        }
    }

    fn test_balance(balance_hours: f64) -> BalanceInfo {
        BalanceInfo {
            purchased_hours: balance_hours.max(0.0),
            used_hours: 0.0,
            balance_hours,
        }
    }

    fn test_notion_credential(workspace_id: &str) -> NotionCredential {
        NotionCredential {
            account_id: Uuid::new_v4(),
            workspace_id: workspace_id.to_string(),
            workspace_name: Some("Workspace".to_string()),
            access_token: "secret-token".to_string(),
            bot_id: "bot-1".to_string(),
            owner_user_id: Some("owner-1".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn notion_message_task_id_is_stable_for_duplicate_delivery() {
        let mut first = build_message("notion-comment-comment-1");
        first.raw_payload = br#"{"id":"notification-1"}"#.to_vec();
        let mut duplicate = build_message("notion-comment-comment-1");
        duplicate.raw_payload = br#"{"id":"notification-2"}"#.to_vec();

        assert_eq!(
            notion_message_task_id(&first),
            notion_message_task_id(&duplicate)
        );
    }

    #[test]
    fn notion_message_task_id_changes_for_distinct_messages() {
        let first = build_message("notion-comment-comment-1");
        let second = build_message("notion-comment-comment-2");

        assert_ne!(
            notion_message_task_id(&first),
            notion_message_task_id(&second)
        );
    }

    #[test]
    fn notion_author_identifier_matches_oauth_binding_shape() {
        assert_eq!(
            notion_author_identifier("workspace-1", "author-1").unwrap(),
            "workspace-1:author-1"
        );
    }

    #[test]
    fn notion_author_identifier_trims_workspace_and_author() {
        assert_eq!(
            notion_author_identifier(" workspace-1 ", " author-1 ").unwrap(),
            "workspace-1:author-1"
        );
    }

    #[test]
    fn notion_author_identifier_rejects_unknown_workspace_or_author() {
        assert_eq!(
            notion_author_identifier("unknown", "author-1"),
            Err(NotionAuthorAuthorizationFailure::MissingWorkspaceId)
        );
        assert_eq!(
            notion_author_identifier("workspace-1", "unknown"),
            Err(NotionAuthorAuthorizationFailure::MissingAuthorId)
        );
        assert_eq!(
            notion_author_identifier(" ", "author-1"),
            Err(NotionAuthorAuthorizationFailure::MissingWorkspaceId)
        );
        assert_eq!(
            notion_author_identifier("workspace-1", " "),
            Err(NotionAuthorAuthorizationFailure::MissingAuthorId)
        );
    }

    #[test]
    fn authorize_notion_requester_rejects_unlinked_author() {
        let result = authorize_notion_requester_with(
            "workspace-1",
            "author-1",
            |_identifier| Ok::<Option<Account>, &'static str>(None),
            |_account_id| Ok::<BalanceInfo, &'static str>(test_balance(1.0)),
        );

        assert_eq!(
            result.unwrap_err(),
            NotionAuthorAuthorizationFailure::NotLinked {
                notion_identifier: "workspace-1:author-1".to_string()
            }
        );
    }

    #[test]
    fn authorize_notion_requester_reports_account_lookup_errors() {
        let result = authorize_notion_requester_with(
            "workspace-1",
            "author-1",
            |_identifier| Err::<Option<Account>, _>("db down"),
            |_account_id| Ok::<BalanceInfo, &'static str>(test_balance(1.0)),
        );

        assert_eq!(
            result.unwrap_err(),
            NotionAuthorAuthorizationFailure::AccountLookupFailed {
                notion_identifier: "workspace-1:author-1".to_string(),
                error: "db down".to_string(),
            }
        );
    }

    #[test]
    fn authorize_notion_requester_reports_balance_lookup_errors() {
        let account_id = Uuid::new_v4();
        let account = test_account(account_id);

        let result = authorize_notion_requester_with(
            "workspace-1",
            "author-1",
            |_identifier| Ok::<Option<Account>, &'static str>(Some(account.clone())),
            |_account_id| Err::<BalanceInfo, _>("balance db down"),
        );

        assert_eq!(
            result.unwrap_err(),
            NotionAuthorAuthorizationFailure::BalanceLookupFailed {
                account_id,
                error: "balance db down".to_string(),
            }
        );
    }

    #[test]
    fn authorize_notion_requester_rejects_zero_or_negative_balance() {
        let account_id = Uuid::new_v4();
        let account = test_account(account_id);

        let zero_result = authorize_notion_requester_with(
            "workspace-1",
            "author-1",
            |_identifier| Ok::<Option<Account>, &'static str>(Some(account.clone())),
            |_account_id| Ok::<BalanceInfo, &'static str>(test_balance(0.0)),
        );
        assert_eq!(
            zero_result.unwrap_err(),
            NotionAuthorAuthorizationFailure::InsufficientBalance {
                account_id,
                balance_hours: 0.0,
            }
        );

        let negative_result = authorize_notion_requester_with(
            "workspace-1",
            "author-1",
            |_identifier| Ok::<Option<Account>, &'static str>(Some(account.clone())),
            |_account_id| Ok::<BalanceInfo, &'static str>(test_balance(-0.5)),
        );
        assert_eq!(
            negative_result.unwrap_err(),
            NotionAuthorAuthorizationFailure::InsufficientBalance {
                account_id,
                balance_hours: -0.5,
            }
        );
    }

    #[test]
    fn authorize_notion_requester_allows_linked_author_with_positive_balance() {
        let account_id = Uuid::new_v4();
        let account = test_account(account_id);

        let requester = authorize_notion_requester_with(
            "workspace-1",
            "author-1",
            |identifier| {
                assert_eq!(identifier, "workspace-1:author-1");
                Ok::<Option<Account>, &'static str>(Some(account.clone()))
            },
            |id| {
                assert_eq!(id, account_id);
                Ok::<BalanceInfo, &'static str>(test_balance(0.25))
            },
        )
        .unwrap();

        assert_eq!(requester.account.id, account_id);
        assert_eq!(requester.notion_identifier, "workspace-1:author-1");
        assert_eq!(requester.balance_hours, 0.25);
    }

    #[test]
    fn synthetic_notion_email_is_workspace_scoped_and_local_safe() {
        assert_eq!(
            synthetic_notion_email("workspace:author id"),
            "notion_workspace_author_id@local"
        );
    }

    #[test]
    fn synthetic_notion_email_handles_empty_identifier() {
        assert_eq!(synthetic_notion_email(":::"), "notion_unknown@local");
    }

    #[test]
    fn resolve_notion_workspace_credential_allows_existing_credential() {
        let credential = test_notion_credential("workspace-1");

        let resolved = resolve_notion_workspace_credential_with(" workspace-1 ", |workspace_id| {
            assert_eq!(workspace_id, "workspace-1");
            Ok::<Option<NotionCredential>, &'static str>(Some(credential.clone()))
        })
        .unwrap();

        assert_eq!(resolved.workspace_id, "workspace-1");
        assert_eq!(resolved.access_token, "secret-token");
    }

    #[test]
    fn resolve_notion_workspace_credential_rejects_missing_or_unknown_workspace() {
        assert_eq!(
            resolve_notion_workspace_credential_with(" ", |_workspace_id| {
                Ok::<Option<NotionCredential>, &'static str>(Some(test_notion_credential(
                    "workspace-1",
                )))
            })
            .unwrap_err(),
            NotionCredentialResolutionFailure::NotFound {
                workspace_id: String::new(),
            }
        );

        assert_eq!(
            resolve_notion_workspace_credential_with("unknown", |_workspace_id| {
                Ok::<Option<NotionCredential>, &'static str>(Some(test_notion_credential(
                    "workspace-1",
                )))
            })
            .unwrap_err(),
            NotionCredentialResolutionFailure::NotFound {
                workspace_id: "unknown".to_string(),
            }
        );
    }

    #[test]
    fn resolve_notion_workspace_credential_rejects_unconnected_workspace() {
        let result = resolve_notion_workspace_credential_with("workspace-1", |_workspace_id| {
            Ok::<Option<NotionCredential>, &'static str>(None)
        });

        assert_eq!(
            result.unwrap_err(),
            NotionCredentialResolutionFailure::NotFound {
                workspace_id: "workspace-1".to_string(),
            }
        );
    }

    #[test]
    fn resolve_notion_workspace_credential_reports_lookup_errors() {
        let result = resolve_notion_workspace_credential_with("workspace-1", |_workspace_id| {
            Err::<Option<NotionCredential>, _>("mongo down")
        });

        assert_eq!(
            result.unwrap_err(),
            NotionCredentialResolutionFailure::LookupFailed {
                workspace_id: "workspace-1".to_string(),
                error: "mongo down".to_string(),
            }
        );
    }
}
