//! TPM (Technical Program Manager) CLI for agent use.
//!
//! Provides commands for TPM workflows:
//! - User contact directory lookup and management
//! - Task board status aggregation
//! - Cross-channel messaging coordination
//!
//! Usage:
//!   tpm_cli get-contact --notion-user-id <id> [--workspace-id <ws>]
//!   tpm_cli list-contacts [--workspace-id <ws>]
//!   tpm_cli update-contacted --contact-id <id>

use chrono::Utc;
use mongodb::bson::oid::ObjectId;
use scheduler_module::account_store::{AccountStore, UserContact};
use scheduler_module::channel::{Channel, ChannelMetadata};
use scheduler_module::dev_task_store::{DevTask, DevTaskStore, Priority, TaskSource, TaskStatus};
use scheduler_module::notion_browser::NotionApiClient;
use scheduler_module::{ModuleExecutor, RunTaskTask, Scheduler, TaskKind};
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use uuid::Uuid;

fn main() -> ExitCode {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return ExitCode::FAILURE;
    }

    let command = &args[1];
    match command.as_str() {
        "get-contact" => cmd_get_contact(&args[2..]),
        "list-contacts" => cmd_list_contacts(&args[2..]),
        "update-contacted" => cmd_update_contacted(&args[2..]),
        "setup-board" => cmd_setup_board(&args[2..]),
        "create-task" => cmd_create_task(&args[2..]),
        "list-tasks" => cmd_list_tasks(&args[2..]),
        "sync-tasks" => cmd_sync_tasks(&args[2..]),
        "setup-tpm-cron" => cmd_setup_tpm_cron(&args[2..]),
        "help" | "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    eprintln!(
        r#"TPM CLI - Technical Program Manager tools

Usage:
  tpm_cli <command> [options]

Contact Commands:
  get-contact       Get contact info for a Notion user
    --notion-user-id <id>    Notion person ID (from task assignee)
    --workspace-id <ws>      Notion workspace ID (required)

  list-contacts     List all configured contacts
    --workspace-id <ws>      Filter by Notion workspace (optional)

  update-contacted  Mark a contact as recently contacted
    --contact-id <id>        Contact entry UUID

Task Board Commands:
  setup-board       Create a Notion task database for an organization
    --organization <org>     Organization name (e.g., "deeptutor")
    --parent-page-id <id>    Notion page to create database under
    --workspace-id <ws>      Notion workspace ID

  create-task       Create a task in MongoDB and Notion
    --organization <org>     Organization name
    --database-id <id>       Notion database ID (from setup-board)
    --workspace-id <ws>      Notion workspace ID
    --title <text>           Task title (required)
    --description <text>     Task description (required)
    --priority <p0|p1|p2|p3> Priority level (default: p2)
    --source <src>           Source: manual|user_feedback|notetaker|market_research
    --tags <tag1,tag2>       Comma-separated tags (optional)
    --assignee <email>       Assignee email (optional)

  list-tasks        List tasks from MongoDB
    --organization <org>     Organization name (required)
    --status <status>        Filter by status (optional)
    --assignee <email>       Filter by assignee (optional)

  sync-tasks        Sync task status from Notion to MongoDB
    --organization <org>     Organization name (required)
    --database-id <id>       Notion database ID
    --workspace-id <ws>      Notion workspace ID

Cron Commands:
  setup-tpm-cron    Create a cron RunTask for daily TPM sync
    --user-id <id>           User ID (must belong to organization)
    --organization <org>     Organization name
    --cron <expr>            Cron expression (default: "0 0 9 * * MON-FRI")

Environment:
  SUPABASE_DB_URL        Required for contact database access
  MONGODB_URI            Required for task database access
  ACCOUNT_ID             Account UUID (from .notion_context.json or env)
  EMPLOYEE_ID            Employee ID for Notion OAuth lookup

Context Files:
  .notion_context.json   Auto-loaded for account_id and workspace_id

Output:
  JSON to stdout on success, error message to stderr on failure.
"#
    );
}

/// Get account_id from environment or context file.
fn get_account_id() -> Option<Uuid> {
    // First try environment variable
    if let Ok(id) = env::var("ACCOUNT_ID") {
        if let Ok(uuid) = Uuid::parse_str(&id) {
            return Some(uuid);
        }
    }

    // Then try .notion_context.json
    let context_path = std::path::Path::new(".notion_context.json");
    if context_path.exists() {
        if let Ok(content) = std::fs::read_to_string(context_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(id) = json["account_id"].as_str() {
                    if let Ok(uuid) = Uuid::parse_str(id) {
                        return Some(uuid);
                    }
                }
            }
        }
    }

    None
}

/// Get workspace_id from environment or context file.
fn get_workspace_id() -> Option<String> {
    // First try argument (handled by caller)
    // Then try .notion_context.json
    let context_path = std::path::Path::new(".notion_context.json");
    if context_path.exists() {
        if let Ok(content) = std::fs::read_to_string(context_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(ws_id) = json["workspace_id"].as_str() {
                    return Some(ws_id.to_string());
                }
            }
        }
    }

    None
}

fn user_contact_to_json(contact: &UserContact) -> serde_json::Value {
    json!({
        "id": contact.id.to_string(),
        "account_id": contact.account_id.to_string(),
        "notion_user_id": contact.notion_user_id,
        "notion_workspace_id": contact.notion_workspace_id,
        "slack_user_id": contact.slack_user_id,
        "slack_workspace_id": contact.slack_workspace_id,
        "discord_user_id": contact.discord_user_id,
        "discord_guild_id": contact.discord_guild_id,
        "preferred_channel": contact.preferred_channel,
        "contact_frequency_days": contact.contact_frequency_days,
        "last_contacted_at": contact.last_contacted_at.map(|t| t.to_rfc3339()),
        "created_at": contact.created_at.to_rfc3339(),
    })
}

fn cmd_get_contact(args: &[String]) -> ExitCode {
    let mut notion_user_id: Option<String> = None;
    let mut workspace_id: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--notion-user-id" => {
                i += 1;
                notion_user_id = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(notion_user_id) = notion_user_id else {
        eprintln!("Error: --notion-user-id is required");
        return ExitCode::FAILURE;
    };

    let workspace_id = workspace_id.or_else(get_workspace_id);
    let Some(workspace_id) = workspace_id else {
        eprintln!("Error: --workspace-id is required (or set via .notion_context.json)");
        return ExitCode::FAILURE;
    };

    let Some(account_id) = get_account_id() else {
        eprintln!("Error: ACCOUNT_ID not found (check env or .notion_context.json)");
        return ExitCode::FAILURE;
    };

    let store = match AccountStore::from_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to connect to database: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match store.get_user_contact_by_notion_user(account_id, &workspace_id, &notion_user_id) {
        Ok(Some(contact)) => {
            let output = user_contact_to_json(&contact);
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            ExitCode::SUCCESS
        }
        Ok(None) => {
            let output = json!({
                "error": "not_found",
                "message": format!("No contact found for Notion user {} in workspace {}", notion_user_id, workspace_id)
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            ExitCode::SUCCESS // Not an error, just no data
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_list_contacts(args: &[String]) -> ExitCode {
    let mut workspace_id: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    // Try to get workspace_id from context if not provided
    let workspace_id = workspace_id.or_else(get_workspace_id);

    let Some(account_id) = get_account_id() else {
        eprintln!("Error: ACCOUNT_ID not found (check env or .notion_context.json)");
        return ExitCode::FAILURE;
    };

    let store = match AccountStore::from_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to connect to database: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match store.list_user_contacts(account_id, workspace_id.as_deref()) {
        Ok(contacts) => {
            let output: Vec<serde_json::Value> =
                contacts.iter().map(user_contact_to_json).collect();
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_update_contacted(args: &[String]) -> ExitCode {
    let mut contact_id: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--contact-id" => {
                i += 1;
                contact_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(contact_id) = contact_id else {
        eprintln!("Error: --contact-id is required");
        return ExitCode::FAILURE;
    };

    let contact_uuid = match Uuid::parse_str(&contact_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            eprintln!("Error: Invalid contact-id UUID format");
            return ExitCode::FAILURE;
        }
    };

    let store = match AccountStore::from_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to connect to database: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match store.update_user_contact_last_contacted(contact_uuid) {
        Ok(()) => {
            let output = json!({
                "status": "success",
                "message": format!("Updated last_contacted_at for contact {}", contact_id)
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

// =============================================================================
// Task Board Commands
// =============================================================================

/// Create a Notion task database for an organization.
fn cmd_setup_board(args: &[String]) -> ExitCode {
    let mut organization: Option<String> = None;
    let mut parent_page_id: Option<String> = None;
    let mut workspace_id: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--organization" => {
                i += 1;
                organization = args.get(i).cloned();
            }
            "--parent-page-id" => {
                i += 1;
                parent_page_id = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(organization) = organization else {
        eprintln!("Error: --organization is required");
        return ExitCode::FAILURE;
    };

    let Some(parent_page_id) = parent_page_id else {
        eprintln!("Error: --parent-page-id is required");
        return ExitCode::FAILURE;
    };

    let workspace_id = workspace_id.or_else(get_workspace_id);
    let Some(workspace_id) = workspace_id else {
        eprintln!("Error: --workspace-id is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    // Build database schema for TPM task board
    let properties = json!({
        "Name": { "title": {} },
        "Status": {
            "select": {
                "options": [
                    { "name": "Backlog", "color": "gray" },
                    { "name": "In Progress", "color": "blue" },
                    { "name": "Review", "color": "yellow" },
                    { "name": "Done", "color": "green" },
                    { "name": "Blocked", "color": "red" }
                ]
            }
        },
        "Priority": {
            "select": {
                "options": [
                    { "name": "P0", "color": "red" },
                    { "name": "P1", "color": "orange" },
                    { "name": "P2", "color": "yellow" },
                    { "name": "P3", "color": "gray" }
                ]
            }
        },
        "Assignee": { "people": {} },
        "Tags": { "multi_select": { "options": [] } },
        "Source": {
            "select": {
                "options": [
                    { "name": "User Feedback", "color": "purple" },
                    { "name": "Notetaker", "color": "blue" },
                    { "name": "Market Research", "color": "green" },
                    { "name": "Manual", "color": "gray" }
                ]
            }
        },
        "MongoDB ID": { "rich_text": {} }
    });

    let client = match NotionApiClient::from_env(&employee_id) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: Failed to create Notion client: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let db_title = format!("{} Task Board", organization);
    match client.create_database(&workspace_id, &parent_page_id, &db_title, properties) {
        Ok(db) => {
            // Update organizations.notion_database_id in Supabase
            let supabase_updated = match AccountStore::from_env() {
                Ok(store) => {
                    match store.update_organization_notion_database_id(&organization, &db.id) {
                        Ok(org) => Some(org),
                        Err(e) => {
                            eprintln!(
                                "Warning: Failed to update organizations.notion_database_id: {}",
                                e
                            );
                            eprintln!(
                                "You must manually update: UPDATE organizations SET notion_database_id = '{}' WHERE name = '{}'",
                                db.id, organization
                            );
                            None
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Could not connect to Supabase: {}", e);
                    eprintln!(
                        "You must manually update: UPDATE organizations SET notion_database_id = '{}' WHERE name = '{}'",
                        db.id, organization
                    );
                    None
                }
            };

            let output = json!({
                "success": true,
                "organization": organization,
                "database_id": db.id,
                "database_url": db.url,
                "database_title": db.title,
                "supabase_updated": supabase_updated.is_some(),
                "message": if supabase_updated.is_some() {
                    "Notion database created and organizations.notion_database_id updated"
                } else {
                    "Notion database created but organizations.notion_database_id NOT updated (see stderr)"
                }
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: Failed to create database: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// Create a task in MongoDB and Notion.
fn cmd_create_task(args: &[String]) -> ExitCode {
    let mut organization: Option<String> = None;
    let mut database_id: Option<String> = None;
    let mut workspace_id: Option<String> = None;
    let mut title: Option<String> = None;
    let mut description: Option<String> = None;
    let mut priority_str: Option<String> = None;
    let mut source_str: Option<String> = None;
    let mut tags_str: Option<String> = None;
    let mut assignee: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--organization" => {
                i += 1;
                organization = args.get(i).cloned();
            }
            "--database-id" => {
                i += 1;
                database_id = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            "--title" => {
                i += 1;
                title = args.get(i).cloned();
            }
            "--description" => {
                i += 1;
                description = args.get(i).cloned();
            }
            "--priority" => {
                i += 1;
                priority_str = args.get(i).cloned();
            }
            "--source" => {
                i += 1;
                source_str = args.get(i).cloned();
            }
            "--tags" => {
                i += 1;
                tags_str = args.get(i).cloned();
            }
            "--assignee" => {
                i += 1;
                assignee = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(organization) = organization else {
        eprintln!("Error: --organization is required");
        return ExitCode::FAILURE;
    };

    let Some(database_id) = database_id else {
        eprintln!("Error: --database-id is required");
        return ExitCode::FAILURE;
    };

    let workspace_id = workspace_id.or_else(get_workspace_id);
    let Some(workspace_id) = workspace_id else {
        eprintln!("Error: --workspace-id is required");
        return ExitCode::FAILURE;
    };

    let Some(title) = title else {
        eprintln!("Error: --title is required");
        return ExitCode::FAILURE;
    };

    let Some(description) = description else {
        eprintln!("Error: --description is required");
        return ExitCode::FAILURE;
    };

    let priority = priority_str
        .as_deref()
        .and_then(Priority::from_str)
        .unwrap_or(Priority::P2);

    let source = source_str
        .as_deref()
        .and_then(TaskSource::from_str)
        .unwrap_or(TaskSource::Manual);

    let tags: Vec<String> = tags_str
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    // 1. Create DevTask in MongoDB
    let store = match DevTaskStore::new(&organization) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to create task store: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let mut task = DevTask::new(
        organization.clone(),
        title.clone(),
        description.clone(),
        source,
    )
    .with_priority(priority)
    .with_tags(tags.clone());

    if let Some(ref a) = assignee {
        task = task.with_assignee(a.clone());
    }

    let task_id = match store.insert_task(&task) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Error: Failed to insert task: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 2. Create page in Notion database
    let client = match NotionApiClient::from_env(&employee_id) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: Failed to create Notion client: {}", e);
            // Clean up MongoDB entry
            let _ = store.delete_task(&task_id);
            return ExitCode::FAILURE;
        }
    };

    let priority_name = match priority {
        Priority::P0 => "P0",
        Priority::P1 => "P1",
        Priority::P2 => "P2",
        Priority::P3 => "P3",
    };

    let source_name = match source {
        TaskSource::UserFeedback => "User Feedback",
        TaskSource::Notetaker => "Notetaker",
        TaskSource::MarketResearch => "Market Research",
        TaskSource::Manual => "Manual",
    };

    let notion_properties = json!({
        "Name": {
            "title": [{
                "text": { "content": title }
            }]
        },
        "Status": {
            "select": { "name": "Backlog" }
        },
        "Priority": {
            "select": { "name": priority_name }
        },
        "Source": {
            "select": { "name": source_name }
        },
        "MongoDB ID": {
            "rich_text": [{
                "text": { "content": task_id.to_string() }
            }]
        }
    });

    let page = match client.create_database_page(&workspace_id, &database_id, notion_properties) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: Failed to create Notion page: {}", e);
            // Clean up MongoDB entry
            let _ = store.delete_task(&task_id);
            return ExitCode::FAILURE;
        }
    };

    // 3. Link Notion page back to MongoDB
    if let Err(e) = store.link_notion_page(&task_id, &page.id) {
        eprintln!("Warning: Failed to link Notion page to MongoDB: {}", e);
        // Don't fail - the task was created, just not linked
    }

    let output = json!({
        "success": true,
        "task_id": task_id.to_string(),
        "notion_page_id": page.id,
        "notion_url": page.url,
        "title": title,
        "priority": priority_name,
        "status": "Backlog"
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

/// List tasks from MongoDB.
fn cmd_list_tasks(args: &[String]) -> ExitCode {
    let mut organization: Option<String> = None;
    let mut status_str: Option<String> = None;
    let mut assignee: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--organization" => {
                i += 1;
                organization = args.get(i).cloned();
            }
            "--status" => {
                i += 1;
                status_str = args.get(i).cloned();
            }
            "--assignee" => {
                i += 1;
                assignee = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(organization) = organization else {
        eprintln!("Error: --organization is required");
        return ExitCode::FAILURE;
    };

    let store = match DevTaskStore::new(&organization) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to create task store: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let status = status_str.as_deref().and_then(TaskStatus::from_str);

    let tasks = match (status, assignee.as_deref()) {
        (Some(s), _) => store.list_tasks_by_status(s),
        (_, Some(a)) => store.list_tasks_by_assignee(a),
        _ => store.list_all_tasks(),
    };

    let tasks = match tasks {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: Failed to list tasks: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let output: Vec<Value> = tasks
        .iter()
        .map(|t| {
            json!({
                "id": t.id.map(|id| id.to_string()),
                "title": t.title,
                "description": t.description,
                "status": t.status.as_str(),
                "priority": t.priority.as_str(),
                "assignee": t.assignee,
                "source": t.source.as_str(),
                "tags": t.tags,
                "notion_page_id": t.notion_page_id,
                "created_at": t.created_at.to_rfc3339(),
                "updated_at": t.updated_at.to_rfc3339(),
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

/// Sync task status from Notion to MongoDB.
fn cmd_sync_tasks(args: &[String]) -> ExitCode {
    let mut organization: Option<String> = None;
    let mut database_id: Option<String> = None;
    let mut workspace_id: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--organization" => {
                i += 1;
                organization = args.get(i).cloned();
            }
            "--database-id" => {
                i += 1;
                database_id = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(organization) = organization else {
        eprintln!("Error: --organization is required");
        return ExitCode::FAILURE;
    };

    let Some(database_id) = database_id else {
        eprintln!("Error: --database-id is required");
        return ExitCode::FAILURE;
    };

    let workspace_id = workspace_id.or_else(get_workspace_id);
    let Some(workspace_id) = workspace_id else {
        eprintln!("Error: --workspace-id is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let store = match DevTaskStore::new(&organization) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to create task store: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let client = match NotionApiClient::from_env(&employee_id) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: Failed to create Notion client: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Query all items from Notion database
    let items = match client.query_database(&workspace_id, &database_id, None, None, Some(500)) {
        Ok(items) => items,
        Err(e) => {
            eprintln!("Error: Failed to query Notion database: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let mut synced = 0;
    let mut skipped = 0;
    let mut errors: Vec<String> = Vec::new();

    for item in items {
        // Extract MongoDB ID from Notion page properties
        let mongo_id_str = extract_rich_text_property(&item.properties, "MongoDB ID");
        let Some(mongo_id_str) = mongo_id_str else {
            skipped += 1;
            continue;
        };

        let Ok(object_id) = ObjectId::parse_str(&mongo_id_str) else {
            errors.push(format!("Invalid ObjectId: {}", mongo_id_str));
            continue;
        };

        // Get current task from MongoDB
        let task = match store.get_task(&object_id) {
            Ok(Some(t)) => t,
            Ok(None) => {
                errors.push(format!("Task not found: {}", mongo_id_str));
                continue;
            }
            Err(e) => {
                errors.push(format!("Failed to get task {}: {}", mongo_id_str, e));
                continue;
            }
        };

        // Extract status from Notion
        let notion_status = extract_select_property(&item.properties, "Status");
        let new_status = match notion_status.as_deref() {
            Some("Backlog") => TaskStatus::Backlog,
            Some("In Progress") => TaskStatus::InProgress,
            Some("Review") => TaskStatus::Review,
            Some("Done") => TaskStatus::Done,
            Some("Blocked") => TaskStatus::Blocked,
            _ => {
                skipped += 1;
                continue;
            }
        };

        // Update MongoDB if status changed
        if task.status != new_status {
            if let Err(e) = store.update_status(&object_id, new_status) {
                errors.push(format!("Failed to update task {}: {}", mongo_id_str, e));
                continue;
            }
            synced += 1;
        }

        // Also sync priority if changed
        let notion_priority = extract_select_property(&item.properties, "Priority");
        let new_priority = match notion_priority.as_deref() {
            Some("P0") => Some(Priority::P0),
            Some("P1") => Some(Priority::P1),
            Some("P2") => Some(Priority::P2),
            Some("P3") => Some(Priority::P3),
            _ => None,
        };

        if let Some(new_priority) = new_priority {
            if task.priority != new_priority {
                if let Err(e) = store.update_priority(&object_id, new_priority) {
                    errors.push(format!(
                        "Failed to update priority for {}: {}",
                        mongo_id_str, e
                    ));
                }
            }
        }
    }

    let output = json!({
        "success": errors.is_empty(),
        "synced": synced,
        "skipped": skipped,
        "errors": errors
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    if errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn cmd_setup_tpm_cron(args: &[String]) -> ExitCode {
    let mut user_id: Option<String> = None;
    let mut organization: Option<String> = None;
    let mut cron_expr: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--user-id" => {
                i += 1;
                user_id = args.get(i).cloned();
            }
            "--organization" => {
                i += 1;
                organization = args.get(i).cloned();
            }
            "--cron" => {
                i += 1;
                cron_expr = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(user_id) = user_id else {
        eprintln!("Error: --user-id is required");
        return ExitCode::FAILURE;
    };

    let Some(organization) = organization else {
        eprintln!("Error: --organization is required");
        return ExitCode::FAILURE;
    };

    let cron_expr = cron_expr.unwrap_or_else(|| "0 0 9 * * MON-FRI".to_string());

    let account_store = match AccountStore::from_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to connect to Supabase: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let account_uuid = match Uuid::parse_str(&user_id) {
        Ok(u) => u,
        Err(_) => {
            eprintln!("Error: Invalid user-id UUID");
            return ExitCode::FAILURE;
        }
    };

    let account = match account_store.get_account(account_uuid) {
        Ok(Some(a)) => a,
        Ok(None) => {
            eprintln!("Error: Account not found: {}", user_id);
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("Error: Failed to fetch account: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let org = match account_store.get_organization_by_name(&organization) {
        Ok(Some(o)) => o,
        Ok(None) => {
            eprintln!("Error: Organization not found: {}", organization);
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("Error: Failed to fetch organization: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if account.organization_id != Some(org.id) {
        eprintln!(
            "Error: User {} is not in organization {}",
            user_id, organization
        );
        return ExitCode::FAILURE;
    }

    let identifiers = match account_store.list_identifiers(account_uuid) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("Error: Failed to list identifiers: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let email = identifiers
        .iter()
        .find(|id| id.identifier_type == "email" && id.verified)
        .map(|id| id.identifier.clone());

    let Some(email) = email else {
        eprintln!("Error: No verified email found for user {}", user_id);
        return ExitCode::FAILURE;
    };

    let users_root = env::var("USERS_ROOT").unwrap_or_else(|_| "/tmp/users".to_string());
    let users_root_path = PathBuf::from(&users_root);

    // Use a persistent workspace dir - the same workspace is reused each cron run
    // (the scheduler stores the workspace_dir path in the task and reuses it)
    let workspace_dir = users_root_path
        .join(&user_id)
        .join("workspaces")
        .join("tpm_cron_placeholder");
    let input_email_dir = workspace_dir.join("incoming_email");

    // Create workspace and write synthetic trigger file
    if let Err(e) = std::fs::create_dir_all(&input_email_dir) {
        eprintln!("Error: Failed to create workspace directory: {}", e);
        return ExitCode::FAILURE;
    }

    let now = Utc::now();
    let synthetic_payload = json!({
        "From": "TPM Cron <cron@dowhiz.com>",
        "Subject": "TPM Sync",
        "TextBody": "This is a scheduled TPM sync. Run the daily TPM sync workflow.",
        "Date": now.to_rfc3339()
    });
    let payload_path = input_email_dir.join("postmark_payload.json");
    if let Err(e) = std::fs::write(&payload_path, synthetic_payload.to_string()) {
        eprintln!("Error: Failed to write synthetic trigger file: {}", e);
        return ExitCode::FAILURE;
    }

    // Build the RunTaskTask struct
    let run_task = RunTaskTask {
        workspace_dir: workspace_dir.clone(),
        input_email_dir: input_email_dir.clone(),
        input_attachments_dir: workspace_dir.join("incoming_attachments"),
        memory_dir: users_root_path.join(&user_id).join("memory"),
        reference_dir: workspace_dir.join("references"),
        model_name: "claude-sonnet-4-20250514".to_string(),
        runner: "codex".to_string(),
        codex_disabled: false,
        reply_to: vec![email.clone()], // Send ack/summary to user's email
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
        account_id: Some(account_uuid),
        channel_metadata: ChannelMetadata::default(),
    };

    // Load scheduler for account-level tasks (writes to MongoDB with owner_scope: user)
    let account_tasks_path = users_root_path.join(&user_id).join("state").join("tasks.db");
    if let Err(e) = std::fs::create_dir_all(account_tasks_path.parent().unwrap()) {
        eprintln!("Error: Failed to create account state directory: {}", e);
        return ExitCode::FAILURE;
    }

    let mut scheduler = match Scheduler::load(&account_tasks_path, ModuleExecutor::default()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to load scheduler: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Add the cron task
    let task_id = match scheduler.add_cron_task(&cron_expr, TaskKind::RunTask(run_task)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Error: Failed to add cron task: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let output = json!({
        "success": true,
        "task_id": task_id.to_string(),
        "user_id": user_id,
        "organization": organization,
        "email": email,
        "cron": cron_expr,
        "workspace_dir": workspace_dir.to_string_lossy(),
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

// =============================================================================
// Helper functions
// =============================================================================

/// Extract a select property value from Notion properties.
fn extract_select_property(props: &Value, name: &str) -> Option<String> {
    props[name]["select"]["name"]
        .as_str()
        .map(|s| s.to_string())
}

/// Extract a rich_text property value from Notion properties.
fn extract_rich_text_property(props: &Value, name: &str) -> Option<String> {
    props[name]["rich_text"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["plain_text"].as_str())
        .map(|s| s.to_string())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Environment check helpers
    // -------------------------------------------------------------------------

    fn require_mongodb_uri(test_name: &str) -> bool {
        match env::var("MONGODB_URI") {
            Ok(value) if !value.trim().is_empty() => true,
            _ => {
                eprintln!("Skipping {test_name}; MONGODB_URI not set.");
                false
            }
        }
    }

    fn require_notion_token(test_name: &str) -> bool {
        match env::var("NOTION_API_TOKEN") {
            Ok(value) if !value.trim().is_empty() => true,
            _ => {
                eprintln!("Skipping {test_name}; NOTION_API_TOKEN not set.");
                false
            }
        }
    }

    fn require_notion_test_page(test_name: &str) -> Option<String> {
        match env::var("NOTION_TEST_PARENT_PAGE_ID") {
            Ok(value) if !value.trim().is_empty() => Some(value),
            _ => {
                eprintln!("Skipping {test_name}; NOTION_TEST_PARENT_PAGE_ID not set.");
                None
            }
        }
    }

    // -------------------------------------------------------------------------
    // Unit tests for helper functions
    // -------------------------------------------------------------------------

    #[test]
    fn test_extract_select_property() {
        let props = json!({
            "Status": {
                "select": {
                    "name": "In Progress",
                    "color": "blue"
                }
            }
        });

        assert_eq!(
            extract_select_property(&props, "Status"),
            Some("In Progress".to_string())
        );
        assert_eq!(extract_select_property(&props, "NonExistent"), None);
    }

    #[test]
    fn test_extract_select_property_null() {
        let props = json!({
            "Status": {
                "select": null
            }
        });

        assert_eq!(extract_select_property(&props, "Status"), None);
    }

    #[test]
    fn test_extract_rich_text_property() {
        let props = json!({
            "MongoDB ID": {
                "rich_text": [{
                    "plain_text": "507f1f77bcf86cd799439011",
                    "type": "text"
                }]
            }
        });

        assert_eq!(
            extract_rich_text_property(&props, "MongoDB ID"),
            Some("507f1f77bcf86cd799439011".to_string())
        );
        assert_eq!(extract_rich_text_property(&props, "NonExistent"), None);
    }

    #[test]
    fn test_extract_rich_text_property_empty() {
        let props = json!({
            "MongoDB ID": {
                "rich_text": []
            }
        });

        assert_eq!(extract_rich_text_property(&props, "MongoDB ID"), None);
    }

    #[test]
    fn test_extract_rich_text_property_multiple() {
        // Should return first item only
        let props = json!({
            "Description": {
                "rich_text": [
                    { "plain_text": "First" },
                    { "plain_text": "Second" }
                ]
            }
        });

        assert_eq!(
            extract_rich_text_property(&props, "Description"),
            Some("First".to_string())
        );
    }

    // -------------------------------------------------------------------------
    // Unit tests for RunTaskTask and Scheduler API
    // -------------------------------------------------------------------------

    use scheduler_module::{SchedulerError, TaskExecution, TaskExecutor};
    use tempfile::TempDir;

    /// A no-op executor for testing scheduler operations without side effects.
    #[derive(Debug, Default, Clone)]
    struct NoopExecutor;

    impl TaskExecutor for NoopExecutor {
        fn execute(&self, _task: &TaskKind) -> Result<TaskExecution, SchedulerError> {
            Ok(TaskExecution::default())
        }
    }

    #[test]
    fn test_run_task_task_construction() {
        let account_id = Uuid::new_v4();
        let workspace_dir = PathBuf::from("/tmp/users/test-user/workspaces/tpm_cron_placeholder");
        let input_email_dir = workspace_dir.join("incoming_email");

        let run_task = RunTaskTask {
            workspace_dir: workspace_dir.clone(),
            input_email_dir: input_email_dir.clone(),
            input_attachments_dir: workspace_dir.join("incoming_attachments"),
            memory_dir: PathBuf::from("/tmp/users/test-user/memory"),
            reference_dir: workspace_dir.join("references"),
            model_name: "claude-sonnet-4-20250514".to_string(),
            runner: "codex".to_string(),
            codex_disabled: false,
            reply_to: vec![],
            reply_from: None,
            archive_root: None,
            thread_id: None,
            thread_epoch: None,
            thread_state_path: None,
            channel: Channel::Email,
            slack_team_id: None,
            employee_id: None,
            requester_identifier_type: Some("email".to_string()),
            requester_identifier: Some("test@example.com".to_string()),
            account_id: Some(account_id),
            channel_metadata: ChannelMetadata::default(),
        };

        // Verify key fields
        assert_eq!(run_task.channel, Channel::Email);
        assert_eq!(run_task.account_id, Some(account_id));
        assert_eq!(run_task.requester_identifier_type, Some("email".to_string()));
        assert_eq!(
            run_task.requester_identifier,
            Some("test@example.com".to_string())
        );
        assert_eq!(run_task.model_name, "claude-sonnet-4-20250514");
        assert_eq!(run_task.runner, "codex");
        assert!(!run_task.codex_disabled);
        assert!(run_task.reply_to.is_empty());
    }

    #[test]
    fn test_tpm_cron_task_has_reply_to_email() {
        // TPM cron tasks should have reply_to set to user's email for ack messages
        let account_id = Uuid::new_v4();
        let user_email = "user@example.com".to_string();
        let workspace_dir = PathBuf::from("/tmp/users/test-user/workspaces/tpm_cron_placeholder");

        let run_task = RunTaskTask {
            workspace_dir: workspace_dir.clone(),
            input_email_dir: workspace_dir.join("incoming_email"),
            input_attachments_dir: workspace_dir.join("incoming_attachments"),
            memory_dir: PathBuf::from("/tmp/users/test-user/memory"),
            reference_dir: workspace_dir.join("references"),
            model_name: "claude-sonnet-4-20250514".to_string(),
            runner: "codex".to_string(),
            codex_disabled: false,
            reply_to: vec![user_email.clone()], // TPM cron sends ack to user
            reply_from: None,
            archive_root: None,
            thread_id: None,
            thread_epoch: None,
            thread_state_path: None,
            channel: Channel::Email,
            slack_team_id: None,
            employee_id: None,
            requester_identifier_type: Some("email".to_string()),
            requester_identifier: Some(user_email.clone()),
            account_id: Some(account_id),
            channel_metadata: ChannelMetadata::default(),
        };

        // Verify reply_to contains user's email
        assert_eq!(run_task.reply_to.len(), 1);
        assert_eq!(run_task.reply_to[0], user_email);
        // Verify requester matches reply_to
        assert_eq!(run_task.requester_identifier, Some(user_email));
    }

    #[test]
    fn test_scheduler_add_cron_task() {
        if !require_mongodb_uri("test_scheduler_add_cron_task") {
            return;
        }

        let temp = TempDir::new().expect("failed to create temp dir");
        let tasks_db = temp.path().join("state").join("tasks.db");
        std::fs::create_dir_all(tasks_db.parent().unwrap()).expect("create state dir");

        let mut scheduler =
            Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load scheduler");

        // Build a minimal RunTaskTask
        let run_task = RunTaskTask {
            workspace_dir: temp.path().join("workspace"),
            input_email_dir: temp.path().join("workspace").join("incoming_email"),
            input_attachments_dir: temp.path().join("workspace").join("incoming_attachments"),
            memory_dir: temp.path().join("memory"),
            reference_dir: temp.path().join("workspace").join("references"),
            model_name: "test-model".to_string(),
            runner: "codex".to_string(),
            codex_disabled: false,
            reply_to: vec![],
            reply_from: None,
            archive_root: None,
            thread_id: None,
            thread_epoch: None,
            thread_state_path: None,
            channel: Channel::Email,
            slack_team_id: None,
            employee_id: None,
            requester_identifier_type: Some("email".to_string()),
            requester_identifier: Some("test@example.com".to_string()),
            account_id: Some(Uuid::new_v4()),
            channel_metadata: ChannelMetadata::default(),
        };

        // Add cron task
        let cron_expr = "0 0 9 * * MON-FRI"; // 9 AM weekdays
        let task_id = scheduler
            .add_cron_task(cron_expr, TaskKind::RunTask(run_task))
            .expect("add cron task");

        // Verify task was added
        assert_eq!(scheduler.tasks().len(), 1);
        let task = &scheduler.tasks()[0];
        assert_eq!(task.id, task_id);
        assert!(task.enabled);

        // Verify schedule is cron
        match &task.schedule {
            scheduler_module::Schedule::Cron { expression, next_run } => {
                assert_eq!(expression, cron_expr);
                assert!(*next_run > Utc::now());
            }
            _ => panic!("expected cron schedule"),
        }

        // Verify task kind is RunTask
        match &task.kind {
            TaskKind::RunTask(rt) => {
                assert_eq!(rt.channel, Channel::Email);
                assert_eq!(rt.model_name, "test-model");
            }
            _ => panic!("expected RunTask kind"),
        }
    }

    #[test]
    fn test_scheduler_rejects_invalid_cron() {
        if !require_mongodb_uri("test_scheduler_rejects_invalid_cron") {
            return;
        }

        let temp = TempDir::new().expect("failed to create temp dir");
        let tasks_db = temp.path().join("tasks.db");

        let mut scheduler =
            Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load scheduler");

        let run_task = RunTaskTask {
            workspace_dir: temp.path().to_path_buf(),
            input_email_dir: temp.path().join("incoming_email"),
            input_attachments_dir: temp.path().join("incoming_attachments"),
            memory_dir: temp.path().join("memory"),
            reference_dir: temp.path().join("references"),
            model_name: "test".to_string(),
            runner: "codex".to_string(),
            codex_disabled: false,
            reply_to: vec![],
            reply_from: None,
            archive_root: None,
            thread_id: None,
            thread_epoch: None,
            thread_state_path: None,
            channel: Channel::Email,
            slack_team_id: None,
            employee_id: None,
            requester_identifier_type: None,
            requester_identifier: None,
            account_id: None,
            channel_metadata: ChannelMetadata::default(),
        };

        // 5 fields instead of 6 should fail
        let result = scheduler.add_cron_task("0 0 9 * *", TaskKind::RunTask(run_task));
        assert!(result.is_err());
    }

    #[test]
    fn test_synthetic_payload_structure() {
        let now = Utc::now();
        let payload = json!({
            "From": "TPM Cron <cron@dowhiz.com>",
            "Subject": "TPM Sync",
            "TextBody": "This is a scheduled TPM sync. Run the daily TPM sync workflow.",
            "Date": now.to_rfc3339()
        });

        // Verify required fields
        assert_eq!(payload["From"], "TPM Cron <cron@dowhiz.com>");
        assert_eq!(payload["Subject"], "TPM Sync");
        assert!(payload["TextBody"].as_str().unwrap().contains("TPM sync"));
        assert!(payload["Date"].as_str().is_some());
    }

    #[test]
    fn test_workspace_paths_are_consistent() {
        let users_root = PathBuf::from("/tmp/users");
        let user_id = "abc123";

        let workspace_dir = users_root
            .join(user_id)
            .join("workspaces")
            .join("tpm_cron_placeholder");
        let input_email_dir = workspace_dir.join("incoming_email");
        let memory_dir = users_root.join(user_id).join("memory");
        let account_tasks_path = users_root.join(user_id).join("state").join("tasks.db");

        // Verify path structure
        assert_eq!(
            workspace_dir.to_string_lossy(),
            "/tmp/users/abc123/workspaces/tpm_cron_placeholder"
        );
        assert_eq!(
            input_email_dir.to_string_lossy(),
            "/tmp/users/abc123/workspaces/tpm_cron_placeholder/incoming_email"
        );
        assert_eq!(memory_dir.to_string_lossy(), "/tmp/users/abc123/memory");
        assert_eq!(
            account_tasks_path.to_string_lossy(),
            "/tmp/users/abc123/state/tasks.db"
        );
    }

    #[test]
    fn test_scheduler_persists_task_to_storage() {
        if !require_mongodb_uri("test_scheduler_persists_task_to_storage") {
            return;
        }

        let temp = TempDir::new().expect("failed to create temp dir");
        let tasks_db = temp.path().join("tasks.db");

        let account_id = Uuid::new_v4();
        let task_id;

        // Create scheduler, add task, drop scheduler
        {
            let mut scheduler =
                Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load scheduler");

            let run_task = RunTaskTask {
                workspace_dir: temp.path().join("workspace"),
                input_email_dir: temp.path().join("workspace").join("incoming_email"),
                input_attachments_dir: temp.path().join("workspace").join("incoming_attachments"),
                memory_dir: temp.path().join("memory"),
                reference_dir: temp.path().join("workspace").join("references"),
                model_name: "persisted-model".to_string(),
                runner: "codex".to_string(),
                codex_disabled: false,
                reply_to: vec![],
                reply_from: None,
                archive_root: None,
                thread_id: None,
                thread_epoch: None,
                thread_state_path: None,
                channel: Channel::Email,
                slack_team_id: None,
                employee_id: None,
                requester_identifier_type: Some("email".to_string()),
                requester_identifier: Some("persist@example.com".to_string()),
                account_id: Some(account_id),
                channel_metadata: ChannelMetadata::default(),
            };

            task_id = scheduler
                .add_cron_task("0 0 10 * * *", TaskKind::RunTask(run_task))
                .expect("add cron task");
        }

        // Reload scheduler from storage
        let scheduler =
            Scheduler::load(&tasks_db, NoopExecutor::default()).expect("reload scheduler");

        // Verify task was persisted
        assert_eq!(scheduler.tasks().len(), 1);
        let task = &scheduler.tasks()[0];
        assert_eq!(task.id, task_id);

        match &task.kind {
            TaskKind::RunTask(rt) => {
                assert_eq!(rt.model_name, "persisted-model");
                assert_eq!(rt.account_id, Some(account_id));
                assert_eq!(
                    rt.requester_identifier,
                    Some("persist@example.com".to_string())
                );
            }
            _ => panic!("expected RunTask kind"),
        }
    }

    // -------------------------------------------------------------------------
    // Integration tests (require MONGODB_URI)
    // -------------------------------------------------------------------------

    const TEST_ORG: &str = "tpm_cli_test_org";

    #[test]
    fn integration_list_tasks_empty() {
        if !require_mongodb_uri("integration_list_tasks_empty") {
            return;
        }

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");

        // List tasks (may or may not be empty, but should not error)
        let tasks = store.list_all_tasks().expect("failed to list tasks");
        // Just verify it returns a valid Vec
        let _ = tasks.len();
    }

    #[test]
    fn integration_create_and_list_task() {
        if !require_mongodb_uri("integration_create_and_list_task") {
            return;
        }

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");

        // Create a task
        let task = DevTask::new(
            TEST_ORG.to_string(),
            "TPM CLI Test Task".to_string(),
            "Created by tpm_cli integration test".to_string(),
            TaskSource::Manual,
        )
        .with_priority(Priority::P1)
        .with_tags(vec!["test".to_string(), "cli".to_string()]);

        let task_id = store.insert_task(&task).expect("failed to insert task");

        // List by status
        let backlog_tasks = store
            .list_tasks_by_status(TaskStatus::Backlog)
            .expect("failed to list by status");

        assert!(backlog_tasks.iter().any(|t| t.title == "TPM CLI Test Task"));

        // Clean up
        store.delete_task(&task_id).expect("failed to delete task");
    }

    // -------------------------------------------------------------------------
    // Integration tests (require MONGODB_URI + NOTION_API_TOKEN)
    // -------------------------------------------------------------------------

    #[test]
    fn integration_setup_board() {
        if !require_mongodb_uri("integration_setup_board") {
            return;
        }
        if !require_notion_token("integration_setup_board") {
            return;
        }
        let Some(parent_page_id) = require_notion_test_page("integration_setup_board") else {
            return;
        };

        // Set required env var for the test
        env::set_var("EMPLOYEE_ID", "test_employee");

        let client =
            NotionApiClient::from_env("test_employee").expect("failed to create Notion client");

        // Build database schema
        let properties = json!({
            "Name": { "title": {} },
            "Status": {
                "select": {
                    "options": [
                        { "name": "Backlog", "color": "gray" },
                        { "name": "In Progress", "color": "blue" },
                        { "name": "Done", "color": "green" }
                    ]
                }
            },
            "Priority": {
                "select": {
                    "options": [
                        { "name": "P0", "color": "red" },
                        { "name": "P1", "color": "orange" },
                        { "name": "P2", "color": "yellow" }
                    ]
                }
            },
            "MongoDB ID": { "rich_text": {} }
        });

        // Create database
        let db = client
            .create_database("default", &parent_page_id, "TPM CLI Test Board", properties)
            .expect("failed to create database");

        assert!(!db.id.is_empty());
        assert_eq!(db.title, "TPM CLI Test Board");
        assert!(db.properties.iter().any(|p| p.name == "Status"));
        assert!(db.properties.iter().any(|p| p.name == "Priority"));

        eprintln!("Created test database: {} ({})", db.title, db.id);
        eprintln!("NOTE: Manually delete this database after test: {}", db.url);
    }

    #[test]
    fn integration_full_task_flow() {
        if !require_mongodb_uri("integration_full_task_flow") {
            return;
        }
        if !require_notion_token("integration_full_task_flow") {
            return;
        }

        // This test requires a pre-existing Notion database
        let database_id = match env::var("NOTION_TEST_DATABASE_ID") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => {
                eprintln!("Skipping integration_full_task_flow; NOTION_TEST_DATABASE_ID not set.");
                eprintln!("Run integration_setup_board first and set the database ID.");
                return;
            }
        };

        env::set_var("EMPLOYEE_ID", "test_employee");

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");
        let client =
            NotionApiClient::from_env("test_employee").expect("failed to create Notion client");

        // 1. Create task in MongoDB
        let task = DevTask::new(
            TEST_ORG.to_string(),
            "Full Flow Test Task".to_string(),
            "Testing create-task flow".to_string(),
            TaskSource::Manual,
        )
        .with_priority(Priority::P1);

        let task_id = store.insert_task(&task).expect("failed to insert task");

        // 2. Create page in Notion
        let notion_properties = json!({
            "Name": {
                "title": [{
                    "text": { "content": "Full Flow Test Task" }
                }]
            },
            "Status": {
                "select": { "name": "Backlog" }
            },
            "Priority": {
                "select": { "name": "P1" }
            },
            "MongoDB ID": {
                "rich_text": [{
                    "text": { "content": task_id.to_string() }
                }]
            }
        });

        let page = client
            .create_database_page("default", &database_id, notion_properties)
            .expect("failed to create Notion page");

        assert!(!page.id.is_empty());

        // 3. Link back to MongoDB
        store
            .link_notion_page(&task_id, &page.id)
            .expect("failed to link notion page");

        // 4. Verify link
        let updated_task = store
            .get_task(&task_id)
            .expect("failed to get task")
            .expect("task not found");

        assert_eq!(updated_task.notion_page_id, Some(page.id.clone()));

        // 5. Clean up MongoDB (leave Notion page for manual inspection)
        store.delete_task(&task_id).expect("failed to delete task");

        eprintln!("Created Notion page: {}", page.url);
        eprintln!("NOTE: Manually delete this page after inspection.");
    }
}
