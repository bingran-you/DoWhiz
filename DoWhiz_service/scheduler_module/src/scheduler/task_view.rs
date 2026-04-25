use std::fs;
use std::path::{Path, PathBuf};

use mongodb::bson::Document;
use serde_json::Value;

use super::types::{ScheduledTask, SchedulerError};

const REQUEST_SUMMARY_MAX_CHARS: usize = 72;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct TaskSenderSummary {
    pub sender: Option<String>,
    pub sender_name: Option<String>,
}

pub(crate) fn deserialize_task_document(
    task_doc: &Document,
) -> Result<ScheduledTask, SchedulerError> {
    let task_json = task_doc.get_str("task_json").map_err(|err| {
        SchedulerError::Storage(format!("missing task_json for task document: {err}"))
    })?;
    serde_json::from_str(task_json)
        .map_err(|err| SchedulerError::Storage(format!("invalid task_json: {err}")))
}

pub(crate) fn derive_request_summary(task_doc: &Document) -> Option<String> {
    let task_value = task_doc_value(task_doc)?;
    let task_kind = task_value.pointer("/kind/type").and_then(|v| v.as_str())?;

    match task_kind {
        "send_email" => task_value
            .pointer("/kind/subject")
            .and_then(|v| v.as_str())
            .and_then(normalize_summary_text),
        "run_task" => {
            let workspace_dir = task_value
                .pointer("/kind/workspace_dir")
                .and_then(|v| v.as_str())?;
            let channel = task_value
                .pointer("/kind/channel")
                .and_then(|v| v.as_str())
                .or_else(|| task_doc.get_str("channel").ok())
                .unwrap_or("");
            let thread_epoch = task_value
                .pointer("/kind/thread_epoch")
                .and_then(|v| v.as_u64());
            derive_run_task_summary(Path::new(workspace_dir), channel, thread_epoch)
        }
        _ => None,
    }
}

pub(crate) fn derive_task_sender_summary(task_doc: &Document) -> TaskSenderSummary {
    let task_value = match task_doc_value(task_doc) {
        Some(task_value) => task_value,
        None => return TaskSenderSummary::default(),
    };
    let task_kind = task_value.pointer("/kind/type").and_then(|v| v.as_str());
    if task_kind != Some("run_task") {
        return TaskSenderSummary::default();
    }

    let workspace_dir = match task_value
        .pointer("/kind/workspace_dir")
        .and_then(|v| v.as_str())
    {
        Some(value) => PathBuf::from(value),
        None => return TaskSenderSummary::default(),
    };
    let channel = task_value
        .pointer("/kind/channel")
        .and_then(|v| v.as_str())
        .or_else(|| task_doc.get_str("channel").ok())
        .unwrap_or("");
    let thread_epoch = task_value
        .pointer("/kind/thread_epoch")
        .and_then(|v| v.as_u64());
    let requester_identifier = task_value
        .pointer("/kind/requester_identifier")
        .and_then(|v| v.as_str())
        .and_then(normalize_optional_string);
    let reply_to_first = task_value
        .pointer("/kind/reply_to")
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
        .and_then(|v| v.as_str())
        .and_then(normalize_optional_string);

    let incoming_dir = workspace_dir.join("incoming_email");
    let mut summary = if incoming_dir.exists() {
        derive_run_task_sender_summary(&incoming_dir, channel, thread_epoch)
    } else {
        TaskSenderSummary::default()
    };

    if summary.sender.is_none() {
        summary.sender = requester_identifier.or(reply_to_first);
    }
    if summary.sender_name.is_none() {
        summary.sender_name = summary.sender.clone();
    }
    summary
}

pub(crate) fn default_routine_name(channel: &str) -> String {
    match channel {
        "slack" => "Scheduled Slack work".to_string(),
        "discord" => "Scheduled Discord work".to_string(),
        "email" => "Scheduled email work".to_string(),
        "google_docs" => "Scheduled Google Docs work".to_string(),
        "google_sheets" => "Scheduled Google Sheets work".to_string(),
        "google_slides" => "Scheduled Google Slides work".to_string(),
        "lark" => "Scheduled Lark work".to_string(),
        _ => "Scheduled Oliver work".to_string(),
    }
}

pub(crate) fn normalize_discord_summary_text(raw: &str) -> Option<String> {
    for line in raw.lines() {
        let stripped = strip_discord_mentions(line.trim());
        if !stripped.is_empty() {
            return clean_summary_line(&stripped);
        }
    }
    None
}

pub(crate) fn strip_discord_mentions(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' && chars.peek() == Some(&'@') {
            chars.next();
            if chars.peek() == Some(&'!') {
                chars.next();
            }
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == '>' {
                    break;
                }
            }
        } else {
            result.push(ch);
        }
    }

    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn task_doc_value(task_doc: &Document) -> Option<Value> {
    let task_json = task_doc.get_str("task_json").ok()?;
    serde_json::from_str(task_json).ok()
}

fn derive_run_task_summary(
    workspace_dir: &Path,
    channel: &str,
    thread_epoch: Option<u64>,
) -> Option<String> {
    let incoming_dir = workspace_dir.join("incoming_email");
    if !incoming_dir.exists() {
        return None;
    }

    match channel {
        "email" => derive_email_summary(&incoming_dir),
        "google_docs" => derive_google_workspace_summary(&incoming_dir, "gdocs", thread_epoch),
        "google_sheets" => derive_google_workspace_summary(&incoming_dir, "gsheets", thread_epoch),
        "google_slides" => derive_google_workspace_summary(&incoming_dir, "gslides", thread_epoch),
        "discord" => derive_discord_summary(&incoming_dir, thread_epoch),
        "slack" => derive_text_file_summary(&incoming_dir, "_slack_message.txt", thread_epoch),
        "sms" => derive_text_file_summary(&incoming_dir, "_sms_message.txt", thread_epoch),
        "bluebubbles" => {
            derive_text_file_summary(&incoming_dir, "_bluebubbles_message.txt", thread_epoch)
        }
        "telegram" => derive_header_text_file_summary(&incoming_dir, "_telegram.txt", thread_epoch),
        "whatsapp" => derive_header_text_file_summary(&incoming_dir, "_whatsapp.txt", thread_epoch),
        "wechat" => derive_header_text_file_summary(&incoming_dir, "_wechat.txt", thread_epoch),
        "lark" => derive_header_text_file_summary(&incoming_dir, "_lark.txt", thread_epoch),
        _ => None,
    }
}

fn derive_run_task_sender_summary(
    incoming_dir: &Path,
    channel: &str,
    thread_epoch: Option<u64>,
) -> TaskSenderSummary {
    match channel {
        "email" => derive_email_sender_summary(incoming_dir),
        "discord" => {
            derive_json_meta_sender_summary(incoming_dir, "_discord_meta.json", thread_epoch)
        }
        "slack" => derive_json_meta_sender_summary(incoming_dir, "_slack_meta.json", thread_epoch),
        "sms" => derive_json_meta_sender_summary(incoming_dir, "_sms_meta.json", thread_epoch),
        "bluebubbles" => {
            derive_json_meta_sender_summary(incoming_dir, "_bluebubbles_meta.json", thread_epoch)
        }
        "notion" => {
            derive_json_meta_sender_summary(incoming_dir, "_notion_meta.json", thread_epoch)
        }
        "google_docs" => {
            derive_json_meta_sender_summary(incoming_dir, "_gdocs_meta.json", thread_epoch)
        }
        "google_sheets" => {
            derive_json_meta_sender_summary(incoming_dir, "_gsheets_meta.json", thread_epoch)
        }
        "google_slides" => {
            derive_json_meta_sender_summary(incoming_dir, "_gslides_meta.json", thread_epoch)
        }
        "telegram" => derive_header_sender_summary(incoming_dir, "_telegram.txt", thread_epoch),
        "whatsapp" => derive_header_sender_summary(incoming_dir, "_whatsapp.txt", thread_epoch),
        "wechat" => derive_header_sender_summary(incoming_dir, "_wechat.txt", thread_epoch),
        "lark" => derive_header_sender_summary(incoming_dir, "_lark.txt", thread_epoch),
        _ => TaskSenderSummary::default(),
    }
}

fn derive_email_summary(incoming_dir: &Path) -> Option<String> {
    let payload_path = incoming_dir.join("postmark_payload.json");
    let raw_payload = fs::read_to_string(payload_path).ok()?;
    let payload_value: serde_json::Value = serde_json::from_str(&raw_payload).ok()?;

    payload_value
        .get("Subject")
        .and_then(|v| v.as_str())
        .and_then(normalize_summary_text)
        .or_else(|| {
            payload_value
                .get("StrippedTextReply")
                .and_then(|v| v.as_str())
                .and_then(normalize_summary_text)
        })
        .or_else(|| {
            payload_value
                .get("TextBody")
                .and_then(|v| v.as_str())
                .and_then(normalize_summary_text)
        })
}

fn derive_email_sender_summary(incoming_dir: &Path) -> TaskSenderSummary {
    let payload_path = incoming_dir.join("postmark_payload.json");
    let raw_payload = match fs::read_to_string(payload_path) {
        Ok(value) => value,
        Err(_) => return TaskSenderSummary::default(),
    };
    let payload_value: serde_json::Value = match serde_json::from_str(&raw_payload) {
        Ok(value) => value,
        Err(_) => return TaskSenderSummary::default(),
    };

    let sender = payload_value
        .pointer("/FromFull/Email")
        .and_then(|v| v.as_str())
        .and_then(normalize_optional_string)
        .or_else(|| {
            payload_value
                .get("From")
                .and_then(|v| v.as_str())
                .and_then(extract_email_from_header)
        });
    let sender_name = payload_value
        .pointer("/FromFull/Name")
        .and_then(|v| v.as_str())
        .and_then(normalize_optional_string)
        .or_else(|| {
            payload_value
                .get("From")
                .and_then(|v| v.as_str())
                .and_then(extract_display_name_from_header)
        })
        .or_else(|| sender.clone());

    TaskSenderSummary {
        sender,
        sender_name,
    }
}

fn derive_google_workspace_summary(
    incoming_dir: &Path,
    file_prefix: &str,
    thread_epoch: Option<u64>,
) -> Option<String> {
    let comment_suffix = format!("_{}_comment.json", file_prefix);
    let comment_path = file_with_epoch_or_latest(incoming_dir, &comment_suffix, thread_epoch);
    if let Some(comment_path) = comment_path {
        if let Ok(raw_comment) = fs::read_to_string(comment_path) {
            if let Ok(comment) = serde_json::from_str::<serde_json::Value>(&raw_comment) {
                if let Some(summary) = comment
                    .get("content")
                    .and_then(|v| v.as_str())
                    .and_then(normalize_summary_text)
                {
                    return Some(summary);
                }
            }
        }
    }

    let meta_suffix = format!("_{}_meta.json", file_prefix);
    let meta_path = file_with_epoch_or_latest(incoming_dir, &meta_suffix, thread_epoch)?;
    let raw_meta = fs::read_to_string(meta_path).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&raw_meta).ok()?;
    let file_name = meta.get("file_name").and_then(|v| v.as_str())?;

    normalize_summary_text(&format!("Comment on {}", file_name))
}

fn derive_discord_summary(incoming_dir: &Path, thread_epoch: Option<u64>) -> Option<String> {
    let raw = read_text_by_epoch_or_latest(incoming_dir, "_discord_message.txt", thread_epoch)?;
    let content = if let Some((_, user_section)) = raw.split_once("User message:\n") {
        user_section
    } else {
        &raw
    };
    normalize_discord_summary_text(content)
}

fn derive_text_file_summary(
    incoming_dir: &Path,
    suffix: &str,
    thread_epoch: Option<u64>,
) -> Option<String> {
    let raw = read_text_by_epoch_or_latest(incoming_dir, suffix, thread_epoch)?;
    normalize_summary_text(&raw)
}

fn derive_header_text_file_summary(
    incoming_dir: &Path,
    suffix: &str,
    thread_epoch: Option<u64>,
) -> Option<String> {
    let raw = read_text_by_epoch_or_latest(incoming_dir, suffix, thread_epoch)?;
    extract_header_file_body_summary(&raw).or_else(|| normalize_summary_text(&raw))
}

fn derive_json_meta_sender_summary(
    incoming_dir: &Path,
    suffix: &str,
    thread_epoch: Option<u64>,
) -> TaskSenderSummary {
    let path = match file_with_epoch_or_latest(incoming_dir, suffix, thread_epoch) {
        Some(path) => path,
        None => return TaskSenderSummary::default(),
    };
    let raw = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(_) => return TaskSenderSummary::default(),
    };
    let meta: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return TaskSenderSummary::default(),
    };

    TaskSenderSummary {
        sender: meta
            .get("sender")
            .and_then(|v| v.as_str())
            .and_then(normalize_optional_string),
        sender_name: meta
            .get("sender_name")
            .and_then(|v| v.as_str())
            .and_then(normalize_optional_string)
            .or_else(|| {
                meta.get("sender")
                    .and_then(|v| v.as_str())
                    .and_then(normalize_optional_string)
            }),
    }
}

fn derive_header_sender_summary(
    incoming_dir: &Path,
    suffix: &str,
    thread_epoch: Option<u64>,
) -> TaskSenderSummary {
    let raw = match read_text_by_epoch_or_latest(incoming_dir, suffix, thread_epoch) {
        Some(value) => value,
        None => return TaskSenderSummary::default(),
    };

    let sender = raw
        .lines()
        .find_map(|line| line.strip_prefix("From:"))
        .map(str::trim)
        .and_then(normalize_optional_string);

    TaskSenderSummary {
        sender: sender.clone(),
        sender_name: sender,
    }
}

fn read_text_by_epoch_or_latest(
    incoming_dir: &Path,
    suffix: &str,
    thread_epoch: Option<u64>,
) -> Option<String> {
    let path = file_with_epoch_or_latest(incoming_dir, suffix, thread_epoch)?;
    fs::read_to_string(path).ok()
}

fn file_with_epoch_or_latest(
    incoming_dir: &Path,
    suffix: &str,
    thread_epoch: Option<u64>,
) -> Option<PathBuf> {
    if let Some(epoch) = thread_epoch {
        if let Some(path) = find_file_by_epoch(incoming_dir, suffix, epoch) {
            return Some(path);
        }
    }
    latest_file_with_suffix(incoming_dir, &[suffix])
}

fn find_file_by_epoch(incoming_dir: &Path, suffix: &str, epoch: u64) -> Option<PathBuf> {
    for entry in fs::read_dir(incoming_dir).ok()? {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(suffix) {
            continue;
        }
        let prefix = name.strip_suffix(suffix)?;
        if let Ok(file_epoch) = prefix.parse::<u64>() {
            if file_epoch == epoch {
                return Some(entry.path());
            }
        }
    }
    None
}

fn latest_file_with_suffix(incoming_dir: &Path, suffixes: &[&str]) -> Option<PathBuf> {
    let mut matches: Vec<(String, PathBuf)> = Vec::new();

    for entry in fs::read_dir(incoming_dir).ok()? {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if suffixes.iter().any(|suffix| name.ends_with(suffix)) {
            matches.push((name, entry.path()));
        }
    }

    matches.sort_by(|a, b| a.0.cmp(&b.0));
    matches.pop().map(|(_, path)| path)
}

fn extract_header_file_body_summary(raw: &str) -> Option<String> {
    let mut body_started = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            body_started = true;
            continue;
        }

        if !body_started
            && (trimmed.starts_with("From:")
                || trimmed.starts_with("Date:")
                || trimmed.starts_with("To:")
                || trimmed.starts_with("Subject:"))
        {
            continue;
        }

        return clean_summary_line(trimmed);
    }

    None
}

fn normalize_summary_text(raw: &str) -> Option<String> {
    let first_line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    clean_summary_line(first_line)
}

fn clean_summary_line(line: &str) -> Option<String> {
    let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    Some(truncate_summary(&compact, REQUEST_SUMMARY_MAX_CHARS))
}

fn truncate_summary(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut output = String::new();

    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => output.push(ch),
            None => return output,
        }
    }

    if chars.next().is_some() {
        output.push_str("...");
    }

    output
}

fn normalize_optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_email_from_header(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((_, rest)) = trimmed.split_once('<') {
        return rest
            .split_once('>')
            .and_then(|(email, _)| normalize_optional_string(email));
    }
    normalize_optional_string(trimmed)
}

fn extract_display_name_from_header(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((name, _)) = trimmed.split_once('<') {
        let compact = name.trim().trim_matches('"');
        return normalize_optional_string(compact);
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use mongodb::bson::doc;
    use tempfile::TempDir;

    use super::{
        derive_request_summary, derive_task_sender_summary, normalize_discord_summary_text,
        strip_discord_mentions,
    };

    #[test]
    fn derive_request_summary_prefers_send_email_subject() {
        let task_json = serde_json::json!({
            "kind": {
                "type": "send_email",
                "subject": "Weekly analytics summary and next actions"
            }
        })
        .to_string();
        let doc = doc! {
            "task_json": task_json,
            "channel": "email",
        };

        let summary = derive_request_summary(&doc);
        assert_eq!(
            summary.as_deref(),
            Some("Weekly analytics summary and next actions")
        );
    }

    #[test]
    fn derive_request_summary_reads_latest_slack_message() {
        let temp = TempDir::new().expect("tempdir");
        let incoming_dir = temp.path().join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("create incoming_email");
        fs::write(
            incoming_dir.join("00001_slack_message.txt"),
            "Earlier message",
        )
        .expect("write old message");
        fs::write(
            incoming_dir.join("00002_slack_message.txt"),
            "Please draft a concise project update for the team.",
        )
        .expect("write latest message");

        let task_json = serde_json::json!({
            "kind": {
                "type": "run_task",
                "workspace_dir": temp.path().to_string_lossy(),
                "channel": "slack"
            }
        })
        .to_string();
        let doc = doc! {
            "task_json": task_json,
            "channel": "slack",
        };

        let summary = derive_request_summary(&doc);
        assert_eq!(
            summary.as_deref(),
            Some("Please draft a concise project update for the team.")
        );
    }

    #[test]
    fn derive_task_sender_summary_reads_slack_meta_file() {
        let temp = TempDir::new().expect("tempdir");
        let incoming_dir = temp.path().join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("create incoming_email");
        fs::write(
            incoming_dir.join("00002_slack_meta.json"),
            serde_json::json!({
                "sender": "U12345",
                "sender_name": "Bingran"
            })
            .to_string(),
        )
        .expect("write slack meta");

        let task_json = serde_json::json!({
            "kind": {
                "type": "run_task",
                "workspace_dir": temp.path().to_string_lossy(),
                "channel": "slack",
                "reply_to": ["U12345", "C999"],
                "requester_identifier": "bingran@dowhiz.com"
            }
        })
        .to_string();
        let doc = doc! {
            "task_json": task_json,
            "channel": "slack",
        };

        let sender = derive_task_sender_summary(&doc);
        assert_eq!(sender.sender.as_deref(), Some("U12345"));
        assert_eq!(sender.sender_name.as_deref(), Some("Bingran"));
    }

    #[test]
    fn derive_task_sender_summary_falls_back_to_reply_to() {
        let temp = TempDir::new().expect("tempdir");
        let incoming_dir = temp.path().join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("create incoming_email");
        fs::write(
            incoming_dir.join("0001_lark.txt"),
            "From: ou_sender\nDate: 2026-03-13T20:00:00Z\n\nReview the attached budget and flag risks.",
        )
        .expect("write lark text");

        let task_json = serde_json::json!({
            "kind": {
                "type": "run_task",
                "workspace_dir": temp.path().to_string_lossy(),
                "channel": "lark",
                "reply_to": ["ou_sender"]
            }
        })
        .to_string();
        let doc = doc! {
            "task_json": task_json,
            "channel": "lark",
        };

        let sender = derive_task_sender_summary(&doc);
        assert_eq!(sender.sender.as_deref(), Some("ou_sender"));
        assert_eq!(sender.sender_name.as_deref(), Some("ou_sender"));
    }

    #[test]
    fn normalize_discord_summary_text_strips_mentions() {
        let summary =
            normalize_discord_summary_text("<@12345> Please review the launch checklist.");
        assert_eq!(
            summary.as_deref(),
            Some("Please review the launch checklist.")
        );
    }

    #[test]
    fn strip_discord_mentions_handles_nickname_mentions() {
        let stripped = strip_discord_mentions("hello <@!123456> world");
        assert_eq!(stripped, "hello world");
    }
}
