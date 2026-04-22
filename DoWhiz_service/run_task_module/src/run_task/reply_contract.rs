use std::fs;
use std::path::Path;

use serde_json::Value;

use super::errors::RunTaskError;

const REQUIRED_INVESTMENT_LABELS: &[(&str, bool)] = &[
    ("Rating", true),
    ("Horizon", true),
    ("Confidence", true),
    ("Timing Verdict", true),
    ("Verified Facts", false),
    ("Derived Metrics", false),
    ("Bull Case", false),
    ("Base Case", false),
    ("Bear Case", false),
    ("Add Criteria", true),
    ("Invalidation Criteria", true),
    ("Biggest Near-Term Risk", true),
    ("Biggest Long-Term Strength", true),
];

const FINANCE_CONTEXT_KEYWORDS: &[&str] = &[
    "stock",
    "etf",
    "ticker",
    "earnings",
    "position",
    "shares",
    "valuation",
    "market cap",
    "revenue",
    "eps",
    "free cash flow",
    "fcf",
];

const INVESTMENT_INTENT_KEYWORDS: &[&str] = &[
    "good time to buy",
    "should i buy",
    "should i sell",
    "worth buying",
    "buy before",
    "sell before",
    "deep research",
    "analyze",
    "analysis",
    "investment",
    "investing",
    "starter position",
    "starter only",
    "buy now",
    "wait",
];

pub(super) fn ensure_expected_reply_artifact(
    workspace_dir: &Path,
    reply_path: &Path,
    output_tail: &str,
) -> Result<(), RunTaskError> {
    if !reply_artifact_present(reply_path) {
        return Err(RunTaskError::OutputMissing {
            path: reply_path.to_path_buf(),
            output: output_tail.to_string(),
        });
    }

    let Some(missing_labels) = investment_contract_missing_labels(workspace_dir, reply_path)?
    else {
        return Ok(());
    };

    Err(RunTaskError::OutputContractViolation {
        path: reply_path.to_path_buf(),
        reason: format!(
            "investment reply is missing required labels: {}",
            missing_labels.join(", ")
        ),
        output: output_tail.to_string(),
    })
}

pub(super) fn reply_artifact_ready_for_workspace(workspace_dir: &Path, reply_path: &Path) -> bool {
    ensure_expected_reply_artifact(workspace_dir, reply_path, "").is_ok()
}

fn investment_contract_missing_labels(
    workspace_dir: &Path,
    reply_path: &Path,
) -> Result<Option<Vec<String>>, RunTaskError> {
    let request_text = load_inbound_request_text(workspace_dir)?;
    if !is_investment_request(&request_text) {
        return Ok(None);
    }

    let reply_body = fs::read_to_string(reply_path)?;
    let normalized_reply = normalize_search_text(&reply_body);
    let mut missing = Vec::new();

    for (label, require_colon) in REQUIRED_INVESTMENT_LABELS {
        let present = if *require_colon {
            normalized_reply.contains(&format!("{}:", label.to_ascii_lowercase()))
        } else {
            normalized_reply.contains(&label.to_ascii_lowercase())
        };
        if !present {
            missing.push((*label).to_string());
        }
    }

    if missing.is_empty() {
        Ok(None)
    } else {
        Ok(Some(missing))
    }
}

fn load_inbound_request_text(workspace_dir: &Path) -> Result<String, RunTaskError> {
    let incoming_dir = workspace_dir.join("incoming_email");
    let mut parts = Vec::new();

    for name in ["thread_request.md", "email.txt", "email.html"] {
        let path = incoming_dir.join(name);
        if !path.exists() {
            continue;
        }
        parts.push(fs::read_to_string(path)?);
    }

    let payload_path = incoming_dir.join("postmark_payload.json");
    if payload_path.exists() {
        let payload = fs::read_to_string(&payload_path)?;
        parts.push(payload.clone());
        if let Ok(json) = serde_json::from_str::<Value>(&payload) {
            for key in ["Subject", "TextBody", "StrippedTextReply", "HtmlBody"] {
                if let Some(value) = json.get(key).and_then(Value::as_str) {
                    parts.push(value.to_string());
                }
            }
        }
    }

    Ok(parts.join("\n"))
}

fn reply_artifact_present(reply_path: &Path) -> bool {
    if !reply_path.is_file() {
        return false;
    }
    if reply_path.file_name().and_then(|value| value.to_str()) == Some(".notion_api_replied") {
        return true;
    }
    match fs::read_to_string(reply_path) {
        Ok(contents) => !contents.trim().is_empty(),
        Err(_) => fs::metadata(reply_path)
            .map(|meta| meta.len() > 0)
            .unwrap_or(false),
    }
}

fn is_investment_request(raw: &str) -> bool {
    let normalized = normalize_search_text(raw);
    let has_finance_context = FINANCE_CONTEXT_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_investment_intent = INVESTMENT_INTENT_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_probable_ticker = contains_probable_ticker(raw);

    (has_finance_context && has_investment_intent)
        || (has_probable_ticker
            && (has_investment_intent
                || normalized.contains("earnings")
                || normalized.contains("position")
                || normalized.contains("deep research")))
}

fn contains_probable_ticker(raw: &str) -> bool {
    const STOPWORDS: &[&str] = &[
        "A", "AI", "AM", "AND", "ARE", "BUY", "ETF", "EPS", "HTML", "I", "JSON", "NOW", "THE",
        "WAIT",
    ];

    raw.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '$')
        .filter(|token| !token.is_empty())
        .any(|token| {
            let trimmed = token.trim_start_matches('$');
            let len = trimmed.len();
            if !(1..=5).contains(&len) {
                return false;
            }
            if STOPWORDS.iter().any(|stop| stop == &trimmed) {
                return false;
            }
            let has_alpha = trimmed.chars().any(|ch| ch.is_ascii_alphabetic());
            has_alpha && trimmed.chars().all(|ch| ch.is_ascii_uppercase())
        })
}

fn normalize_search_text(raw: &str) -> String {
    let without_tags = rough_html_to_text(raw);
    let without_entities = without_tags
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    without_entities
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn rough_html_to_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;

    for ch in raw.chars() {
        match ch {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_expected_reply_artifact, is_investment_request, reply_artifact_ready_for_workspace,
    };
    use std::fs;
    use tempfile::tempdir;

    fn write_workspace(raw_request: &str, reply_body: &str) -> std::path::PathBuf {
        let temp = tempdir().expect("tempdir");
        let root = temp.keep();
        let incoming_dir = root.join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("incoming_email");
        fs::write(
            incoming_dir.join("postmark_payload.json"),
            serde_json::json!({
                "Subject": "NVIDIA stock",
                "TextBody": raw_request,
            })
            .to_string(),
        )
        .expect("payload");
        fs::write(root.join("reply_email_draft.html"), reply_body).expect("reply");
        root
    }

    #[test]
    fn investment_request_detection_handles_single_ticker_prompts() {
        assert!(is_investment_request(
            "Give me deep research on NVDA and tell me whether now is a good time to buy."
        ));
        assert!(is_investment_request(
            "Is NVDA a buy this week for a 3-month position?"
        ));
        assert!(!is_investment_request(
            "Please buy an NVDA GPU and compare keyboard options."
        ));
    }

    #[test]
    fn structured_investment_reply_passes_contract_validation() {
        let workspace = write_workspace(
            "Give me deep research on NVDA and tell me whether now is a good time to buy.",
            r#"
            <h2>Request Framing</h2>
            <ul><li><strong>Horizon:</strong> Long-term (inferred)</li></ul>
            <h2>Final Recommendation</h2>
            <ul>
              <li><strong>Rating:</strong> Wait</li>
              <li><strong>Horizon:</strong> Long-term (inferred)</li>
              <li><strong>Confidence:</strong> Medium</li>
              <li><strong>Timing Verdict:</strong> Wait</li>
              <li><strong>Add Criteria:</strong> Better valuation or cleaner post-earnings setup.</li>
              <li><strong>Invalidation Criteria:</strong> Demand slowdown or margin compression.</li>
              <li><strong>Biggest Near-Term Risk:</strong> Event volatility.</li>
              <li><strong>Biggest Long-Term Strength:</strong> AI platform leadership.</li>
            </ul>
            <h2>Verified Facts</h2><ul><li>Fact</li></ul>
            <h2>Derived Metrics</h2><ul><li>Metric: price / eps = 10x</li></ul>
            <h2>Inference / Judgment</h2><ul><li>Judgment</li></ul>
            <h2>Scenario Analysis</h2>
            <p><strong>Bull Case:</strong> Demand remains strong.</p>
            <p><strong>Base Case:</strong> Growth normalizes.</p>
            <p><strong>Bear Case:</strong> Spending slows.</p>
            "#,
        );
        let reply_path = workspace.join("reply_email_draft.html");

        ensure_expected_reply_artifact(&workspace, &reply_path, "").expect("valid contract");
        assert!(reply_artifact_ready_for_workspace(&workspace, &reply_path));
    }

    #[test]
    fn generic_investment_commentary_fails_contract_validation() {
        let workspace = write_workspace(
            "Give me deep research on NVDA and tell me whether now is a good time to buy.",
            "<p>NVIDIA is a good business, but I would wait until after earnings and buy in tranches.</p>",
        );
        let reply_path = workspace.join("reply_email_draft.html");

        let err = ensure_expected_reply_artifact(&workspace, &reply_path, "tail")
            .expect_err("expected contract violation");
        let rendered = err.to_string();
        assert!(
            rendered.contains("Output contract violation")
                || rendered.contains("missing required labels")
        );
        assert!(!reply_artifact_ready_for_workspace(&workspace, &reply_path));
    }
}
