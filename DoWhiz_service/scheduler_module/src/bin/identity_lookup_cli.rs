//! Identity Lookup CLI for agent use.
//!
//! Looks up DoWhiz-linked identities (email, GitHub) for Discord users.
//! Enables agents to find correct contact info for group project members.
//!
//! Usage:
//!   identity_lookup_cli discord-to-email <discord_user_id>
//!   identity_lookup_cli discord-to-github <discord_user_id>
//!   identity_lookup_cli guild-identities <guild_id> [--type email,github]

use serde::Serialize;
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
        "discord-to-email" => cmd_discord_to_email(&args[2..]),
        "discord-to-github" => cmd_discord_to_github(&args[2..]),
        "lookup" => cmd_lookup(&args[2..]),
        "guild-identities" => cmd_guild_identities(&args[2..]),
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
        r#"Identity Lookup CLI - Find DoWhiz-linked identities for Discord users

Usage:
  identity_lookup_cli <command> [options]

Commands:
  discord-to-email <discord_user_id>
    Look up the email address linked to a Discord user's DoWhiz account.
    Returns JSON: {{"discord_user_id": "...", "email": "..." or null}}

  discord-to-github <discord_user_id>
    Look up the GitHub username linked to a Discord user's DoWhiz account.
    Returns JSON: {{"discord_user_id": "...", "github": "..." or null}}

  lookup <discord_user_id>
    Look up all linked identities for a Discord user.
    Returns JSON: {{"discord_user_id": "...", "email": ..., "github": ..., "account_id": ...}}

  guild-identities <guild_id> [--type email,github]
    Look up identities for all Discord guild members who have DoWhiz accounts.
    Requires discord_cli to be available for listing guild members.
    --type: Comma-separated list of identity types (default: email,github)
    Returns JSON array of member identities.

Environment:
  SUPABASE_DB_URL         Required. Supabase database connection string.

Notes:
  - Only returns identities for users who have linked their DoWhiz account
  - Users without DoWhiz accounts will have null values
  - Use this before inviting users to Google Drive, GitHub repos, etc.
"#
    );
}

fn cmd_discord_to_email(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: Missing discord_user_id argument");
        eprintln!("Usage: identity_lookup_cli discord-to-email <discord_user_id>");
        return ExitCode::FAILURE;
    }

    let discord_user_id = &args[0];
    match lookup_identity_by_discord(discord_user_id, "email") {
        Ok(result) => {
            println!(
                "{}",
                json!({
                    "discord_user_id": discord_user_id,
                    "email": result
                })
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_discord_to_github(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: Missing discord_user_id argument");
        eprintln!("Usage: identity_lookup_cli discord-to-github <discord_user_id>");
        return ExitCode::FAILURE;
    }

    let discord_user_id = &args[0];
    match lookup_identity_by_discord(discord_user_id, "github") {
        Ok(result) => {
            println!(
                "{}",
                json!({
                    "discord_user_id": discord_user_id,
                    "github": result
                })
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_lookup(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: Missing discord_user_id argument");
        eprintln!("Usage: identity_lookup_cli lookup <discord_user_id>");
        return ExitCode::FAILURE;
    }

    let discord_user_id = &args[0];
    match lookup_all_identities_by_discord(discord_user_id) {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn cmd_guild_identities(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: Missing guild_id argument");
        eprintln!("Usage: identity_lookup_cli guild-identities <guild_id> [--type email,github]");
        return ExitCode::FAILURE;
    }

    let guild_id = &args[0];

    // Parse --type argument
    let mut types = vec!["email".to_string(), "github".to_string()];
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--type" && i + 1 < args.len() {
            types = args[i + 1]
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            i += 2;
        } else {
            i += 1;
        }
    }

    match lookup_guild_identities(guild_id, &types) {
        Ok(results) => {
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

#[derive(Serialize)]
struct IdentityResult {
    discord_user_id: String,
    account_id: Option<String>,
    email: Option<String>,
    github: Option<String>,
}

#[derive(Serialize)]
struct GuildMemberIdentity {
    discord_user_id: String,
    discord_username: Option<String>,
    account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github: Option<String>,
    has_dowhiz_account: bool,
}

/// Look up a specific identity type for a Discord user
fn lookup_identity_by_discord(
    discord_user_id: &str,
    identity_type: &str,
) -> Result<Option<String>, String> {
    let db_url =
        env::var("SUPABASE_DB_URL").map_err(|_| "SUPABASE_DB_URL environment variable not set")?;

    let mut client = postgres::Client::connect(&db_url, postgres::NoTls)
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    // First, find the account_id for this Discord user
    let account_row = client
        .query_opt(
            "SELECT account_id FROM account_identifiers
             WHERE identifier_type = 'discord' AND identifier = $1 AND verified = true",
            &[&discord_user_id],
        )
        .map_err(|e| format!("Database query failed: {}", e))?;

    let Some(account_row) = account_row else {
        return Ok(None); // No DoWhiz account linked
    };

    let account_id: uuid::Uuid = account_row.get(0);

    // Now look up the requested identity type
    let identity_row = client
        .query_opt(
            "SELECT identifier FROM account_identifiers
             WHERE account_id = $1 AND identifier_type = $2 AND verified = true",
            &[&account_id, &identity_type],
        )
        .map_err(|e| format!("Database query failed: {}", e))?;

    Ok(identity_row.map(|r| r.get(0)))
}

/// Look up all identities for a Discord user
fn lookup_all_identities_by_discord(discord_user_id: &str) -> Result<IdentityResult, String> {
    let db_url =
        env::var("SUPABASE_DB_URL").map_err(|_| "SUPABASE_DB_URL environment variable not set")?;

    let mut client = postgres::Client::connect(&db_url, postgres::NoTls)
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    // First, find the account_id for this Discord user
    let account_row = client
        .query_opt(
            "SELECT account_id FROM account_identifiers
             WHERE identifier_type = 'discord' AND identifier = $1 AND verified = true",
            &[&discord_user_id],
        )
        .map_err(|e| format!("Database query failed: {}", e))?;

    let Some(account_row) = account_row else {
        return Ok(IdentityResult {
            discord_user_id: discord_user_id.to_string(),
            account_id: None,
            email: None,
            github: None,
        });
    };

    let account_id: uuid::Uuid = account_row.get(0);

    // Get all identifiers for this account
    let rows = client
        .query(
            "SELECT identifier_type, identifier FROM account_identifiers
             WHERE account_id = $1 AND verified = true",
            &[&account_id],
        )
        .map_err(|e| format!("Database query failed: {}", e))?;

    let mut email = None;
    let mut github = None;

    for row in rows {
        let id_type: String = row.get(0);
        let identifier: String = row.get(1);
        match id_type.as_str() {
            "email" => email = Some(identifier),
            "github" => github = Some(identifier),
            _ => {}
        }
    }

    Ok(IdentityResult {
        discord_user_id: discord_user_id.to_string(),
        account_id: Some(account_id.to_string()),
        email,
        github,
    })
}

/// Look up identities for all members in a Discord guild
fn lookup_guild_identities(
    guild_id: &str,
    types: &[String],
) -> Result<Vec<GuildMemberIdentity>, String> {
    // First, get guild members using discord_cli
    let output = std::process::Command::new("discord_cli")
        .args(["list-guild-members", "--guild-id", guild_id, "--no-bots"])
        .output()
        .map_err(|e| format!("Failed to run discord_cli: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("discord_cli failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let members: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse guild members: {}", e))?;

    let members_array = members
        .as_array()
        .ok_or("Expected array of guild members")?;

    // Connect to database
    let db_url =
        env::var("SUPABASE_DB_URL").map_err(|_| "SUPABASE_DB_URL environment variable not set")?;

    let mut client = postgres::Client::connect(&db_url, postgres::NoTls)
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    let include_email = types.iter().any(|t| t == "email");
    let include_github = types.iter().any(|t| t == "github");

    let mut results = Vec::new();

    for member in members_array {
        let discord_user_id = member["user"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let discord_username = member["user"]["username"].as_str().map(|s| s.to_string());

        if discord_user_id.is_empty() {
            continue;
        }

        // Look up account
        let account_row = client
            .query_opt(
                "SELECT account_id FROM account_identifiers
                 WHERE identifier_type = 'discord' AND identifier = $1 AND verified = true",
                &[&discord_user_id],
            )
            .map_err(|e| format!("Database query failed: {}", e))?;

        let (account_id, email, github, has_account) = if let Some(row) = account_row {
            let account_id: uuid::Uuid = row.get(0);

            // Get identifiers
            let id_rows = client
                .query(
                    "SELECT identifier_type, identifier FROM account_identifiers
                     WHERE account_id = $1 AND verified = true",
                    &[&account_id],
                )
                .map_err(|e| format!("Database query failed: {}", e))?;

            let mut email = None;
            let mut github = None;

            for id_row in id_rows {
                let id_type: String = id_row.get(0);
                let identifier: String = id_row.get(1);
                match id_type.as_str() {
                    "email" if include_email => email = Some(identifier),
                    "github" if include_github => github = Some(identifier),
                    _ => {}
                }
            }

            (Some(account_id.to_string()), email, github, true)
        } else {
            (None, None, None, false)
        };

        results.push(GuildMemberIdentity {
            discord_user_id,
            discord_username,
            account_id,
            email,
            github,
            has_dowhiz_account: has_account,
        });
    }

    Ok(results)
}
