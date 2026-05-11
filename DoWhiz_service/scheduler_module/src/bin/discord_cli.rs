//! Discord CLI for agent use.
//!
//! Provides commands for sending messages to Discord channels and users,
//! as well as reading channels and messages for bug scanning.
//!
//! Usage:
//!   discord_cli send-dm --user-id <id> --message <text>
//!   discord_cli send-channel --channel-id <id> --message <text>
//!   discord_cli send --to <id> --message <text> [--reply-to <msg_id>]
//!   discord_cli list-channels --guild-id <id>
//!   discord_cli read-messages --channel-id <id> [--limit <n>]

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
        "list-guild-members" => cmd_list_guild_members(&args[2..]),
        "dm-all-guild" => cmd_dm_all_guild(&args[2..]),
        "list-channels" => cmd_list_channels(&args[2..]),
        "read-messages" => cmd_read_messages(&args[2..]),
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
        r#"Discord CLI - Send messages to Discord

Usage:
  discord_cli <command> [options]

Commands:
  send           Send message (auto-detects channel vs DM)
    --to <id>           Channel ID or User ID (snowflake)
    --message <text>    Message content
    --reply-to <id>     Reply to message ID (optional)

  send-dm        Send direct message to user
    --user-id <id>      Discord user ID (snowflake)
    --message <text>    Message content

  send-channel   Send message to channel
    --channel-id <id>   Discord channel ID (snowflake)
    --message <text>    Message content
    --reply-to <id>     Reply to message ID (optional)

  list-guild-members   List all members in a guild/server
    --guild-id <id>     Discord guild/server ID
    --no-bots           Exclude bot accounts (optional)

  dm-all-guild   Send DM to all members in a guild
    --guild-id <id>     Discord guild/server ID
    --message <text>    Message content
    --no-bots           Exclude bot accounts (default: true)
    --dry-run           Preview without sending (optional)

  list-channels  List channels in a guild/server
    --guild-id <id>     Discord guild/server ID
    --text-only         Only show text channels (optional)

  read-messages  Read recent messages from a channel
    --channel-id <id>   Discord channel ID
    --limit <n>         Number of messages (default: 50, max: 100)

Environment:
  DISCORD_BOT_TOKEN     Required. Bot token
  DISCORD_API_BASE_URL  Optional. Override API base (default: https://discord.com/api/v10)

Context Files:
  .discord_context.json   Auto-loaded for bot token lookup

Output:
  JSON to stdout on success, error message to stderr on failure.

Notes:
  - Discord message content limit is 2000 characters
  - DM requires the bot and user to share a server
  - Bot must have SERVER MEMBERS INTENT enabled for list-guild-members
"#
    );
}

/// Get Discord bot token from environment or context file.
fn get_bot_token() -> Option<String> {
    // Try environment variable first
    if let Ok(token) = env::var("DISCORD_BOT_TOKEN") {
        if !token.trim().is_empty() {
            return Some(token);
        }
    }

    // Try employee-specific token
    if let Ok(employee_id) = env::var("EMPLOYEE_ID") {
        let key = format!(
            "{}_DISCORD_BOT_TOKEN",
            employee_id.to_uppercase().replace('-', "_")
        );
        if let Ok(token) = env::var(&key) {
            if !token.trim().is_empty() {
                return Some(token);
            }
        }
    }

    // Try .discord_context.json
    let context_path = std::path::Path::new(".discord_context.json");
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
    env::var("DISCORD_API_BASE_URL").unwrap_or_else(|_| "https://discord.com/api/v10".to_string())
}

#[derive(Debug, Serialize)]
struct CreateDmRequest {
    recipient_id: String,
}

#[derive(Debug, Deserialize)]
struct DmChannelResponse {
    id: String,
}

#[derive(Debug, Serialize)]
struct CreateMessageRequest {
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_reference: Option<MessageReference>,
}

#[derive(Debug, Serialize)]
struct MessageReference {
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct MessageResponse {
    id: String,
    channel_id: String,
}

#[derive(Debug, Deserialize)]
struct GuildMember {
    user: Option<DiscordUser>,
}

#[derive(Debug, Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    #[serde(default)]
    bot: bool,
    #[serde(default)]
    global_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscordChannel {
    id: String,
    name: Option<String>,
    #[serde(rename = "type")]
    channel_type: u8,
    #[serde(default)]
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscordMessage {
    id: String,
    content: String,
    author: DiscordUser,
    timestamp: String,
}

fn create_dm_channel(token: &str, user_id: &str) -> Result<String, String> {
    let client = Client::new();
    let url = format!(
        "{}/users/@me/channels",
        get_api_base().trim_end_matches('/')
    );

    let request = CreateDmRequest {
        recipient_id: user_id.to_string(),
    };

    let response = client
        .post(&url)
        .header("Authorization", format!("Bot {}", token))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if response.status().is_success() {
        let dm_response: DmChannelResponse = response
            .json()
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        Ok(dm_response.id)
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        Err(format!(
            "DM channel creation failed ({}): {}",
            status, error_text
        ))
    }
}

fn send_message(
    token: &str,
    channel_id: &str,
    content: &str,
    reply_to: Option<&str>,
) -> Result<MessageResponse, String> {
    let client = Client::new();
    let url = format!(
        "{}/channels/{}/messages",
        get_api_base().trim_end_matches('/'),
        channel_id
    );

    // Discord has a 2000 character limit
    let content = if content.len() > 2000 {
        format!("{}...", &content[..1997])
    } else {
        content.to_string()
    };

    let request = CreateMessageRequest {
        content,
        message_reference: reply_to.map(|id| MessageReference {
            message_id: id.to_string(),
        }),
    };

    let response = client
        .post(&url)
        .header("Authorization", format!("Bot {}", token))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if response.status().is_success() {
        response
            .json()
            .map_err(|e| format!("Failed to parse response: {}", e))
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        Err(format!("Message send failed ({}): {}", status, error_text))
    }
}

fn cmd_send(args: &[String]) -> ExitCode {
    let mut to: Option<String> = None;
    let mut message: Option<String> = None;
    let mut reply_to: Option<String> = None;

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
            "--reply-to" => {
                i += 1;
                reply_to = args.get(i).cloned();
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
        eprintln!("Error: DISCORD_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    // Heuristic: if ID is less than 18 digits, treat as user ID and create DM
    // Channel IDs and User IDs are both snowflakes, but this is a reasonable guess
    // In practice, use send-dm or send-channel explicitly
    let channel_id = if to.len() < 18 || to.starts_with("dm:") {
        let user_id = to.strip_prefix("dm:").unwrap_or(&to);
        match create_dm_channel(&token, user_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Error creating DM channel: {}", e);
                return ExitCode::FAILURE;
            }
        }
    } else {
        to
    };

    match send_message(&token, &channel_id, &message, reply_to.as_deref()) {
        Ok(response) => {
            let output = json!({
                "success": true,
                "channel_id": response.channel_id,
                "message_id": response.id,
                "message": "Message sent successfully"
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
        eprintln!("Error: DISCORD_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    // Create DM channel first
    let channel_id = match create_dm_channel(&token, &user_id) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Error creating DM channel: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match send_message(&token, &channel_id, &message, None) {
        Ok(response) => {
            let output = json!({
                "success": true,
                "user_id": user_id,
                "channel_id": channel_id,
                "message_id": response.id,
                "message": "DM sent successfully"
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

fn cmd_send_channel(args: &[String]) -> ExitCode {
    let mut channel_id: Option<String> = None;
    let mut message: Option<String> = None;
    let mut reply_to: Option<String> = None;

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
            "--reply-to" => {
                i += 1;
                reply_to = args.get(i).cloned();
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
        eprintln!("Error: DISCORD_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    match send_message(&token, &channel_id, &message, reply_to.as_deref()) {
        Ok(response) => {
            let output = json!({
                "success": true,
                "channel_id": channel_id,
                "message_id": response.id,
                "message": "Message sent to channel"
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

fn list_guild_members(
    token: &str,
    guild_id: &str,
    exclude_bots: bool,
) -> Result<Vec<DiscordUser>, String> {
    let client = Client::new();
    let mut all_members = Vec::new();
    let mut after: Option<String> = None;
    let limit = 1000; // Discord max per request

    loop {
        let mut url = format!(
            "{}/guilds/{}/members?limit={}",
            get_api_base().trim_end_matches('/'),
            guild_id,
            limit
        );

        if let Some(ref after_id) = after {
            url.push_str(&format!("&after={}", after_id));
        }

        let response = client
            .get(&url)
            .header("Authorization", format!("Bot {}", token))
            .send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(format!(
                "Failed to list guild members ({}): {}",
                status, error_text
            ));
        }

        let members: Vec<GuildMember> = response
            .json()
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if members.is_empty() {
            break;
        }

        let last_id = members
            .last()
            .and_then(|m| m.user.as_ref())
            .map(|u| u.id.clone());

        for member in members {
            if let Some(user) = member.user {
                if exclude_bots && user.bot {
                    continue;
                }
                all_members.push(user);
            }
        }

        // If we got fewer than limit, we've reached the end
        if last_id.is_none() || all_members.len() < limit {
            break;
        }
        after = last_id;
    }

    Ok(all_members)
}

fn cmd_list_guild_members(args: &[String]) -> ExitCode {
    let mut guild_id: Option<String> = None;
    let mut exclude_bots = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--guild-id" => {
                i += 1;
                guild_id = args.get(i).cloned();
            }
            "--no-bots" => {
                exclude_bots = true;
            }
            _ => {}
        }
        i += 1;
    }

    let Some(guild_id) = guild_id else {
        eprintln!("Error: --guild-id is required");
        return ExitCode::FAILURE;
    };

    let Some(token) = get_bot_token() else {
        eprintln!("Error: DISCORD_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    match list_guild_members(&token, &guild_id, exclude_bots) {
        Ok(members) => {
            let member_list: Vec<serde_json::Value> = members
                .iter()
                .map(|u| {
                    json!({
                        "id": u.id,
                        "username": u.username,
                        "display_name": u.global_name.as_deref().unwrap_or(&u.username),
                        "bot": u.bot
                    })
                })
                .collect();

            let output = json!({
                "success": true,
                "guild_id": guild_id,
                "member_count": members.len(),
                "members": member_list
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

fn cmd_dm_all_guild(args: &[String]) -> ExitCode {
    let mut guild_id: Option<String> = None;
    let mut message: Option<String> = None;
    let mut exclude_bots = true; // Default to excluding bots
    let mut dry_run = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--guild-id" => {
                i += 1;
                guild_id = args.get(i).cloned();
            }
            "--message" => {
                i += 1;
                message = args.get(i).cloned();
            }
            "--no-bots" => {
                exclude_bots = true;
            }
            "--include-bots" => {
                exclude_bots = false;
            }
            "--dry-run" => {
                dry_run = true;
            }
            _ => {}
        }
        i += 1;
    }

    let Some(guild_id) = guild_id else {
        eprintln!("Error: --guild-id is required");
        return ExitCode::FAILURE;
    };

    let Some(message_content) = message else {
        eprintln!("Error: --message is required");
        return ExitCode::FAILURE;
    };

    let Some(token) = get_bot_token() else {
        eprintln!("Error: DISCORD_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    // Get all members first
    let members = match list_guild_members(&token, &guild_id, exclude_bots) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error listing members: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if dry_run {
        let member_list: Vec<serde_json::Value> = members
            .iter()
            .map(|u| {
                json!({
                    "id": u.id,
                    "username": u.username,
                    "display_name": u.global_name.as_deref().unwrap_or(&u.username)
                })
            })
            .collect();

        let output = json!({
            "dry_run": true,
            "guild_id": guild_id,
            "message_preview": if message_content.len() > 100 {
                format!("{}...", &message_content[..100])
            } else {
                message_content.clone()
            },
            "would_send_to": member_list,
            "recipient_count": members.len()
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return ExitCode::SUCCESS;
    }

    // Send DMs to all members
    let mut success_count = 0;
    let mut failed_count = 0;
    let mut results: Vec<serde_json::Value> = Vec::new();

    for user in &members {
        // Create DM channel
        match create_dm_channel(&token, &user.id) {
            Ok(channel_id) => {
                // Send message
                match send_message(&token, &channel_id, &message_content, None) {
                    Ok(response) => {
                        success_count += 1;
                        results.push(json!({
                            "user_id": user.id,
                            "username": user.username,
                            "status": "sent",
                            "message_id": response.id
                        }));
                    }
                    Err(e) => {
                        failed_count += 1;
                        results.push(json!({
                            "user_id": user.id,
                            "username": user.username,
                            "status": "failed",
                            "error": e
                        }));
                    }
                }
            }
            Err(e) => {
                failed_count += 1;
                results.push(json!({
                    "user_id": user.id,
                    "username": user.username,
                    "status": "dm_channel_failed",
                    "error": e
                }));
            }
        }

        // Small delay to avoid rate limiting (Discord allows ~5 requests/second)
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    let output = json!({
        "success": true,
        "guild_id": guild_id,
        "total_members": members.len(),
        "sent": success_count,
        "failed": failed_count,
        "results": results
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    if failed_count > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_list_channels(args: &[String]) -> ExitCode {
    let mut guild_id: Option<String> = None;
    let mut text_only = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--guild-id" => {
                i += 1;
                guild_id = args.get(i).cloned();
            }
            "--text-only" => {
                text_only = true;
            }
            _ => {}
        }
        i += 1;
    }

    let Some(guild_id) = guild_id else {
        eprintln!("Error: --guild-id is required");
        return ExitCode::FAILURE;
    };

    let Some(token) = get_bot_token() else {
        eprintln!("Error: DISCORD_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    let client = Client::new();
    let url = format!(
        "{}/guilds/{}/channels",
        get_api_base().trim_end_matches('/'),
        guild_id
    );

    let response = match client
        .get(&url)
        .header("Authorization", format!("Bot {}", token))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: HTTP request failed: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        eprintln!(
            "Error: Failed to list channels ({}): {}",
            status, error_text
        );
        return ExitCode::FAILURE;
    }

    let channels: Vec<DiscordChannel> = match response.json() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: Failed to parse response: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Filter to text channels (type 0) if requested
    let filtered: Vec<_> = channels
        .into_iter()
        .filter(|c| !text_only || c.channel_type == 0)
        .collect();

    let channel_list: Vec<serde_json::Value> = filtered
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "type": match c.channel_type {
                    0 => "text",
                    2 => "voice",
                    4 => "category",
                    5 => "announcement",
                    10 | 11 | 12 => "thread",
                    13 => "stage",
                    15 => "forum",
                    _ => "other"
                },
                "parent_id": c.parent_id
            })
        })
        .collect();

    let output = json!({
        "success": true,
        "guild_id": guild_id,
        "channel_count": channel_list.len(),
        "channels": channel_list
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}

fn cmd_read_messages(args: &[String]) -> ExitCode {
    let mut channel_id: Option<String> = None;
    let mut limit: u32 = 50;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--channel-id" => {
                i += 1;
                channel_id = args.get(i).cloned();
            }
            "--limit" => {
                i += 1;
                if let Some(n) = args.get(i).and_then(|s| s.parse().ok()) {
                    limit = std::cmp::min(n, 100); // Discord max is 100
                }
            }
            _ => {}
        }
        i += 1;
    }

    let Some(channel_id) = channel_id else {
        eprintln!("Error: --channel-id is required");
        return ExitCode::FAILURE;
    };

    let Some(token) = get_bot_token() else {
        eprintln!("Error: DISCORD_BOT_TOKEN not configured");
        return ExitCode::FAILURE;
    };

    let client = Client::new();
    let url = format!(
        "{}/channels/{}/messages?limit={}",
        get_api_base().trim_end_matches('/'),
        channel_id,
        limit
    );

    let response = match client
        .get(&url)
        .header("Authorization", format!("Bot {}", token))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: HTTP request failed: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        eprintln!(
            "Error: Failed to read messages ({}): {}",
            status, error_text
        );
        return ExitCode::FAILURE;
    }

    let messages: Vec<DiscordMessage> = match response.json() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error: Failed to parse response: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let message_list: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "author": {
                    "id": m.author.id,
                    "username": m.author.username,
                    "display_name": m.author.global_name.as_deref().unwrap_or(&m.author.username)
                },
                "content": m.content,
                "timestamp": m.timestamp
            })
        })
        .collect();

    let output = json!({
        "success": true,
        "channel_id": channel_id,
        "message_count": message_list.len(),
        "messages": message_list
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::SUCCESS
}
