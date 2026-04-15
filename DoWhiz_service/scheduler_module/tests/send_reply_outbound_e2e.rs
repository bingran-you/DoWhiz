mod test_support;

use mockito::Matcher;
use scheduler_module::{
    channel::{Channel, ChannelMetadata},
    ModuleExecutor, Scheduler, SendReplyTask, TaskKind,
};
use send_emails_module::normalize_email_html;
use serde_json::json;
use std::env;
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = env::var(key).ok();
        env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => env::set_var(self.key, value),
            None => env::remove_var(self.key),
        }
    }
}

fn require_mongodb_uri(test_name: &str) -> Option<String> {
    dotenvy::dotenv().ok();
    match env::var("MONGODB_URI") {
        Ok(value) if !value.trim().is_empty() => {
            if let Some(address) = extract_mongodb_socket_addr(&value) {
                if TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_err() {
                    eprintln!("Skipping {test_name}; MongoDB at {address} is not reachable.");
                    return None;
                }
            }
            Some(value)
        }
        _ => {
            eprintln!("Skipping {test_name}; MONGODB_URI not set.");
            None
        }
    }
}

fn extract_mongodb_socket_addr(uri: &str) -> Option<std::net::SocketAddr> {
    let remainder = uri.trim().strip_prefix("mongodb://")?;
    let authority = remainder.split('/').next()?.split('?').next()?.trim();
    if authority.is_empty() {
        return None;
    }

    let host_port = authority.rsplit('@').next()?.split(',').next()?.trim();
    if host_port.is_empty() {
        return None;
    }

    let normalized = if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{host_port}:27017")
    };

    normalized.to_socket_addrs().ok()?.next()
}

fn write_text_file(
    temp: &TempDir,
    name: &str,
    contents: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = temp.path().join(name);
    fs::write(&path, contents)?;
    Ok(path)
}

fn create_attachments_dir(temp: &TempDir) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = temp.path().join("attachments");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn base_send_task(channel: Channel, html_path: PathBuf, attachments_dir: PathBuf) -> SendReplyTask {
    SendReplyTask {
        channel,
        subject: String::new(),
        html_path,
        attachments_dir,
        from: None,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        in_reply_to: None,
        references: None,
        archive_root: None,
        thread_epoch: None,
        thread_state_path: None,
        employee_id: None,
        channel_metadata: Default::default(),
    }
}

#[test]
fn send_reply_slack_uses_mock() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_MUTEX.lock().unwrap();
    let Some(_mongo_uri) = require_mongodb_uri("send_reply_slack_uses_mock") else {
        return Ok(());
    };
    let Some(mut server) = test_support::start_mockito_server("send_reply_slack_uses_mock") else {
        return Ok(());
    };

    let slack_mock = server
        .mock("POST", "/chat.postMessage")
        .match_header("authorization", "Bearer xoxb-test")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Regex("\\\"channel\\\":\\\"C123\\\"".to_string()))
        .match_body(Matcher::Regex("Hello Slack".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ok":true,"ts":"1700000000.123"}"#)
        .expect(1)
        .create();

    let _guard_token = EnvGuard::set("SLACK_BOT_TOKEN", "xoxb-test");
    let _guard_api = EnvGuard::set("SLACK_API_BASE_URL", server.url());

    let temp = TempDir::new()?;
    let html_path = write_text_file(&temp, "slack_message.txt", "Hello Slack")?;
    let attachments_dir = create_attachments_dir(&temp)?;

    let mut task = base_send_task(Channel::Slack, html_path, attachments_dir);
    task.to = vec!["C123".to_string()];

    let db_path = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&db_path, ModuleExecutor::default())?;
    scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::SendReply(task))?;
    scheduler.tick()?;

    slack_mock.assert();
    Ok(())
}

#[test]
fn send_reply_slack_includes_thread_ts_when_present() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_MUTEX.lock().unwrap();
    let Some(_mongo_uri) = require_mongodb_uri("send_reply_slack_includes_thread_ts_when_present")
    else {
        return Ok(());
    };
    let Some(mut server) =
        test_support::start_mockito_server("send_reply_slack_includes_thread_ts_when_present")
    else {
        return Ok(());
    };

    let slack_mock = server
        .mock("POST", "/chat.postMessage")
        .match_header("authorization", "Bearer xoxb-test")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Regex("\\\"channel\\\":\\\"C123\\\"".to_string()))
        .match_body(Matcher::Regex(
            "\\\"thread_ts\\\":\\\"1700000000\\.123\\\"".to_string(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ok":true,"ts":"1700000000.456"}"#)
        .expect(1)
        .create();

    let _guard_token = EnvGuard::set("SLACK_BOT_TOKEN", "xoxb-test");
    let _guard_api = EnvGuard::set("SLACK_API_BASE_URL", server.url());

    let temp = TempDir::new()?;
    let html_path = write_text_file(&temp, "slack_message.txt", "Hello Slack thread")?;
    let attachments_dir = create_attachments_dir(&temp)?;

    let mut task = base_send_task(Channel::Slack, html_path, attachments_dir);
    task.to = vec!["C123".to_string()];
    task.in_reply_to = Some("1700000000.123".to_string());

    let db_path = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&db_path, ModuleExecutor::default())?;
    scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::SendReply(task))?;
    scheduler.tick()?;

    slack_mock.assert();
    Ok(())
}

#[test]
fn send_reply_slack_team_scoped_prefers_employee_token_over_global(
) -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_MUTEX.lock().unwrap();
    let Some(_mongo_uri) =
        require_mongodb_uri("send_reply_slack_team_scoped_prefers_employee_token_over_global")
    else {
        return Ok(());
    };
    let Some(mut server) = test_support::start_mockito_server(
        "send_reply_slack_team_scoped_prefers_employee_token_over_global",
    ) else {
        return Ok(());
    };

    let slack_mock = server
        .mock("POST", "/chat.postMessage")
        .match_header("authorization", "Bearer xoxb-secondary")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Regex("\\\"channel\\\":\\\"C456\\\"".to_string()))
        .match_body(Matcher::Regex("Hello non-primary workspace".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ok":true,"ts":"1700000000.789"}"#)
        .expect(1)
        .create();

    let _guard_global_token = EnvGuard::set("SLACK_BOT_TOKEN", "xoxb-primary");
    let _guard_employee_token = EnvGuard::set("LITTLE_BEAR_SLACK_BOT_TOKEN", "xoxb-secondary");
    let _guard_api = EnvGuard::set("SLACK_API_BASE_URL", server.url());

    let temp = TempDir::new()?;
    let html_path = write_text_file(
        &temp,
        "slack_message_secondary.txt",
        "Hello non-primary workspace",
    )?;
    let attachments_dir = create_attachments_dir(&temp)?;

    let mut task = base_send_task(Channel::Slack, html_path, attachments_dir);
    task.employee_id = Some("little_bear".to_string());
    task.to = vec!["U123".to_string(), "C456".to_string()];
    task.channel_metadata = ChannelMetadata {
        slack_team_id: Some("TSECONDARY".to_string()),
        ..Default::default()
    };

    let db_path = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&db_path, ModuleExecutor::default())?;
    scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::SendReply(task))?;
    scheduler.tick()?;

    slack_mock.assert();
    Ok(())
}

#[test]
fn send_reply_discord_uses_mock() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_MUTEX.lock().unwrap();
    let Some(_mongo_uri) = require_mongodb_uri("send_reply_discord_uses_mock") else {
        return Ok(());
    };
    let Some(mut server) = test_support::start_mockito_server("send_reply_discord_uses_mock")
    else {
        return Ok(());
    };

    let discord_mock = server
        .mock("POST", "/api/v10/channels/987654/messages")
        .match_header("authorization", "Bot discord-test")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Regex("Hello Discord".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"msg-999","timestamp":"2024-01-01T00:00:00Z","channel_id":"987654"}"#)
        .expect(1)
        .create();

    let _guard_token = EnvGuard::set("DISCORD_BOT_TOKEN", "discord-test");
    let _guard_api = EnvGuard::set("DISCORD_API_BASE_URL", format!("{}/api/v10", server.url()));

    let temp = TempDir::new()?;
    let html_path = write_text_file(&temp, "discord_message.txt", "Hello Discord")?;
    let attachments_dir = create_attachments_dir(&temp)?;

    let mut task = base_send_task(Channel::Discord, html_path, attachments_dir);
    task.to = vec!["987654".to_string()];

    let db_path = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&db_path, ModuleExecutor::default())?;
    scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::SendReply(task))?;
    scheduler.tick()?;

    discord_mock.assert();
    Ok(())
}

#[test]
fn send_reply_discord_uploads_attachments_and_includes_blob_links(
) -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_MUTEX.lock().unwrap();
    let Some(_mongo_uri) =
        require_mongodb_uri("send_reply_discord_uploads_attachments_and_includes_blob_links")
    else {
        return Ok(());
    };
    let Some(mut server) = test_support::start_mockito_server(
        "send_reply_discord_uploads_attachments_and_includes_blob_links",
    ) else {
        return Ok(());
    };

    let azure_mock = server
        .mock("PUT", Matcher::Regex("^/ingestion-raw/.+".to_string()))
        .match_header("x-ms-blob-type", "BlockBlob")
        .with_status(201)
        .expect(1)
        .create();

    let discord_mock = server
        .mock("POST", "/api/v10/channels/987654/messages")
        .match_header("authorization", "Bot discord-test")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Regex("Hello Discord".to_string()))
        .match_body(Matcher::Regex("diagram\\.png".to_string()))
        .match_body(Matcher::Regex("ingestion-raw".to_string()))
        .match_body(Matcher::Regex("sig=test".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"msg-1000","timestamp":"2024-01-01T00:00:00Z","channel_id":"987654"}"#)
        .expect(1)
        .create();

    let _guard_token = EnvGuard::set("DISCORD_BOT_TOKEN", "discord-test");
    let _guard_api = EnvGuard::set("DISCORD_API_BASE_URL", format!("{}/api/v10", server.url()));
    let _guard_container = EnvGuard::set("AZURE_STORAGE_CONTAINER_INGEST", "ingestion-raw");
    let _guard_container_prefixed = EnvGuard::set(
        "SCALE_OLIVER_AZURE_STORAGE_CONTAINER_INGEST",
        "ingestion-raw",
    );
    let _guard_path_prefix = EnvGuard::set("RAW_PAYLOAD_PATH_PREFIX", "ingestion_raw");
    let _guard_path_prefix_prefixed =
        EnvGuard::set("SCALE_OLIVER_RAW_PAYLOAD_PATH_PREFIX", "ingestion_raw");
    let container_sas_url = format!("{}/ingestion-raw?sig=test", server.url());
    let _guard_container_sas_url =
        EnvGuard::set("AZURE_STORAGE_CONTAINER_SAS_URL", &container_sas_url);
    let _guard_container_sas_url_prefixed = EnvGuard::set(
        "SCALE_OLIVER_AZURE_STORAGE_CONTAINER_SAS_URL",
        &container_sas_url,
    );

    let temp = TempDir::new()?;
    let html_path = write_text_file(&temp, "discord_message.txt", "Hello Discord")?;
    let attachments_dir = create_attachments_dir(&temp)?;
    fs::write(attachments_dir.join("diagram.png"), b"fake-png-bytes")?;

    let mut task = base_send_task(Channel::Discord, html_path, attachments_dir.clone());
    task.to = vec!["987654".to_string()];

    let db_path = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&db_path, ModuleExecutor::default())?;
    scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::SendReply(task))?;
    scheduler.tick()?;

    azure_mock.assert();
    discord_mock.assert();

    let sidecar = fs::read_to_string(attachments_dir.join("diagram.png.azure_url"))?;
    assert!(sidecar.contains("/ingestion-raw/"));
    assert!(sidecar.contains("sig=test"));

    Ok(())
}

#[test]
fn send_reply_sms_uses_mock() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_MUTEX.lock().unwrap();
    let Some(_mongo_uri) = require_mongodb_uri("send_reply_sms_uses_mock") else {
        return Ok(());
    };
    let Some(mut server) = test_support::start_mockito_server("send_reply_sms_uses_mock") else {
        return Ok(());
    };

    let sms_mock = server
        .mock("POST", "/2010-04-01/Accounts/AC123/Messages.json")
        .match_header("authorization", Matcher::Regex("^Basic ".to_string()))
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex("To=%2B15551234567".to_string()),
            Matcher::Regex("From=%2B15557654321".to_string()),
            Matcher::Regex("Body=Hello".to_string()),
        ]))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"sid":"SM123","status":"queued"}"#)
        .expect(1)
        .create();

    let _guard_sid = EnvGuard::set("TWILIO_ACCOUNT_SID", "AC123");
    let _guard_token = EnvGuard::set("TWILIO_AUTH_TOKEN", "twilio-test");
    let _guard_api = EnvGuard::set("TWILIO_API_BASE_URL", server.url());

    let temp = TempDir::new()?;
    let html_path = write_text_file(&temp, "sms_message.txt", "Hello SMS")?;
    let attachments_dir = create_attachments_dir(&temp)?;

    let mut task = base_send_task(Channel::Sms, html_path, attachments_dir);
    task.from = Some("+15557654321".to_string());
    task.to = vec!["+15551234567".to_string()];

    let db_path = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&db_path, ModuleExecutor::default())?;
    scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::SendReply(task))?;
    scheduler.tick()?;

    sms_mock.assert();
    Ok(())
}

#[test]
fn send_reply_email_applies_branded_shell_before_send() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_MUTEX.lock().unwrap();
    let Some(_mongo_uri) =
        require_mongodb_uri("send_reply_email_applies_branded_shell_before_send")
    else {
        return Ok(());
    };
    let Some(mut server) =
        test_support::start_mockito_server("send_reply_email_applies_branded_shell_before_send")
    else {
        return Ok(());
    };

    let raw_html = r#"<div style="max-width: 520px; margin: 0 auto;"><p>你好，这里有一个超长字符串用于检查自动换行：LONGTOKENLONGTOKENLONGTOKENLONGTOKENLONGTOKEN</p></div>"#;
    let expected_html = normalize_email_html("Project update", raw_html);
    let expected_payload = json!({
        "From": "sender@example.com",
        "To": "user@example.com",
        "Bcc": "sender@example.com",
        "Subject": "Project update",
        "TextBody": "你好，这里有一个超长字符串用于检查自动换行：LONGTOKENLONGTOKENLONGTOKENLONGTOKENLONGTOKEN",
        "HtmlBody": expected_html,
    });

    let email_mock = server
        .mock("POST", "/email")
        .match_header("x-postmark-server-token", "test-token")
        .match_header("accept", "application/json")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Json(expected_payload))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "To": "user@example.com",
                "SubmittedAt": "2024-01-01T00:00:00Z",
                "MessageID": "test-message-id",
                "ErrorCode": 0,
                "Message": "OK",
            })
            .to_string(),
        )
        .expect(1)
        .create();

    let _guard_token = EnvGuard::set("POSTMARK_SERVER_TOKEN", "test-token");
    let _guard_api = EnvGuard::set("POSTMARK_API_BASE_URL", server.url());

    let temp = TempDir::new()?;
    let html_path = write_text_file(&temp, "reply_email_draft.html", raw_html)?;
    let attachments_dir = create_attachments_dir(&temp)?;

    let mut task = base_send_task(Channel::Email, html_path.clone(), attachments_dir);
    task.subject = "Project update".to_string();
    task.from = Some("sender@example.com".to_string());
    task.to = vec!["user@example.com".to_string()];

    let db_path = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&db_path, ModuleExecutor::default())?;
    scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::SendReply(task))?;
    scheduler.tick()?;

    let normalized_file = fs::read_to_string(&html_path)?;
    assert_eq!(normalized_file, expected_html);

    email_mock.assert();
    Ok(())
}
