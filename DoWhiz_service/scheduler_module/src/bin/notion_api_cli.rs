//! Notion API CLI for agent use.
//!
//! Provides commands for agents to interact with Notion pages and comments
//! without browser automation.
//!
//! Usage:
//!   notion_api_cli read-page --page-id <id> [--workspace-id <ws>]
//!   notion_api_cli get-comments --page-id <id> [--workspace-id <ws>]
//!   notion_api_cli reply --discussion-id <id> --content <text> [--workspace-id <ws>]
//!   notion_api_cli create-comment --page-id <id> --content <text> [--workspace-id <ws>]

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    dotenvy::dotenv().ok();
    // Load .notion_env for channel-agnostic Notion token support
    // (written by codex.rs when user has linked Notion account)
    dotenvy::from_filename(".notion_env").ok();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return ExitCode::FAILURE;
    }

    let command = &args[1];
    match command.as_str() {
        "read-page" => cmd_read_page(&args[2..]),
        "get-comments" => cmd_get_comments(&args[2..]),
        "reply" => cmd_reply(&args[2..]),
        "create-comment" => cmd_create_comment(&args[2..]),
        "search" => cmd_search(&args[2..]),
        "create-page" => cmd_create_page(&args[2..]),
        "append-blocks" => cmd_append_blocks(&args[2..]),
        "get-database" => cmd_get_database(&args[2..]),
        "query-database" => cmd_query_database(&args[2..]),
        "update-page" => cmd_update_page(&args[2..]),
        "archive-page" => cmd_archive_page(&args[2..]),
        "list-pages" => cmd_list_pages(&args[2..]),
        "get-children" => cmd_get_children(&args[2..]),
        "bulk-read" => cmd_bulk_read(&args[2..]),
        "export-workspace" => cmd_export_workspace(&args[2..]),
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
        r#"Notion API CLI

Usage:
  notion_api_cli <command> [options]

Commands:
  read-page        Read a Notion page and its content
    --page-id <id>       Page ID (32-char UUID without dashes)
    --workspace-id <ws>  Workspace ID (optional, auto-detected if not provided)

  get-comments     Get comments on a page
    --page-id <id>       Page ID
    --workspace-id <ws>  Workspace ID (optional)

  reply            Reply to an existing comment thread
    --discussion-id <id> Discussion ID from the comment thread
    --content <text>     Reply content
    --workspace-id <ws>  Workspace ID (optional)

  create-comment   Create a new comment on a page
    --page-id <id>       Page ID
    --content <text>     Comment content
    --workspace-id <ws>  Workspace ID (optional)

  search           Search for pages
    --query <text>       Search query
    --workspace-id <ws>  Workspace ID (optional)

  create-page      Create a new page under a parent page
    --parent-id <id>     Parent page ID
    --title <text>       Page title
    --content <text>     Initial paragraph content (optional)
    --workspace-id <ws>  Workspace ID (optional)

  append-blocks    Append content blocks to a page
    --page-id <id>       Page or block ID to append to
    --blocks <json>      JSON array of blocks (see below)
    --workspace-id <ws>  Workspace ID (optional)

  get-database     Get database schema and properties
    --database-id <id>   Database ID
    --workspace-id <ws>  Workspace ID (optional)

  query-database   Query items from a database
    --database-id <id>   Database ID
    --filter <json>      Filter JSON (optional)
    --sorts <json>       Sorts JSON array (optional)
    --limit <n>          Max results (optional, default 100)
    --workspace-id <ws>  Workspace ID (optional)

  update-page      Update page properties
    --page-id <id>       Page ID
    --properties <json>  Properties JSON
    --workspace-id <ws>  Workspace ID (optional)

  archive-page     Archive (soft delete) a page
    --page-id <id>       Page ID
    --workspace-id <ws>  Workspace ID (optional)

  list-pages       List all pages accessible to the integration
    --limit <n>          Max results (optional, default 100)
    --workspace-id <ws>  Workspace ID (optional)

  get-children     Get child pages under a parent page
    --parent-id <id>     Parent page ID
    --workspace-id <ws>  Workspace ID (optional)

  bulk-read        Read multiple pages at once
    --page-ids <ids>     Comma-separated page IDs
    --workspace-id <ws>  Workspace ID (optional)

  export-workspace Export entire workspace as JSON
    --max-pages <n>      Max pages to export (optional, default 100)
    --output <file>      Output file path (optional, stdout if not specified)
    --workspace-id <ws>  Workspace ID (optional)

Block Types for append-blocks:
  [
    {{"type": "paragraph", "text": "..."}},
    {{"type": "heading_1", "text": "..."}},
    {{"type": "heading_2", "text": "..."}},
    {{"type": "heading_3", "text": "..."}},
    {{"type": "bulleted_list_item", "text": "..."}},
    {{"type": "numbered_list_item", "text": "..."}},
    {{"type": "to_do", "text": "...", "checked": false}},
    {{"type": "quote", "text": "..."}},
    {{"type": "callout", "text": "...", "emoji": "..."}},
    {{"type": "code", "text": "...", "language": "python"}},
    {{"type": "divider"}}
  ]

Environment:
  EMPLOYEE_ID              Required for OAuth token lookup
  NOTION_DEFAULT_WORKSPACE Default workspace ID if not specified

Security:
  When .notion_context.json exists (agent task context), the workspace_id
  from that file is ENFORCED for user isolation. The --workspace-id parameter
  is ignored to prevent cross-workspace access.

Output:
  JSON to stdout on success, error message to stderr on failure.
"#
    );
}

/// Get the enforced workspace ID from .notion_context.json if it exists.
/// This ensures user isolation - agents can only access the workspace
/// that triggered the current task, not other workspaces the employee
/// may have access to.
fn get_enforced_workspace_id() -> Option<String> {
    let context_path = std::path::Path::new(".notion_context.json");
    if !context_path.exists() {
        return None;
    }

    let content = match std::fs::read_to_string(context_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    let ctx: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return None,
    };

    ctx["workspace_id"].as_str().map(|s| s.to_string())
}

/// Resolve workspace ID with user isolation enforcement.
/// Priority:
/// 1. .notion_context.json workspace_id (ENFORCED if exists)
/// 2. --workspace-id parameter (only if no context file)
/// 3. NOTION_DEFAULT_WORKSPACE env var
/// 4. "default" fallback
fn resolve_workspace_id(param_workspace_id: Option<String>) -> String {
    // Check for enforced workspace from task context
    if let Some(enforced_ws) = get_enforced_workspace_id() {
        if let Some(ref param_ws) = param_workspace_id {
            if param_ws != &enforced_ws {
                eprintln!(
                    "Warning: --workspace-id {} ignored. Using enforced workspace {} from .notion_context.json for user isolation.",
                    param_ws, enforced_ws
                );
            }
        }
        return enforced_ws;
    }

    // No context file - use parameter or fallback
    param_workspace_id
        .or_else(|| env::var("NOTION_DEFAULT_WORKSPACE").ok())
        .unwrap_or_else(|| "default".to_string())
}

fn cmd_read_page(args: &[String]) -> ExitCode {
    let (page_id, workspace_id) = match parse_page_args(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let Some(page_id) = page_id else {
        eprintln!("Error: --page-id is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    // Use the API client
    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.get_page_content(&workspace_id, &page_id) {
            Ok(content) => {
                let output = serde_json::json!({
                    "page": {
                        "id": content.page.id,
                        "title": content.page.title,
                        "url": content.page.url,
                        "created_time": content.page.created_time,
                        "last_edited_time": content.page.last_edited_time,
                    },
                    "blocks": content.blocks.iter().map(|b| {
                        serde_json::json!({
                            "id": b.id,
                            "type": b.block_type,
                            "text": b.text_content,
                            "has_children": b.has_children,
                        })
                    }).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_get_comments(args: &[String]) -> ExitCode {
    let (page_id, workspace_id) = match parse_page_args(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let Some(page_id) = page_id else {
        eprintln!("Error: --page-id is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.get_comments(&workspace_id, &page_id) {
            Ok(comments) => {
                let output: Vec<_> = comments
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "discussion_id": c.discussion_id,
                            "parent_id": c.parent_id,
                            "created_by": {
                                "id": c.created_by.id,
                                "name": c.created_by.name,
                            },
                            "created_time": c.created_time,
                            "text": c.plain_text(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_reply(args: &[String]) -> ExitCode {
    let mut discussion_id = None;
    let mut content = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--discussion-id" => {
                i += 1;
                discussion_id = args.get(i).cloned();
            }
            "--content" => {
                i += 1;
                content = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(discussion_id) = discussion_id else {
        eprintln!("Error: --discussion-id is required");
        return ExitCode::FAILURE;
    };

    let Some(content) = content else {
        eprintln!("Error: --content is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.reply_to_comment(&workspace_id, &discussion_id, &content) {
            Ok(comment) => {
                let output = serde_json::json!({
                    "success": true,
                    "comment_id": comment.id,
                    "discussion_id": comment.discussion_id,
                    "text": comment.plain_text(),
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_create_comment(args: &[String]) -> ExitCode {
    let mut page_id = None;
    let mut content = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--page-id" => {
                i += 1;
                page_id = args.get(i).cloned();
            }
            "--content" => {
                i += 1;
                content = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(page_id) = page_id else {
        eprintln!("Error: --page-id is required");
        return ExitCode::FAILURE;
    };

    let Some(content) = content else {
        eprintln!("Error: --content is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.create_comment(&workspace_id, &page_id, &content) {
            Ok(comment) => {
                let output = serde_json::json!({
                    "success": true,
                    "comment_id": comment.id,
                    "discussion_id": comment.discussion_id,
                    "page_id": comment.parent_id,
                    "text": comment.plain_text(),
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_search(args: &[String]) -> ExitCode {
    let mut query = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--query" => {
                i += 1;
                query = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(query) = query else {
        eprintln!("Error: --query is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.search_pages(&workspace_id, &query) {
            Ok(pages) => {
                let output: Vec<_> = pages
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id,
                            "title": p.title,
                            "url": p.url,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_create_page(args: &[String]) -> ExitCode {
    let mut parent_id = None;
    let mut title = None;
    let mut content = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--parent-id" => {
                i += 1;
                parent_id = args.get(i).cloned();
            }
            "--title" => {
                i += 1;
                title = args.get(i).cloned();
            }
            "--content" => {
                i += 1;
                content = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(parent_id) = parent_id else {
        eprintln!("Error: --parent-id is required");
        return ExitCode::FAILURE;
    };

    let Some(title) = title else {
        eprintln!("Error: --title is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    // Build initial content blocks if provided
    let content_blocks = content.map(|text| {
        vec![scheduler_module::notion_browser::BlockInput::Paragraph(
            text,
        )]
    });

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.create_page(&workspace_id, &parent_id, &title, content_blocks) {
            Ok(page) => {
                let output = serde_json::json!({
                    "success": true,
                    "page": {
                        "id": page.id,
                        "title": page.title,
                        "url": page.url,
                        "created_time": page.created_time,
                    }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_append_blocks(args: &[String]) -> ExitCode {
    let mut page_id = None;
    let mut blocks_json = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--page-id" => {
                i += 1;
                page_id = args.get(i).cloned();
            }
            "--blocks" => {
                i += 1;
                blocks_json = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(page_id) = page_id else {
        eprintln!("Error: --page-id is required");
        return ExitCode::FAILURE;
    };

    let Some(blocks_json) = blocks_json else {
        eprintln!("Error: --blocks is required (JSON array)");
        return ExitCode::FAILURE;
    };

    // Parse blocks JSON
    let blocks: Vec<serde_json::Value> = match serde_json::from_str(&blocks_json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing blocks JSON: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Convert to BlockInput
    let block_inputs: Vec<scheduler_module::notion_browser::BlockInput> = blocks
        .into_iter()
        .filter_map(|b| parse_block_input(&b))
        .collect();

    if block_inputs.is_empty() {
        eprintln!("Error: No valid blocks found in JSON");
        return ExitCode::FAILURE;
    }

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.append_blocks(&workspace_id, &page_id, block_inputs) {
            Ok(blocks) => {
                let output = serde_json::json!({
                    "success": true,
                    "blocks_created": blocks.len(),
                    "blocks": blocks.iter().map(|b| {
                        serde_json::json!({
                            "id": b.id,
                            "type": b.block_type,
                        })
                    }).collect::<Vec<_>>()
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_get_database(args: &[String]) -> ExitCode {
    let mut database_id = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
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

    let Some(database_id) = database_id else {
        eprintln!("Error: --database-id is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.get_database(&workspace_id, &database_id) {
            Ok(db) => {
                let output = serde_json::json!({
                    "id": db.id,
                    "title": db.title,
                    "url": db.url,
                    "properties": db.properties.iter().map(|p| {
                        serde_json::json!({
                            "name": p.name,
                            "type": p.property_type,
                            "id": p.id,
                        })
                    }).collect::<Vec<_>>()
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_query_database(args: &[String]) -> ExitCode {
    let mut database_id = None;
    let mut filter_json = None;
    let mut sorts_json = None;
    let mut limit = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--database-id" => {
                i += 1;
                database_id = args.get(i).cloned();
            }
            "--filter" => {
                i += 1;
                filter_json = args.get(i).cloned();
            }
            "--sorts" => {
                i += 1;
                sorts_json = args.get(i).cloned();
            }
            "--limit" => {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok());
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(database_id) = database_id else {
        eprintln!("Error: --database-id is required");
        return ExitCode::FAILURE;
    };

    let filter: Option<serde_json::Value> = filter_json.and_then(|s| serde_json::from_str(&s).ok());
    let sorts: Option<Vec<serde_json::Value>> =
        sorts_json.and_then(|s| serde_json::from_str(&s).ok());

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => {
            match client.query_database(&workspace_id, &database_id, filter, sorts, limit) {
                Ok(items) => {
                    let output: Vec<_> = items
                        .iter()
                        .map(|item| {
                            serde_json::json!({
                                "id": item.id,
                                "title": item.title,
                                "url": item.url,
                                "properties": item.properties,
                                "created_time": item.created_time,
                                "last_edited_time": item.last_edited_time,
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("API Error: {}", e);
                    ExitCode::FAILURE
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_update_page(args: &[String]) -> ExitCode {
    let mut page_id = None;
    let mut properties_json = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--page-id" => {
                i += 1;
                page_id = args.get(i).cloned();
            }
            "--properties" => {
                i += 1;
                properties_json = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(page_id) = page_id else {
        eprintln!("Error: --page-id is required");
        return ExitCode::FAILURE;
    };

    let Some(properties_json) = properties_json else {
        eprintln!("Error: --properties is required (JSON object)");
        return ExitCode::FAILURE;
    };

    let properties: serde_json::Value = match serde_json::from_str(&properties_json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing properties JSON: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.update_page(&workspace_id, &page_id, properties) {
            Ok(page) => {
                let output = serde_json::json!({
                    "success": true,
                    "page": {
                        "id": page.id,
                        "title": page.title,
                        "url": page.url,
                        "last_edited_time": page.last_edited_time,
                    }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_archive_page(args: &[String]) -> ExitCode {
    let (page_id, workspace_id) = match parse_page_args(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let Some(page_id) = page_id else {
        eprintln!("Error: --page-id is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.archive_page(&workspace_id, &page_id) {
            Ok(()) => {
                let output = serde_json::json!({
                    "success": true,
                    "page_id": page_id,
                    "archived": true,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_list_pages(args: &[String]) -> ExitCode {
    let mut limit = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok());
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.list_pages(&workspace_id, limit) {
            Ok(pages) => {
                let output: Vec<_> = pages
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id,
                            "title": p.title,
                            "url": p.url,
                            "last_edited_time": p.last_edited_time,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_get_children(args: &[String]) -> ExitCode {
    let mut parent_id = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--parent-id" => {
                i += 1;
                parent_id = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(parent_id) = parent_id else {
        eprintln!("Error: --parent-id is required");
        return ExitCode::FAILURE;
    };

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.get_child_pages(&workspace_id, &parent_id) {
            Ok(pages) => {
                let output: Vec<_> = pages
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id,
                            "title": p.title,
                            "created_time": p.created_time,
                            "last_edited_time": p.last_edited_time,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_bulk_read(args: &[String]) -> ExitCode {
    let mut page_ids_str = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--page-ids" => {
                i += 1;
                page_ids_str = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(page_ids_str) = page_ids_str else {
        eprintln!("Error: --page-ids is required (comma-separated)");
        return ExitCode::FAILURE;
    };

    let page_ids: Vec<String> = page_ids_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if page_ids.is_empty() {
        eprintln!("Error: No valid page IDs provided");
        return ExitCode::FAILURE;
    }

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => {
            let results = client.bulk_read(&workspace_id, &page_ids);
            let output: Vec<_> = results
                .into_iter()
                .map(|(page_id, result)| match result {
                    Ok(content) => serde_json::json!({
                        "page_id": page_id,
                        "success": true,
                        "page": {
                            "id": content.page.id,
                            "title": content.page.title,
                            "url": content.page.url,
                        },
                        "blocks": content.blocks.iter().map(|b| {
                            serde_json::json!({
                                "id": b.id,
                                "type": b.block_type,
                                "text": b.text_content,
                            })
                        }).collect::<Vec<_>>()
                    }),
                    Err(e) => serde_json::json!({
                        "page_id": page_id,
                        "success": false,
                        "error": e
                    }),
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_export_workspace(args: &[String]) -> ExitCode {
    let mut max_pages = None;
    let mut output_file = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--max-pages" => {
                i += 1;
                max_pages = args.get(i).and_then(|s| s.parse().ok());
            }
            "--output" => {
                i += 1;
                output_file = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let employee_id = match env::var("EMPLOYEE_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Error: EMPLOYEE_ID environment variable is required");
            return ExitCode::FAILURE;
        }
    };

    let workspace_id = resolve_workspace_id(workspace_id);

    match scheduler_module::notion_browser::NotionApiClient::from_env(&employee_id) {
        Ok(client) => match client.export_workspace(&workspace_id, max_pages) {
            Ok(export_data) => {
                let json_output = serde_json::to_string_pretty(&export_data).unwrap();

                if let Some(file_path) = output_file {
                    match std::fs::write(&file_path, &json_output) {
                        Ok(_) => {
                            eprintln!("Exported to: {}", file_path);
                            ExitCode::SUCCESS
                        }
                        Err(e) => {
                            eprintln!("Failed to write output file: {}", e);
                            ExitCode::FAILURE
                        }
                    }
                } else {
                    println!("{}", json_output);
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("API Error: {}", e);
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// Parse a block input from JSON.
fn parse_block_input(
    value: &serde_json::Value,
) -> Option<scheduler_module::notion_browser::BlockInput> {
    let block_type = value["type"].as_str()?;
    let text = value["text"].as_str().unwrap_or("").to_string();

    Some(match block_type {
        "paragraph" => scheduler_module::notion_browser::BlockInput::Paragraph(text),
        "heading_1" => scheduler_module::notion_browser::BlockInput::Heading1(text),
        "heading_2" => scheduler_module::notion_browser::BlockInput::Heading2(text),
        "heading_3" => scheduler_module::notion_browser::BlockInput::Heading3(text),
        "bulleted_list_item" => {
            scheduler_module::notion_browser::BlockInput::BulletedListItem(text)
        }
        "numbered_list_item" => {
            scheduler_module::notion_browser::BlockInput::NumberedListItem(text)
        }
        "to_do" => scheduler_module::notion_browser::BlockInput::ToDo {
            text,
            checked: value["checked"].as_bool().unwrap_or(false),
        },
        "quote" => scheduler_module::notion_browser::BlockInput::Quote(text),
        "callout" => scheduler_module::notion_browser::BlockInput::Callout {
            text,
            emoji: value["emoji"].as_str().map(|s| s.to_string()),
        },
        "code" => scheduler_module::notion_browser::BlockInput::Code {
            text,
            language: value["language"]
                .as_str()
                .unwrap_or("plain text")
                .to_string(),
        },
        "divider" => scheduler_module::notion_browser::BlockInput::Divider,
        _ => return None,
    })
}

fn parse_page_args(args: &[String]) -> Result<(Option<String>, Option<String>), String> {
    let mut page_id = None;
    let mut workspace_id = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--page-id" => {
                i += 1;
                page_id = args.get(i).cloned();
            }
            "--workspace-id" => {
                i += 1;
                workspace_id = args.get(i).cloned();
            }
            arg if arg.starts_with("--") => {
                return Err(format!("Unknown argument: {}", arg));
            }
            _ => {}
        }
        i += 1;
    }

    Ok((page_id, workspace_id))
}
