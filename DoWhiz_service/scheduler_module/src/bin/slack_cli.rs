//! Slack CLI for agent use.
//!
//! Provides commands for sending messages to Slack channels and users.
//! Used by TPM agents for proactive follow-ups.
//!
//! Usage:
//!   slack_cli send-dm --user-id <id> --message <text>
//!   slack_cli send-channel --channel-id <id> --message <text>
//!   slack_cli send --to <id> --message <text> [--thread-ts <ts>]

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return ExitCode::FAILURE;
    }

    let command = &args[1];
    match command.as_str() {
        "send" => cmd_send(&args[2..]),
        "send-dm" => cmd_send_dm(&args[2..]),
        "send-channel" => cmd_send_channel(&args[2..]),
        "open-dm" => cmd_open_dm(&args[2..]),
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
        r#"Slack CLI - Send messages to Slack

Usage:
  slack_cli <command> [options]

Commands:
  send           Send message to channel or DM
    --to <id>           Channel ID (C...) or User ID (U...) to send to
    --message <text>    Message content (supports mrkdwn)
    --thread-ts <ts>    Reply in thread (optional)

  send-dm        Send direct message to user
    --user-id <id>      Slack user ID (U...)
    --message <text>    Message content

  send-channel   Send message to channel
    --channel-id <id>   Slack channel ID (C...)
    --message <text>    Message content
    --thread-ts <ts>    Reply in thread (optional)

  open-dm        Open/get DM channel with user
    --user-id <id>      Slack user ID (U...)
    Returns the channel ID for DM

Environment:
  SLACK_BOT_TOKEN    Required. Bot OAuth token (xoxb-...)
  SLACK_API_BASE_URL Optional. Override API base (default: https://slack.com/api)

Context Files:
  .slack_context.json   Auto-loaded for workspace token lookup

Output:
  JSON to stdout on success, error message to stderr on failure.
"#
    );
}

/// Get Slack bot token from environment or context file.
fn get_bot_token() -> Option<String> {
    // Try environment variable first
    if let Ok(token) = env::var("SLACK_BOT_TOKEN") {
        if !token.trim().is_empty() {
            return Some(token);
        }
    }

    // Try employee-specific token
    if let Ok(employee_id) = env::var("EMPLOYEE_ID") {
        let key = format!(
            "{}_SLACK_BOT_TOKEN",
            employee_id.to_uppercase().replace('-', "_")
        );
        if let Ok(token) = env::var(&key) {
            if !token.trim().is_empty() {
                return Some(token);
            }
        }
    }

    // Try .slack_context.json
    let context_path = std::path::Path::new(".slack_context.json");
    if context_path.exists() {
        if let Ok(content) = std::fs::read_to_string(context_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(token) = json["bot_token"].as_str() {
                    return Some(token.to_string());
                }
            }
        }
    }

    None
}

fn get_api_base() -> String {
    env::var("SLACK_API_BASE_URL").unwrap_or_else(|_| "https://slack.com/api".to_string())
}

#[derive(Debug, Serialize)]
struct PostMessageRequest {
    channel: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_ts: Option<String>,
    mrkdwn: bool,
}

#[derive(Debug, Deserialize)]
struct SlackApiResponse {
    ok: bool,
    error: Option<String>,
    ts: Option<String>,
    channel: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenDmRequest {
    users: String, // Comma-separated user IDs
}

#[derive(Debug, Deserialize)]
struct OpenDmResponse {
    ok: bool,
    error: Option<String>,
    channel: Option<OpenDmChannel>,
}

#[derive(Debug, Deserialize)]
struct OpenDmChannel {
    id: String,
}

fn send_message(
    token: &str,
    channel: &str,
    text: &str,
    thread_ts: Option<&str>,
) -> Result<SlackApiResponse, String> {
    let client = Client::new();
    let url = format!("{}/chat.postMessage", get_api_base().trim_end_matches('/'));

    let request = PostMessageRequest {
        channel: channel.to_string(),
        text: text.to_string(),
        thread_ts: thread_ts.map(|s| s.to_string()),
        mrkdwn: true,
    };

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    response
        .json::<SlackApiResponse>()
        .map_err(|e| format!("Failed to parse response: {}", e))
}

fn open_dm_channel(token: &str, user_id: &str) -> Result<String, String> {
    let client = Client::new();
    let url = format!(
        "{}/conversations.open",
        get_api_base().trim_end_matches('/')
    );

    let request = OpenDmRequest {
        users: user_id.to_string(),
    };

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let dm_response: OpenDmResponse = response
        .json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if dm_response.ok {
        dm_response
            .channel
            .map(|c| c.id)
            .ok_or_else(|| "No channel in response".to_string())
    } else {
        Err(dm_response
            .error
            .unwrap_or_else(|| "Unknown error".to_string()))
    }
}

fn cmd_send(args: &[String]) -> ExitCode {
    let mut to: Option<String> = None;
    let mut message: Option<String> = None;
    let mut thread_ts: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--to" => {
                i += 1;
                to = args.get(i).cloned();
            }
            "--message" => {
                i += 1;
                message = args.get(i).cloned();
            }
            "--thread-ts" => {
                i += 1;
                thread_ts = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(to) = to else {
        eprintln!("Error: --to is required");
        return ExitCode::FAILURE;
    };

    let Some(message) = message else {
        eprintln!("Error: --message is required");
        return ExitCode::FAILURE;
    };

    let Some(token) = get_bot_token() else {
        eprintln!("Error: SLACK_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    // If "to" starts with U (user ID), open DM first
    let channel_id = if to.starts_with('U') {
        match open_dm_channel(&token, &to) {
            Ok(channel_id) => channel_id,
            Err(e) => {
                eprintln!("Error opening DM channel: {}", e);
                return ExitCode::FAILURE;
            }
        }
    } else {
        to
    };

    match send_message(&token, &channel_id, &message, thread_ts.as_deref()) {
        Ok(response) => {
            if response.ok {
                let output = json!({
                    "success": true,
                    "channel": response.channel,
                    "ts": response.ts,
                    "message": "Message sent successfully"
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            } else {
                let output = json!({
                    "success": false,
                    "error": response.error
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_send_dm(args: &[String]) -> ExitCode {
    let mut user_id: Option<String> = None;
    let mut message: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--user-id" => {
                i += 1;
                user_id = args.get(i).cloned();
            }
            "--message" => {
                i += 1;
                message = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(user_id) = user_id else {
        eprintln!("Error: --user-id is required");
        return ExitCode::FAILURE;
    };

    let Some(message) = message else {
        eprintln!("Error: --message is required");
        return ExitCode::FAILURE;
    };

    let Some(token) = get_bot_token() else {
        eprintln!("Error: SLACK_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    // Open DM channel first
    let channel_id = match open_dm_channel(&token, &user_id) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Error opening DM channel: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match send_message(&token, &channel_id, &message, None) {
        Ok(response) => {
            if response.ok {
                let output = json!({
                    "success": true,
                    "channel": channel_id,
                    "user_id": user_id,
                    "ts": response.ts,
                    "message": "DM sent successfully"
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            } else {
                let output = json!({
                    "success": false,
                    "error": response.error
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_send_channel(args: &[String]) -> ExitCode {
    let mut channel_id: Option<String> = None;
    let mut message: Option<String> = None;
    let mut thread_ts: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--channel-id" => {
                i += 1;
                channel_id = args.get(i).cloned();
            }
            "--message" => {
                i += 1;
                message = args.get(i).cloned();
            }
            "--thread-ts" => {
                i += 1;
                thread_ts = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(channel_id) = channel_id else {
        eprintln!("Error: --channel-id is required");
        return ExitCode::FAILURE;
    };

    let Some(message) = message else {
        eprintln!("Error: --message is required");
        return ExitCode::FAILURE;
    };

    let Some(token) = get_bot_token() else {
        eprintln!("Error: SLACK_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    match send_message(&token, &channel_id, &message, thread_ts.as_deref()) {
        Ok(response) => {
            if response.ok {
                let output = json!({
                    "success": true,
                    "channel": channel_id,
                    "ts": response.ts,
                    "message": "Message sent to channel"
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::SUCCESS
            } else {
                let output = json!({
                    "success": false,
                    "error": response.error
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_open_dm(args: &[String]) -> ExitCode {
    let mut user_id: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--user-id" => {
                i += 1;
                user_id = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let Some(user_id) = user_id else {
        eprintln!("Error: --user-id is required");
        return ExitCode::FAILURE;
    };

    let Some(token) = get_bot_token() else {
        eprintln!("Error: SLACK_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    match open_dm_channel(&token, &user_id) {
        Ok(channel_id) => {
            let output = json!({
                "success": true,
                "user_id": user_id,
                "channel_id": channel_id,
                "message": "DM channel opened/retrieved"
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
