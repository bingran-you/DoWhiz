use std::fs;
use std::path::Path;

use serde_json::Value;

use super::errors::RunTaskError;

const FULL_REQUIRED_LABELS: &[&str] = &[
    "As of:",
    "Price:",
    "Investor question:",
    "Decision Card",
    "Monitor Status",
    "New Money Action",
    "Existing Holder Action",
    "Thesis Impact",
    "Signal Quality",
    "Confidence",
    "One-line rationale:",
    "Dual-Horizon Framing",
    "Near-Term Timing View",
    "Long-Term Ownership View",
    "Verified Facts",
    "Derived Metrics",
    "Scenarios",
    "Bull Case",
    "Base Case",
    "Bear Case",
    "Triggers",
    "Upgrade / Review Now",
    "Downgrade / De-risk",
    "Invalidation",
    "Judgment",
];

const SHORT_REQUIRED_LABELS: &[&str] = &[
    "As of:",
    "Price:",
    "Investor question:",
    "Decision Card",
    "Monitor Status",
    "New Money Action",
    "Existing Holder Action",
    "Thesis Impact",
    "Signal Quality",
    "Confidence",
    "One-line rationale:",
    "What Changed",
    "Evidence",
    "Triggers",
    "Upgrade / Review Now",
    "Downgrade / De-risk",
    "Invalidation",
    "Judgment",
];

const FULL_ORDER: &[&str] = &[
    "decision card",
    "dual-horizon framing",
    "verified facts",
    "derived metrics",
    "scenarios",
    "triggers",
    "judgment",
];

const SHORT_ORDER: &[&str] = &[
    "decision card",
    "what changed",
    "evidence",
    "triggers",
    "judgment",
];

const GENERIC_PHRASES: &[&str] = &[
    "good company, but do not chase",
    "great business, but wait",
    "hold for now",
    "buy in tranches",
    "wait for clarity",
    "do not chase",
    "not a broken asset",
];

const INVESTMENT_INSTRUMENT_KEYWORDS: &[&str] =
    &["stock", "etf", "ticker", "earnings", "position", "shares"];

const INVESTMENT_INTENT_KEYWORDS: &[&str] = &[
    "good time to buy",
    "should i buy",
    "should i sell",
    "worth buying",
    "a buy",
    "buy this week",
    "buy before",
    "sell before",
    "investment",
    "investing",
    "starter position",
    "starter only",
    "buy now",
    "add or trim",
];

const INVESTMENT_RESEARCH_KEYWORDS: &[&str] = &["deep research", "analyze", "analysis"];

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

    let Some(violations) = investment_contract_violations(workspace_dir, reply_path)? else {
        return Ok(());
    };

    Err(RunTaskError::OutputContractViolation {
        path: reply_path.to_path_buf(),
        reason: format!(
            "investment reply violates required contract: {}",
            violations.join("; ")
        ),
        output: output_tail.to_string(),
    })
}

pub(super) fn reply_artifact_ready_for_workspace(workspace_dir: &Path, reply_path: &Path) -> bool {
    ensure_expected_reply_artifact(workspace_dir, reply_path, "").is_ok()
}

fn investment_contract_violations(
    workspace_dir: &Path,
    reply_path: &Path,
) -> Result<Option<Vec<String>>, RunTaskError> {
    let request_text = load_inbound_request_text(workspace_dir)?;
    if !is_investment_request(&request_text) {
        return Ok(None);
    }

    let reply_body = fs::read_to_string(reply_path)?;
    let normalized_reply = normalize_search_text(&reply_body);
    let lowered_reply = reply_body.to_ascii_lowercase();
    let contract_type = detect_contract_type(&normalized_reply);
    let required_labels = if contract_type == "short" {
        SHORT_REQUIRED_LABELS
    } else {
        FULL_REQUIRED_LABELS
    };
    let required_order = if contract_type == "short" {
        SHORT_ORDER
    } else {
        FULL_ORDER
    };
    let mut violations = Vec::new();

    let missing_markers = missing_required_markers(&normalized_reply, required_labels);
    if !missing_markers.is_empty() {
        violations.push(format!(
            "missing required labels: {}",
            missing_markers.join(", ")
        ));
    }

    if !contains_markers_in_order(&normalized_reply, required_order) {
        violations.push("summary-first section order is wrong".to_string());
    }

    if !decision_card_has_required_fields(&normalized_reply) {
        violations.push(
            "decision card must include monitor status, both action fields, thesis impact, signal quality, and confidence"
                .to_string(),
        );
    }

    if contract_type == "full" && !derived_metrics_has_formula(&normalized_reply) {
        violations.push(
            "derived metrics must include a formula-like expression or an explicit non-derivable note"
                .to_string(),
        );
    }

    let clickable_link_count = count_clickable_links(&lowered_reply);
    let min_links = if contract_type == "short" { 2 } else { 3 };
    if clickable_link_count < min_links {
        violations.push(format!(
            "expected at least {} clickable source links, found {}",
            min_links, clickable_link_count
        ));
    }

    let trigger_section = extract_text_section(&normalized_reply, "triggers", &["judgment"]);
    if neutral_actions_present(&normalized_reply) {
        for label in [
            "upgrade / review now",
            "downgrade / de-risk",
            "invalidation",
        ] {
            if !trigger_section.contains(label) {
                violations.push(format!("neutral stance missing trigger label `{}`", label));
            }
        }
        if count_numeric_hits(&trigger_section) < 3 {
            violations
                .push("neutral stance lacks enough concrete numeric trigger detail".to_string());
        }
    }

    if contract_type == "short" && normalized_reply.len() > 2200 {
        violations.push("No Material Change artifact exceeds short-output budget".to_string());
    }

    if GENERIC_PHRASES
        .iter()
        .any(|phrase| normalized_reply.contains(phrase))
        && count_numeric_hits(&trigger_section) < 3
    {
        violations
            .push("generic hold/wait phrasing without concrete movement criteria".to_string());
    }

    if violations.is_empty() {
        Ok(None)
    } else {
        Ok(Some(violations))
    }
}

fn detect_contract_type(normalized_reply: &str) -> &'static str {
    if normalized_reply.contains("what changed")
        && normalized_reply.contains("evidence")
        && !normalized_reply.contains("dual-horizon framing")
    {
        "short"
    } else {
        "full"
    }
}

fn missing_required_markers(normalized_reply: &str, labels: &[&str]) -> Vec<String> {
    labels
        .iter()
        .filter(|label| !normalized_reply.contains(&label.to_ascii_lowercase()))
        .map(|label| (*label).to_string())
        .collect()
}

fn contains_markers_in_order(normalized_reply: &str, markers: &[&str]) -> bool {
    let mut search_start = 0;
    for marker in markers {
        let haystack = &normalized_reply[search_start..];
        let Some(found) = haystack.find(marker) else {
            return false;
        };
        search_start += found + marker.len();
    }
    true
}

fn decision_card_has_required_fields(normalized_reply: &str) -> bool {
    [
        "monitor status",
        "new money action",
        "existing holder action",
        "thesis impact",
        "signal quality",
        "confidence",
    ]
    .iter()
    .all(|marker| normalized_reply.contains(marker))
}

fn derived_metrics_has_formula(normalized_reply: &str) -> bool {
    let section = extract_text_section(normalized_reply, "derived metrics", &["scenarios"]);
    section.contains("formula / inputs")
        || section.contains('/')
        || section.contains('=')
        || section.contains("not reliably derivable")
}

fn neutral_actions_present(normalized_reply: &str) -> bool {
    normalized_reply.contains("new money action wait")
        || normalized_reply.contains("existing holder action hold")
        || normalized_reply.contains("existing holder action hold/do not add")
}

fn count_clickable_links(lowered_reply: &str) -> usize {
    lowered_reply.matches("href=\"http").count() + lowered_reply.matches("href='http").count()
}

fn extract_text_section(normalized_reply: &str, start_label: &str, end_labels: &[&str]) -> String {
    let Some(start) = normalized_reply.find(start_label) else {
        return String::new();
    };
    let tail = &normalized_reply[start..];
    let end = end_labels
        .iter()
        .filter_map(|label| tail.find(label))
        .min()
        .unwrap_or(tail.len());
    tail[..end].to_string()
}

fn count_numeric_hits(text: &str) -> usize {
    let mut count = 0;
    let mut in_number = false;
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            if !in_number {
                count += 1;
            }
            in_number = true;
        } else {
            in_number = false;
        }
    }
    count + text.matches('%').count() + text.matches('$').count() + text.matches("bps").count()
}

#[allow(dead_code)]
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
    let has_instrument_context = INVESTMENT_INSTRUMENT_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_investment_intent = INVESTMENT_INTENT_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_investment_research = INVESTMENT_RESEARCH_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_probable_ticker = contains_probable_ticker(raw);

    (has_instrument_context && (has_investment_intent || has_investment_research))
        || (has_probable_ticker && has_investment_intent)
}

fn contains_probable_ticker(raw: &str) -> bool {
    const STOPWORDS: &[&str] = &[
        "A", "AI", "ACI", "AM", "AND", "API", "ARE", "BUY", "CI", "ETF", "EPS", "HTML", "I",
        "JSON", "NOW", "PR", "THE", "UI", "URL", "UX", "WAIT",
    ];

    raw.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '$')
        .filter(|token| !token.is_empty())
        .any(|token| {
            let trimmed = token.trim_start_matches('$');
            let len = trimmed.len();
            if !(2..=5).contains(&len) {
                return false;
            }
            if STOPWORDS.iter().any(|stop| stop == &trimmed) {
                return false;
            }
            let has_alpha = trimmed.chars().any(|ch| ch.is_ascii_alphabetic());
            has_alpha && trimmed.chars().all(|ch| ch.is_ascii_uppercase())
        })
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
        assert!(is_investment_request(
            "Please analyze Tesla stock and tell me if it is worth buying now."
        ));
        assert!(!is_investment_request(
            "Please buy an NVDA GPU and compare keyboard options."
        ));
        assert!(!is_investment_request(
            "Analyze PR comments on our API design and summarize the tradeoffs."
        ));
    }

    #[test]
    fn structured_investment_reply_passes_contract_validation() {
        let workspace = write_workspace(
            "Give me deep research on NVDA and tell me whether now is a good time to buy.",
            r#"
            <section>
              <p><strong>As of:</strong> 2026-04-26 · <strong>Price:</strong> $202.06</p>
              <p><strong>Investor question:</strong> Give me deep research on NVDA and tell me whether now is a good time to buy.</p>
            </section>
            <section>
              <h2>Decision Card</h2>
              <table>
                <tr><th>Field</th><th>Value</th></tr>
                <tr><td>Monitor Status</td><td>Watch Closely</td></tr>
                <tr><td>New Money Action</td><td>Starter Only</td></tr>
                <tr><td>Existing Holder Action</td><td>Hold/Do not add</td></tr>
                <tr><td>Thesis Impact</td><td>Mixed</td></tr>
                <tr><td>Signal Quality</td><td>Moderate</td></tr>
                <tr><td>Confidence</td><td>Medium</td></tr>
              </table>
              <p><strong>One-line rationale:</strong> NVIDIA still looks strong, but the next print carries enough margin risk that fresh capital should stay sized and conditional.</p>
            </section>
            <section>
              <h2>Dual-Horizon Framing</h2>
              <h3>Near-Term Timing View</h3>
              <p>The next earnings print is the dominant catalyst for new money.</p>
              <h3>Long-Term Ownership View</h3>
              <p>Existing holders can stay with the AI demand story while margin durability remains intact.</p>
            </section>
            <section>
              <h2>Verified Facts</h2>
              <ul>
                <li>FY2026 revenue reached $215.9B. <a href="https://investor.nvidia.com/">NVIDIA IR</a></li>
                <li>Q4 FY2026 revenue was $68.1B with GAAP diluted EPS of $1.76. <a href="https://www.sec.gov/">SEC EDGAR</a></li>
                <li>The stock closed at $202.06 on April 20, 2026. <a href="https://www.nasdaq.com/">Nasdaq</a></li>
              </ul>
            </section>
            <section>
              <h2>Derived Metrics</h2>
              <table>
                <tr><th>Metric</th><th>Value</th><th>Formula / Inputs</th></tr>
                <tr><td>P/E (TTM)</td><td>41.2x</td><td>$202.06 / TTM diluted EPS $4.90</td></tr>
                <tr><td>Revenue YoY (Q4)</td><td>21.0%</td><td>$68.1B / $56.3B - 1</td></tr>
              </table>
            </section>
            <section>
              <h2>Scenarios</h2>
              <h3>Bull Case</h3>
              <p>Revenue stays above $70B and gross margin holds above 74%.</p>
              <h3>Base Case</h3>
              <p>Revenue remains near the current run-rate and valuation stays elevated but stable.</p>
              <h3>Bear Case</h3>
              <p>Customer digestion pushes revenue below $64B or gross margin slips under 71%.</p>
            </section>
            <section>
              <h2>Triggers — Verdict Movement</h2>
              <ul>
                <li><strong>Upgrade / Review Now:</strong> Revenue above $70B and gross margin above 74%.</li>
                <li><strong>Downgrade / De-risk:</strong> Gross margin below 71% or a guide cut of 5% or more.</li>
                <li><strong>Invalidation:</strong> A major customer capex reset or export-control shock that threatens more than 10% of revenue.</li>
              </ul>
            </section>
            <section>
              <h2>Judgment</h2>
              <p><em>Inference, Medium confidence.</em> The business still looks strong, but the right calibrated output is `Watch Closely`, not a generic hold-and-wait paragraph.</p>
            </section>
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
            "<p>NVIDIA is a good company, but I would wait for clarity and buy in tranches.</p>",
        );
        let reply_path = workspace.join("reply_email_draft.html");

        let err = ensure_expected_reply_artifact(&workspace, &reply_path, "tail")
            .expect_err("expected contract violation");
        let rendered = err.to_string();
        assert!(
            rendered.contains("Output contract violation")
                || rendered.contains("violates required contract")
        );
        assert!(!reply_artifact_ready_for_workspace(&workspace, &reply_path));
    }
}
