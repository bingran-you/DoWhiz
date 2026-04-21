//! Task Status CLI for DevOps monitoring.
//!
//! SECURITY: This CLI is designed for LOCAL SERVER USE ONLY.
//! It requires direct MongoDB access and performs security checks to prevent
//! unauthorized remote execution.
//!
//! Usage:
//!   task_status_cli list-running [--stale-minutes 60]
//!   task_status_cli list-failed [--hours 24]
//!   task_status_cli list-pending [--hours 24]
//!   task_status_cli get-task --task-id <uuid> --user-id <uuid>
//!   task_status_cli executions --task-id <uuid> --user-id <uuid>
//!   task_status_cli summary

use chrono::{Duration as ChronoDuration, Utc};
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::FindOptions;
use mongodb::sync::Database;
use serde_json::json;
use std::env;
use std::path::Path;
use std::process::ExitCode;

const DEFAULT_STALE_MINUTES: i64 = 60;
const DEFAULT_HOURS: i64 = 24;
const LONG_RUNNING_THRESHOLD_SECS: i64 = 3600;
const MAX_ERROR_MESSAGE_LEN: usize = 500;

fn main() -> ExitCode {
    dotenvy::dotenv().ok();

    // Security check: ensure we're running locally on the server
    if let Err(msg) = verify_local_execution() {
        eprintln!("SECURITY ERROR: {}", msg);
        eprintln!();
        eprintln!("This CLI is designed for LOCAL SERVER USE ONLY.");
        eprintln!("It must be run directly on the DoWhiz server with proper environment.");
        return ExitCode::FAILURE;
    }

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return ExitCode::FAILURE;
    }

    let command = &args[1];
    match command.as_str() {
        "list-running" => cmd_list_running(&args[2..]),
        "list-failed" => cmd_list_failed(&args[2..]),
        "list-pending" => cmd_list_pending(&args[2..]),
        "get-task" => cmd_get_task(&args[2..]),
        "executions" => cmd_executions(&args[2..]),
        "summary" => cmd_summary(&args[2..]),
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

/// Verify that the CLI is being run locally on the server.
/// Returns Ok(()) if local execution is confirmed, Err(message) otherwise.
fn verify_local_execution() -> Result<(), String> {
    // Check 1: MONGODB_URI must be set (only available on server)
    if env::var("MONGODB_URI").is_err() {
        return Err("MONGODB_URI environment variable not set".to_string());
    }

    // Check 2: Must have DEPLOY_TARGET or be running from server path
    let has_deploy_target = env::var("DEPLOY_TARGET").is_ok();
    let has_server_path = Path::new("/home/azureuser/server").exists()
        || Path::new("/home/liuxt/deeptutor").exists()
        || env::current_dir()
            .map(|p| {
                p.to_string_lossy().contains("DoWhiz_service")
                    || p.to_string_lossy().contains("server")
            })
            .unwrap_or(false);

    // Check 3: Verify we're not being called through a web server context
    let web_context_indicators = [
        "HTTP_HOST",
        "REQUEST_METHOD",
        "GATEWAY_INTERFACE",
        "SERVER_PROTOCOL",
    ];
    for indicator in &web_context_indicators {
        if env::var(indicator).is_ok() {
            return Err(format!(
                "Detected web server context ({}). This CLI cannot be run via HTTP.",
                indicator
            ));
        }
    }

    // Check 4: Must have at least one server indicator
    if !has_deploy_target && !has_server_path {
        // Allow if TASK_STATUS_CLI_LOCAL_OVERRIDE is set (for local development)
        if env::var("TASK_STATUS_CLI_LOCAL_OVERRIDE").is_ok() {
            eprintln!(
                "WARNING: Running with TASK_STATUS_CLI_LOCAL_OVERRIDE - use only for development"
            );
            return Ok(());
        }
        return Err(
            "Cannot verify server environment. Set DEPLOY_TARGET or run from server path."
                .to_string(),
        );
    }

    Ok(())
}

fn print_usage() {
    eprintln!(
        r#"Task Status CLI - DevOps monitoring for DoWhiz tasks

SECURITY: This CLI is for LOCAL SERVER USE ONLY.

Usage:
  task_status_cli <command> [options]

Commands:
  list-running      List all currently running task executions
    --stale-minutes <n>    Highlight tasks running longer than N minutes (default: 60)
    --limit <n>            Maximum number of results (default: 100)

  list-failed       List recently failed task executions
    --hours <n>            Look back N hours (default: 24)
    --limit <n>            Maximum number of results (default: 100)

  list-pending      List pending/queued tasks waiting to run
    --limit <n>            Maximum number of results (default: 100)

  get-task          Get detailed info for a specific task
    --task-id <uuid>       Task UUID (required)
    --user-id <uuid>       User/owner UUID (required)

  executions        List execution history for a task
    --task-id <uuid>       Task UUID (required)
    --user-id <uuid>       User/owner UUID (required)
    --limit <n>            Maximum number of results (default: 20)

  summary           Show aggregate statistics

Environment:
  MONGODB_URI              Required - MongoDB connection string
  MONGODB_DATABASE         Required - Target database name
                           (e.g., dowhiz_production_little_bear or dowhiz_staging_boiled_egg)
  DEPLOY_TARGET            Required on server (staging/production)
  TASK_STATUS_CLI_LOCAL_OVERRIDE    Set to allow local development use

Output:
  JSON to stdout on success, error message to stderr on failure.
"#
    );
}

fn get_database() -> Result<Database, String> {
    let uri = env::var("MONGODB_URI").map_err(|_| "MONGODB_URI not set")?;
    let db_name = env::var("MONGODB_DATABASE")
        .map_err(|_| "MONGODB_DATABASE not set. Please set this environment variable to the target database name (e.g., dowhiz_production_little_bear or dowhiz_staging_boiled_egg)")?;

    if db_name.trim().is_empty() {
        return Err("MONGODB_DATABASE is empty".to_string());
    }

    let mut options =
        mongodb::options::ClientOptions::parse(&uri).map_err(|e| format!("Invalid URI: {}", e))?;
    options.app_name = Some("TaskStatusCLI".to_string());
    let client = mongodb::sync::Client::with_options(options)
        .map_err(|e| format!("Failed to create client: {}", e))?;

    Ok(client.database(&db_name))
}

/// Truncate error message to avoid exposing too much detail
fn truncate_error_message(msg: Option<&str>) -> Option<String> {
    msg.map(|s| {
        if s.len() > MAX_ERROR_MESSAGE_LEN {
            format!("{}... [truncated]", &s[..MAX_ERROR_MESSAGE_LEN])
        } else {
            s.to_string()
        }
    })
}

fn cmd_list_running(args: &[String]) -> ExitCode {
    let mut stale_minutes = DEFAULT_STALE_MINUTES;
    let mut limit = 100i64;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--stale-minutes" => {
                i += 1;
                stale_minutes = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_STALE_MINUTES);
            }
            "--limit" => {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(100);
            }
            _ => {}
        }
        i += 1;
    }

    let db = match get_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let executions = db.collection::<Document>("task_executions");
    let filter = doc! { "status": "running" };
    let options = FindOptions::builder()
        .sort(doc! { "started_at": -1 })
        .limit(limit)
        .build();

    let cursor = match executions.find(filter, options) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error querying executions: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let now = Utc::now();
    let stale_threshold = now - ChronoDuration::minutes(stale_minutes);
    let long_running_threshold = now - ChronoDuration::seconds(LONG_RUNNING_THRESHOLD_SECS);

    let mut results = Vec::new();
    let mut stale_count = 0usize;
    let mut long_running_count = 0usize;

    for doc in cursor {
        let doc = match doc {
            Ok(d) => d,
            Err(_) => continue,
        };

        let started_at = doc
            .get_datetime("started_at")
            .map(|dt| dt.to_chrono())
            .ok();
        let is_stale = started_at
            .map(|dt| dt < stale_threshold)
            .unwrap_or(false);
        let is_long_running = started_at
            .map(|dt| dt < long_running_threshold)
            .unwrap_or(false);

        if is_stale {
            stale_count += 1;
        }
        if is_long_running {
            long_running_count += 1;
        }

        let running_minutes = started_at
            .map(|dt| now.signed_duration_since(dt).num_minutes())
            .unwrap_or(0);

        results.push(json!({
            "task_id": doc.get_str("task_id").unwrap_or(""),
            "execution_id": doc.get_i64("execution_id").unwrap_or(0),
            "owner_id": doc.get_document("owner_scope").ok().and_then(|d| d.get_str("id").ok()).unwrap_or(""),
            "owner_kind": doc.get_document("owner_scope").ok().and_then(|d| d.get_str("kind").ok()).unwrap_or(""),
            "started_at": started_at.map(|dt| dt.to_rfc3339()),
            "running_minutes": running_minutes,
            "is_stale": is_stale,
            "is_long_running": is_long_running,
        }));
    }

    let output = json!({
        "command": "list-running",
        "timestamp": now.to_rfc3339(),
        "stale_threshold_minutes": stale_minutes,
        "total_running": results.len(),
        "stale_count": stale_count,
        "long_running_count": long_running_count,
        "tasks": results,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

fn cmd_list_failed(args: &[String]) -> ExitCode {
    let mut hours = DEFAULT_HOURS;
    let mut limit = 100i64;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--hours" => {
                i += 1;
                hours = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_HOURS);
            }
            "--limit" => {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(100);
            }
            _ => {}
        }
        i += 1;
    }

    let db = match get_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let executions = db.collection::<Document>("task_executions");
    let since = Utc::now() - ChronoDuration::hours(hours);
    let filter = doc! {
        "status": "failed",
        "started_at": { "$gte": BsonDateTime::from_chrono(since) }
    };
    let options = FindOptions::builder()
        .sort(doc! { "started_at": -1 })
        .limit(limit)
        .build();

    let cursor = match executions.find(filter, options) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error querying executions: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let mut results = Vec::new();
    for doc in cursor {
        let doc = match doc {
            Ok(d) => d,
            Err(_) => continue,
        };

        let started_at = doc
            .get_datetime("started_at")
            .map(|dt| dt.to_chrono())
            .ok();
        let finished_at = match doc.get("finished_at") {
            Some(Bson::DateTime(dt)) => Some(dt.to_chrono()),
            _ => None,
        };
        let duration_secs = match (started_at, finished_at) {
            (Some(s), Some(f)) => Some(f.signed_duration_since(s).num_seconds()),
            _ => None,
        };

        results.push(json!({
            "task_id": doc.get_str("task_id").unwrap_or(""),
            "execution_id": doc.get_i64("execution_id").unwrap_or(0),
            "owner_id": doc.get_document("owner_scope").ok().and_then(|d| d.get_str("id").ok()).unwrap_or(""),
            "owner_kind": doc.get_document("owner_scope").ok().and_then(|d| d.get_str("kind").ok()).unwrap_or(""),
            "started_at": started_at.map(|dt| dt.to_rfc3339()),
            "finished_at": finished_at.map(|dt| dt.to_rfc3339()),
            "duration_seconds": duration_secs,
            "error_message": truncate_error_message(doc.get_str("error_message").ok()),
        }));
    }

    let output = json!({
        "command": "list-failed",
        "timestamp": Utc::now().to_rfc3339(),
        "lookback_hours": hours,
        "total_failed": results.len(),
        "tasks": results,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

fn cmd_list_pending(args: &[String]) -> ExitCode {
    let mut limit = 100i64;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(100);
            }
            _ => {}
        }
        i += 1;
    }

    let db = match get_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let now = Utc::now();
    let task_index = db.collection::<Document>("task_index");

    // Find tasks that are due (next_run <= now)
    let filter = doc! {
        "enabled": true,
        "next_run": { "$lte": BsonDateTime::from_chrono(now) }
    };
    let options = FindOptions::builder()
        .sort(doc! { "next_run": 1 })
        .limit(limit)
        .build();

    let cursor = match task_index.find(filter, options) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error querying task_index: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let mut results = Vec::new();
    for doc in cursor {
        let doc = match doc {
            Ok(d) => d,
            Err(_) => continue,
        };

        let next_run = doc.get_datetime("next_run").map(|dt| dt.to_chrono()).ok();
        let waiting_minutes = next_run
            .map(|dt| now.signed_duration_since(dt).num_minutes())
            .unwrap_or(0);

        results.push(json!({
            "task_id": doc.get_str("task_id").unwrap_or(""),
            "user_id": doc.get_str("user_id").unwrap_or(""),
            "next_run": next_run.map(|dt| dt.to_rfc3339()),
            "waiting_minutes": waiting_minutes,
            "enabled": doc.get_bool("enabled").unwrap_or(false),
        }));
    }

    let output = json!({
        "command": "list-pending",
        "timestamp": now.to_rfc3339(),
        "total_pending": results.len(),
        "tasks": results,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

fn cmd_get_task(args: &[String]) -> ExitCode {
    let mut task_id: Option<String> = None;
    let mut user_id: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--task-id" => {
                i += 1;
                task_id = args.get(i).cloned();
            }
            "--user-id" => {
                i += 1;
                user_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(task_id) = task_id else {
        eprintln!("Error: --task-id is required");
        return ExitCode::FAILURE;
    };

    let Some(user_id) = user_id else {
        eprintln!("Error: --user-id is required");
        return ExitCode::FAILURE;
    };

    let db = match get_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let tasks = db.collection::<Document>("tasks");
    let filter = doc! {
        "task_id": &task_id,
        "owner_scope.id": &user_id,
    };

    let task_doc = match tasks.find_one(filter, None) {
        Ok(Some(doc)) => doc,
        Ok(None) => {
            let output = json!({
                "error": "not_found",
                "message": format!("Task {} not found for user {}", task_id, user_id)
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Get latest execution
    let executions = db.collection::<Document>("task_executions");
    let exec_filter = doc! {
        "task_id": &task_id,
        "owner_scope.id": &user_id,
    };
    let exec_options = FindOptions::builder()
        .sort(doc! { "started_at": -1 })
        .limit(1)
        .build();

    let latest_execution = executions
        .find(exec_filter, exec_options)
        .ok()
        .and_then(|mut cursor| cursor.next())
        .and_then(|r| r.ok());

    let created_at = task_doc
        .get_datetime("created_at")
        .map(|dt| dt.to_chrono())
        .ok();
    let last_run = match task_doc.get("last_run") {
        Some(Bson::DateTime(dt)) => Some(dt.to_chrono()),
        _ => None,
    };

    let output = json!({
        "command": "get-task",
        "timestamp": Utc::now().to_rfc3339(),
        "task": {
            "task_id": task_doc.get_str("task_id").unwrap_or(""),
            "owner_id": task_doc.get_document("owner_scope").ok().and_then(|d| d.get_str("id").ok()).unwrap_or(""),
            "owner_kind": task_doc.get_document("owner_scope").ok().and_then(|d| d.get_str("kind").ok()).unwrap_or(""),
            "kind": task_doc.get_str("kind").unwrap_or(""),
            "channel": task_doc.get_str("channel").unwrap_or(""),
            "enabled": task_doc.get_bool("enabled").unwrap_or(false),
            "retry_count": task_doc.get_i32("retry_count").unwrap_or(0),
            "created_at": created_at.map(|dt| dt.to_rfc3339()),
            "last_run": last_run.map(|dt| dt.to_rfc3339()),
            "schedule": task_doc.get_document("schedule").ok().map(|d| {
                json!({
                    "type": d.get_str("type").unwrap_or(""),
                    "cron_expression": d.get_str("cron_expression").ok(),
                    "next_run": d.get_datetime("next_run").map(|dt| dt.to_chrono().to_rfc3339()).ok(),
                    "run_at": d.get_datetime("run_at").map(|dt| dt.to_chrono().to_rfc3339()).ok(),
                })
            }),
        },
        "latest_execution": latest_execution.map(|exec| {
            let started_at = exec.get_datetime("started_at").map(|dt| dt.to_chrono()).ok();
            let finished_at = match exec.get("finished_at") {
                Some(Bson::DateTime(dt)) => Some(dt.to_chrono()),
                _ => None,
            };
            json!({
                "execution_id": exec.get_i64("execution_id").unwrap_or(0),
                "status": exec.get_str("status").unwrap_or(""),
                "started_at": started_at.map(|dt| dt.to_rfc3339()),
                "finished_at": finished_at.map(|dt| dt.to_rfc3339()),
                "error_message": truncate_error_message(exec.get_str("error_message").ok()),
            })
        }),
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

fn cmd_executions(args: &[String]) -> ExitCode {
    let mut task_id: Option<String> = None;
    let mut user_id: Option<String> = None;
    let mut limit = 20i64;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--task-id" => {
                i += 1;
                task_id = args.get(i).cloned();
            }
            "--user-id" => {
                i += 1;
                user_id = args.get(i).cloned();
            }
            "--limit" => {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(20);
            }
            _ => {}
        }
        i += 1;
    }

    let Some(task_id) = task_id else {
        eprintln!("Error: --task-id is required");
        return ExitCode::FAILURE;
    };

    let Some(user_id) = user_id else {
        eprintln!("Error: --user-id is required");
        return ExitCode::FAILURE;
    };

    let db = match get_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let executions = db.collection::<Document>("task_executions");
    let filter = doc! {
        "task_id": &task_id,
        "owner_scope.id": &user_id,
    };
    let options = FindOptions::builder()
        .sort(doc! { "started_at": -1 })
        .limit(limit)
        .build();

    let cursor = match executions.find(filter, options) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error querying executions: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let mut results = Vec::new();
    for doc in cursor {
        let doc = match doc {
            Ok(d) => d,
            Err(_) => continue,
        };

        let started_at = doc
            .get_datetime("started_at")
            .map(|dt| dt.to_chrono())
            .ok();
        let finished_at = match doc.get("finished_at") {
            Some(Bson::DateTime(dt)) => Some(dt.to_chrono()),
            _ => None,
        };
        let duration_secs = match (started_at, finished_at) {
            (Some(s), Some(f)) => Some(f.signed_duration_since(s).num_seconds()),
            _ => None,
        };

        results.push(json!({
            "execution_id": doc.get_i64("execution_id").unwrap_or(0),
            "status": doc.get_str("status").unwrap_or(""),
            "started_at": started_at.map(|dt| dt.to_rfc3339()),
            "finished_at": finished_at.map(|dt| dt.to_rfc3339()),
            "duration_seconds": duration_secs,
            "error_message": truncate_error_message(doc.get_str("error_message").ok()),
        }));
    }

    let output = json!({
        "command": "executions",
        "timestamp": Utc::now().to_rfc3339(),
        "task_id": task_id,
        "user_id": user_id,
        "total_executions": results.len(),
        "executions": results,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

fn cmd_summary(_args: &[String]) -> ExitCode {
    let db = match get_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let now = Utc::now();
    let last_24h = now - ChronoDuration::hours(24);

    let executions = db.collection::<Document>("task_executions");
    let tasks = db.collection::<Document>("tasks");
    let task_index = db.collection::<Document>("task_index");

    // Count running executions
    let running_count = executions
        .count_documents(doc! { "status": "running" }, None)
        .unwrap_or(0);

    // Count running for > 1 hour (potentially stale)
    let stale_threshold = now - ChronoDuration::hours(1);
    let stale_running_count = executions
        .count_documents(
            doc! {
                "status": "running",
                "started_at": { "$lt": BsonDateTime::from_chrono(stale_threshold) }
            },
            None,
        )
        .unwrap_or(0);

    // Count failed in last 24h
    let failed_24h_count = executions
        .count_documents(
            doc! {
                "status": "failed",
                "started_at": { "$gte": BsonDateTime::from_chrono(last_24h) }
            },
            None,
        )
        .unwrap_or(0);

    // Count successful in last 24h
    let success_24h_count = executions
        .count_documents(
            doc! {
                "status": "success",
                "started_at": { "$gte": BsonDateTime::from_chrono(last_24h) }
            },
            None,
        )
        .unwrap_or(0);

    // Count pending tasks (due but not yet picked up)
    let pending_count = task_index
        .count_documents(
            doc! {
                "enabled": true,
                "next_run": { "$lte": BsonDateTime::from_chrono(now) }
            },
            None,
        )
        .unwrap_or(0);

    // Count total enabled tasks
    let enabled_tasks_count = tasks
        .count_documents(doc! { "enabled": true }, None)
        .unwrap_or(0);

    // Count distinct users with running tasks
    let distinct_running_users = executions
        .distinct(
            "owner_scope.id",
            doc! {
                "status": "running",
                "owner_scope.kind": "user"
            },
            None,
        )
        .map(|v| v.len())
        .unwrap_or(0);

    let output = json!({
        "command": "summary",
        "timestamp": now.to_rfc3339(),
        "database": db.name(),
        "execution_stats": {
            "running": running_count,
            "stale_running_1h": stale_running_count,
            "failed_24h": failed_24h_count,
            "success_24h": success_24h_count,
            "distinct_users_with_running_tasks": distinct_running_users,
        },
        "task_stats": {
            "pending_due": pending_count,
            "enabled_total": enabled_tasks_count,
        },
        "health": {
            "has_stale_tasks": stale_running_count > 0,
            "high_failure_rate": failed_24h_count > success_24h_count && failed_24h_count > 10,
            "pending_backlog": pending_count > 50,
        }
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn cleanup_env_vars() {
        std::env::remove_var("HTTP_HOST");
        std::env::remove_var("REQUEST_METHOD");
        std::env::remove_var("GATEWAY_INTERFACE");
        std::env::remove_var("SERVER_PROTOCOL");
        std::env::remove_var("MONGODB_URI");
        std::env::remove_var("MONGODB_DATABASE");
        std::env::remove_var("DEPLOY_TARGET");
        std::env::remove_var("TASK_STATUS_CLI_LOCAL_OVERRIDE");
    }

    #[test]
    #[serial]
    fn verify_local_execution_rejects_web_context_http_host() {
        cleanup_env_vars();
        std::env::set_var("MONGODB_URI", "mongodb://localhost");
        std::env::set_var("TASK_STATUS_CLI_LOCAL_OVERRIDE", "1");
        std::env::set_var("HTTP_HOST", "example.com");
        let result = verify_local_execution();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("web server context"));
        cleanup_env_vars();
    }

    #[test]
    #[serial]
    fn verify_local_execution_rejects_web_context_request_method() {
        cleanup_env_vars();
        std::env::set_var("MONGODB_URI", "mongodb://localhost");
        std::env::set_var("TASK_STATUS_CLI_LOCAL_OVERRIDE", "1");
        std::env::set_var("REQUEST_METHOD", "GET");
        let result = verify_local_execution();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("web server context"));
        cleanup_env_vars();
    }

    #[test]
    #[serial]
    fn verify_local_execution_requires_mongodb_uri() {
        cleanup_env_vars();
        let result = verify_local_execution();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("MONGODB_URI"));
        cleanup_env_vars();
    }

    #[test]
    #[serial]
    fn verify_local_execution_allows_with_override() {
        cleanup_env_vars();
        std::env::set_var("MONGODB_URI", "mongodb://localhost");
        std::env::set_var("TASK_STATUS_CLI_LOCAL_OVERRIDE", "1");
        let result = verify_local_execution();
        assert!(result.is_ok());
        cleanup_env_vars();
    }

    #[test]
    #[serial]
    fn verify_local_execution_allows_with_deploy_target() {
        cleanup_env_vars();
        std::env::set_var("MONGODB_URI", "mongodb://localhost");
        std::env::set_var("DEPLOY_TARGET", "staging");
        let result = verify_local_execution();
        assert!(result.is_ok());
        cleanup_env_vars();
    }

    #[test]
    fn truncate_error_message_short() {
        let msg = Some("short error");
        let result = truncate_error_message(msg);
        assert_eq!(result, Some("short error".to_string()));
    }

    #[test]
    fn truncate_error_message_long() {
        let long_msg = "x".repeat(600);
        let result = truncate_error_message(Some(&long_msg));
        assert!(result.is_some());
        let truncated = result.unwrap();
        assert!(truncated.ends_with("... [truncated]"));
        assert!(truncated.len() < 600);
    }

    #[test]
    fn truncate_error_message_none() {
        let result = truncate_error_message(None);
        assert!(result.is_none());
    }

    #[test]
    fn parse_stale_minutes_arg() {
        let args = vec!["--stale-minutes".to_string(), "120".to_string()];
        let mut stale_minutes = DEFAULT_STALE_MINUTES;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--stale-minutes" {
                i += 1;
                stale_minutes = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_STALE_MINUTES);
            }
            i += 1;
        }
        assert_eq!(stale_minutes, 120);
    }

    #[test]
    fn parse_limit_arg() {
        let args = vec!["--limit".to_string(), "50".to_string()];
        let mut limit = 100i64;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--limit" {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(100);
            }
            i += 1;
        }
        assert_eq!(limit, 50);
    }

    #[test]
    fn parse_hours_arg() {
        let args = vec!["--hours".to_string(), "6".to_string()];
        let mut hours = DEFAULT_HOURS;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--hours" {
                i += 1;
                hours = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_HOURS);
            }
            i += 1;
        }
        assert_eq!(hours, 6);
    }

    #[test]
    fn parse_invalid_arg_uses_default() {
        let args = vec!["--stale-minutes".to_string(), "not_a_number".to_string()];
        let mut stale_minutes = DEFAULT_STALE_MINUTES;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--stale-minutes" {
                i += 1;
                stale_minutes = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_STALE_MINUTES);
            }
            i += 1;
        }
        assert_eq!(stale_minutes, DEFAULT_STALE_MINUTES);
    }
}
