use send_emails_module::SendEmailParams;
use serde::Deserialize;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const DEV_ALERT_EMAIL: &str = "822334legacy@gmail.com";
const ALERT_SENDER: &str = "oliver@dowhiz.com";
const CONSECUTIVE_FAILURE_THRESHOLD: usize = 2;

static CONSECUTIVE_FAILED_PASSES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Deserialize)]
struct DateWrapper {
    #[serde(rename = "$date")]
    date: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReconciliationFailure {
    task_id: String,
    started_at: DateWrapper,
    finished_at: DateWrapper,
    error_message: String,
}

#[derive(Debug, Clone)]
pub struct ReconciliationFailureRecord {
    pub task_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub error_message: String,
}

impl From<ReconciliationFailure> for ReconciliationFailureRecord {
    fn from(f: ReconciliationFailure) -> Self {
        Self {
            task_id: f.task_id,
            started_at: f.started_at.date,
            finished_at: f.finished_at.date,
            error_message: f.error_message,
        }
    }
}

pub fn query_recent_reconciliation_failures(limit: usize) -> Vec<ReconciliationFailureRecord> {
    let mongodb_uri = match std::env::var("MONGODB_URI") {
        Ok(uri) if !uri.trim().is_empty() => uri,
        _ => {
            tracing::warn!("MONGODB_URI not set, cannot query reconciliation failures");
            return Vec::new();
        }
    };

    let db_name = std::env::var("MONGODB_DATABASE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            let target = std::env::var("DEPLOY_TARGET")
                .ok()
                .map(|v| v.trim().to_ascii_lowercase())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "production".to_string());
            let employee = std::env::var("EMPLOYEE_ID")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "default".to_string());
            format!("dowhiz_{}_{}", target, employee)
        });

    let query = format!(
        r#"db.getSiblingDB("{}").task_executions.find(
            {{status: "failed", error_message: /reconciled/}},
            {{task_id: 1, started_at: 1, finished_at: 1, error_message: 1, _id: 0}}
        ).limit({}).toArray()"#,
        db_name, limit
    );

    let output = match Command::new("mongosh")
        .arg(&mongodb_uri)
        .arg("--quiet")
        .arg("--json=relaxed")
        .arg("--eval")
        .arg(&query)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            tracing::error!("failed to run mongosh: {}", e);
            return Vec::new();
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!("mongosh query failed: {}", stderr);
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    match serde_json::from_str::<Vec<ReconciliationFailure>>(&stdout) {
        Ok(failures) => failures.into_iter().map(Into::into).collect(),
        Err(e) => {
            tracing::error!("failed to parse mongosh output: {}", e);
            Vec::new()
        }
    }
}

pub fn format_reconciliation_alert_html(
    failures: &[ReconciliationFailureRecord],
    consecutive_passes: usize,
) -> String {
    let mut html = String::new();

    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("<style>\n");
    html.push_str("body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 20px; }\n");
    html.push_str("h1 { color: #d32f2f; }\n");
    html.push_str("table { border-collapse: collapse; width: 100%; margin: 20px 0; }\n");
    html.push_str("th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }\n");
    html.push_str("th { background-color: #f5f5f5; }\n");
    html.push_str("pre { background: #f5f5f5; padding: 12px; overflow-x: auto; white-space: pre-wrap; word-wrap: break-word; font-size: 12px; }\n");
    html.push_str(".detail-section { margin: 20px 0; padding: 15px; border: 1px solid #e0e0e0; border-radius: 4px; }\n");
    html.push_str(".task-id { font-family: monospace; font-weight: bold; }\n");
    html.push_str("</style>\n");
    html.push_str("</head>\n<body>\n");

    html.push_str(&format!(
        "<h1>Reconciliation Alert: failures in {} consecutive passes</h1>\n",
        consecutive_passes
    ));
    html.push_str(&format!(
        "<p>Generated at: {}</p>\n",
        chrono::Utc::now().to_rfc3339()
    ));

    html.push_str("<h2>Summary</h2>\n");
    html.push_str("<table>\n");
    html.push_str("<tr><th>Task ID</th><th>Started</th><th>Finished</th><th>Error (truncated)</th></tr>\n");

    for failure in failures {
        let error_truncated = if failure.error_message.len() > 80 {
            format!("{}...", &failure.error_message[..80])
        } else {
            failure.error_message.clone()
        };
        let error_escaped = html_escape(&error_truncated);

        html.push_str(&format!(
            "<tr><td class=\"task-id\">{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            &failure.task_id,
            format_datetime(&failure.started_at),
            format_datetime(&failure.finished_at),
            error_escaped
        ));
    }
    html.push_str("</table>\n");

    html.push_str("<h2>Full Error Details</h2>\n");
    for failure in failures {
        html.push_str("<div class=\"detail-section\">\n");
        html.push_str(&format!(
            "<h3>Task: <span class=\"task-id\">{}</span></h3>\n",
            &failure.task_id
        ));
        html.push_str(&format!(
            "<p><strong>Started:</strong> {} | <strong>Finished:</strong> {}</p>\n",
            format_datetime(&failure.started_at),
            format_datetime(&failure.finished_at)
        ));
        html.push_str("<pre>");
        html.push_str(&html_escape(&failure.error_message));
        html.push_str("</pre>\n");
        html.push_str("</div>\n");
    }

    html.push_str("</body>\n</html>\n");
    html
}

pub fn get_dev_alert_email() -> &'static str {
    DEV_ALERT_EMAIL
}

pub fn check_and_send_alert_if_needed(failures_this_pass: usize) {
    if failures_this_pass == 0 {
        let prev = CONSECUTIVE_FAILED_PASSES.swap(0, Ordering::SeqCst);
        if prev > 0 {
            tracing::debug!("reconciliation pass had 0 failures, reset consecutive counter from {}", prev);
        }
        return;
    }

    let consecutive = CONSECUTIVE_FAILED_PASSES.fetch_add(1, Ordering::SeqCst) + 1;
    tracing::debug!(
        "reconciliation pass had {} failures, consecutive failed passes: {}/{}",
        failures_this_pass,
        consecutive,
        CONSECUTIVE_FAILURE_THRESHOLD
    );

    if consecutive < CONSECUTIVE_FAILURE_THRESHOLD {
        return;
    }

    tracing::warn!(
        "reconciliation failures detected in {} consecutive passes, sending alert",
        consecutive
    );

    if let Err(e) = send_reconciliation_alert(consecutive) {
        tracing::error!("failed to send reconciliation alert: {}", e);
    } else {
        CONSECUTIVE_FAILED_PASSES.store(0, Ordering::SeqCst);
    }
}

fn send_reconciliation_alert(consecutive_passes: usize) -> Result<(), String> {
    let failures = query_recent_reconciliation_failures(20);
    if failures.is_empty() {
        tracing::warn!("no reconciliation failures found in query, skipping alert");
        return Ok(());
    }

    let html = format_reconciliation_alert_html(&failures, consecutive_passes);

    let temp_dir = std::env::temp_dir();
    let html_path = temp_dir.join("reconciliation_alert.html");
    let attachments_dir = temp_dir.join("reconciliation_alert_attachments");

    std::fs::write(&html_path, &html).map_err(|e| format!("failed to write HTML: {}", e))?;
    std::fs::create_dir_all(&attachments_dir)
        .map_err(|e| format!("failed to create attachments dir: {}", e))?;

    let employee_id = std::env::var("EMPLOYEE_ID").unwrap_or_else(|_| "unknown".to_string());
    let subject = format!(
        "[Alert] Reconciliation failures - {} consecutive passes ({})",
        consecutive_passes, employee_id
    );

    let params = SendEmailParams {
        subject,
        html_path,
        attachments_dir,
        from: Some(ALERT_SENDER.to_string()),
        to: vec![DEV_ALERT_EMAIL.to_string()],
        cc: Vec::new(),
        bcc: Vec::new(),
        in_reply_to: None,
        references: None,
        reply_to: None,
    };

    match send_emails_module::send_email(&params) {
        Ok(response) => {
            tracing::info!(
                "sent reconciliation alert email, message_id={}",
                response.message_id
            );
            Ok(())
        }
        Err(e) => Err(format!("postmark error: {}", e)),
    }
}

fn format_datetime(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_datetime_parses_iso8601() {
        let iso = "2026-04-08T03:13:46.662Z";
        let formatted = format_datetime(iso);
        assert!(formatted.contains("2026-04-08"));
        assert!(formatted.contains("03:13:46"));
    }

    #[test]
    fn html_escape_handles_special_chars() {
        let input = "<script>alert('xss')</script>";
        let escaped = html_escape(input);
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert!(escaped.contains("&lt;"));
        assert!(escaped.contains("&gt;"));
    }

    #[test]
    fn format_alert_html_generates_valid_structure() {
        let failures = vec![
            ReconciliationFailureRecord {
                task_id: "test-task-1".to_string(),
                started_at: "2026-04-08T03:13:46.662Z".to_string(),
                finished_at: "2026-04-08T05:54:12.730Z".to_string(),
                error_message: "reconciled stale running execution".to_string(),
            },
        ];

        let html = format_reconciliation_alert_html(&failures, 2);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("2 consecutive passes"));
        assert!(html.contains("test-task-1"));
        assert!(html.contains("Summary"));
        assert!(html.contains("Full Error Details"));
        assert!(html.contains("<table>"));
        assert!(html.contains("<pre>"));
    }
}
