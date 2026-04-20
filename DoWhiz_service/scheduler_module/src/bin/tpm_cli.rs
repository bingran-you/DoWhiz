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

use scheduler_module::account_store::{AccountStore, UserContact};
use scheduler_module::index_store::IndexStore;
use scheduler_module::notion_browser::NotionApiClient;
use scheduler_module::tpm_cron::trigger_tpm_sync;
use scheduler_module::user_store::UserStore;
use serde_json::{json, Value};
use std::env;
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
        "trigger-sync" => cmd_trigger_sync(&args[2..]),
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

  create-task       Create a task in Notion
    --organization <org>     Organization name (database_id auto-fetched from Supabase)
    --title <text>           Task title (required)
    --description <text>     Task description (optional)
    --priority <p0|p1|p2|p3> Priority level (default: p2)
    --source <src>           Source: manual|user_feedback|notetaker|market_research
    --tags <tag1,tag2>       Comma-separated tags (optional)
    --assignee <email>       Assignee email (optional)

  list-tasks        List tasks from Notion
    --organization <org>     Organization name (database_id auto-fetched from Supabase)
    --status <status>        Filter by status (optional)
    --assignee <email>       Filter by assignee (optional)

  trigger-sync      Trigger immediate TPM check-in (one-shot task)
    --user-id <uuid>         Account UUID (required)
    --organization <org>     Organization name (required)


Environment:
  SUPABASE_DB_URL        Required for organization database access
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
        }
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

/// Create a task in Notion.
fn cmd_create_task(args: &[String]) -> ExitCode {
    let mut organization: Option<String> = None;
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

    // Auto-fetch database_id from Supabase
    let database_id = match AccountStore::from_env() {
        Ok(store) => match store.get_organization_by_name(&organization) {
            Ok(Some(org)) => match org.notion_database_id {
                Some(id) => id,
                None => {
                    eprintln!("Error: No notion_database_id configured for organization '{}'. Run setup-board first.", organization);
                    return ExitCode::FAILURE;
                }
            },
            Ok(None) => {
                eprintln!(
                    "Error: Organization '{}' not found in Supabase",
                    organization
                );
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("Error: Failed to query organization: {}", e);
                return ExitCode::FAILURE;
            }
        },
        Err(e) => {
            eprintln!("Error: Could not connect to Supabase: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = get_workspace_id();
    let Some(workspace_id) = workspace_id else {
        eprintln!("Error: workspace_id is required (set via .notion_context.json)");
        return ExitCode::FAILURE;
    };

    let Some(title) = title else {
        eprintln!("Error: --title is required");
        return ExitCode::FAILURE;
    };

    let description = description.unwrap_or_default();

    let priority_name = match priority_str.as_deref() {
        Some("p0") | Some("P0") => "P0",
        Some("p1") | Some("P1") => "P1",
        Some("p3") | Some("P3") => "P3",
        _ => "P2",
    };

    let source_name = match source_str.as_deref() {
        Some("user_feedback") => "User Feedback",
        Some("notetaker") => "Notetaker",
        Some("market_research") => "Market Research",
        _ => "Manual",
    };

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

    let client = match NotionApiClient::from_env(&employee_id) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: Failed to create Notion client: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Build Notion properties
    let mut notion_properties = json!({
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
        }
    });

    // Add description if provided
    if !description.is_empty() {
        notion_properties["Description"] = json!({
            "rich_text": [{
                "text": { "content": description }
            }]
        });
    }

    // Add assignee if provided
    if let Some(ref a) = assignee {
        notion_properties["Assignee"] = json!({
            "rich_text": [{
                "text": { "content": a }
            }]
        });
    }

    // Add tags if provided
    if !tags.is_empty() {
        notion_properties["Tags"] = json!({
            "multi_select": tags.iter().map(|t| json!({"name": t})).collect::<Vec<_>>()
        });
    }

    let page = match client.create_database_page(&workspace_id, &database_id, notion_properties) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: Failed to create Notion page: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let output = json!({
        "success": true,
        "page_id": page.id,
        "page_url": page.url,
        "title": title,
        "priority": priority_name,
        "status": "Backlog"
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

/// List tasks from Notion.
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

    // Auto-fetch database_id from Supabase
    let database_id = match AccountStore::from_env() {
        Ok(store) => match store.get_organization_by_name(&organization) {
            Ok(Some(org)) => match org.notion_database_id {
                Some(id) => id,
                None => {
                    eprintln!("Error: No notion_database_id configured for organization '{}'. Run setup-board first.", organization);
                    return ExitCode::FAILURE;
                }
            },
            Ok(None) => {
                eprintln!(
                    "Error: Organization '{}' not found in Supabase",
                    organization
                );
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("Error: Failed to query organization: {}", e);
                return ExitCode::FAILURE;
            }
        },
        Err(e) => {
            eprintln!("Error: Could not connect to Supabase: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = get_workspace_id();
    let Some(workspace_id) = workspace_id else {
        eprintln!("Error: workspace_id is required (set via .notion_context.json)");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
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

    // Build filter based on status/assignee
    let filter = match (status_str.as_deref(), assignee.as_deref()) {
        (Some(status), Some(assignee)) => Some(json!({
            "and": [
                {"property": "Status", "select": {"equals": normalize_status(status)}},
                {"property": "Assignee", "rich_text": {"contains": assignee}}
            ]
        })),
        (Some(status), None) => Some(json!({
            "property": "Status",
            "select": {"equals": normalize_status(status)}
        })),
        (None, Some(assignee)) => Some(json!({
            "property": "Assignee",
            "rich_text": {"contains": assignee}
        })),
        (None, None) => None,
    };

    let pages = match client.query_database(&workspace_id, &database_id, filter, None, Some(500)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: Failed to query Notion: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let tasks: Vec<Value> = pages
        .iter()
        .map(|p| {
            json!({
                "page_id": p.id,
                "url": p.url,
                "title": extract_title_property(&p.properties, "Name").unwrap_or_default(),
                "status": extract_select_property(&p.properties, "Status").unwrap_or_default(),
                "priority": extract_select_property(&p.properties, "Priority").unwrap_or_default(),
                "assignee": extract_rich_text_property(&p.properties, "Assignee").unwrap_or_default(),
                "source": extract_select_property(&p.properties, "Source").unwrap_or_default(),
            })
        })
        .collect();

    let output = json!({
        "success": true,
        "organization": organization,
        "count": tasks.len(),
        "tasks": tasks
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

/// Normalize status string to match Notion select options.
fn normalize_status(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        "backlog" => "Backlog",
        "in_progress" | "in-progress" | "inprogress" => "In Progress",
        "review" => "Review",
        "done" => "Done",
        "blocked" => "Blocked",
        _ => "Backlog",
    }
}

/// Trigger an immediate TPM check-in (one-shot task).
fn cmd_trigger_sync(args: &[String]) -> ExitCode {
    let mut user_id: Option<String> = None;
    let mut organization: Option<String> = None;

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
            _ => {}
        }
        i += 1;
    }

    let Some(user_id_str) = user_id else {
        eprintln!("Error: --user-id is required");
        return ExitCode::FAILURE;
    };

    let user_id = match Uuid::parse_str(&user_id_str) {
        Ok(uuid) => uuid,
        Err(_) => {
            eprintln!("Error: Invalid user-id UUID format");
            return ExitCode::FAILURE;
        }
    };

    let Some(organization) = organization else {
        eprintln!("Error: --organization is required");
        return ExitCode::FAILURE;
    };

    let account_store = match AccountStore::from_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to connect to account store: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let user_store = match UserStore::new("/tmp/users.db") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to create user store: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let index_store = match IndexStore::new("/tmp/task_index.db") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to create index store: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match trigger_tpm_sync(
        &account_store,
        &user_store,
        &index_store,
        user_id,
        &organization,
    ) {
        Ok(result) => {
            let output = json!({
                "success": result.success,
                "task_id": result.task_id,
                "user_id": result.user_id,
                "organization": result.organization,
                "email": result.email,
                "workspace_dir": result.workspace_dir,
                "message": "TPM sync task queued for immediate execution"
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

/// Extract a title property value from Notion properties.
fn extract_title_property(props: &Value, name: &str) -> Option<String> {
    props[name]["title"]
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
            "Description": {
                "rich_text": [{
                    "plain_text": "Task description text",
                    "type": "text"
                }]
            }
        });

        assert_eq!(
            extract_rich_text_property(&props, "Description"),
            Some("Task description text".to_string())
        );
        assert_eq!(extract_rich_text_property(&props, "NonExistent"), None);
    }

    #[test]
    fn test_extract_rich_text_property_empty() {
        let props = json!({
            "Description": {
                "rich_text": []
            }
        });

        assert_eq!(extract_rich_text_property(&props, "Description"), None);
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
    // Integration tests (require NOTION_API_TOKEN)
    // -------------------------------------------------------------------------

    #[test]
    fn integration_setup_board() {
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
            }
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
}
