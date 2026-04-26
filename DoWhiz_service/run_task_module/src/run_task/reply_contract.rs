use std::fs;
use std::path::Path;

use serde_json::Value;

use super::errors::RunTaskError;

const REQUIRED_INVESTMENT_MARKERS: &[&str] = &[
    "As of:",
    "Price:",
    "Investor question:",
    "Decision Card",
    "Audience",
    "Action",
    "Confidence",
    "New money",
    "Existing holder",
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
    "Judgment",
];

const SUMMARY_FIRST_HEADINGS: &[&str] = &[
    "decision card",
    "dual-horizon framing",
    "verified facts",
    "derived metrics",
    "scenarios",
    "triggers",
    "judgment",
];

const SECTION_HEADINGS: &[&str] = &[
    "decision card",
    "dual-horizon framing",
    "verified facts",
    "derived metrics",
    "scenarios",
    "triggers",
    "judgment",
];

const LINKED_EVIDENCE_SECTIONS: &[&str] = &["verified facts"];

const INVESTMENT_INSTRUMENT_KEYWORDS: &[&str] =
    &["stock", "etf", "ticker", "earnings", "position", "shares"];

const INVESTMENT_FUNDAMENTAL_KEYWORDS: &[&str] = &[
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
    "investment",
    "investing",
    "starter position",
    "starter only",
    "buy now",
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
    let mut violations = Vec::new();

    let missing_markers = missing_required_markers(&normalized_reply);
    if !missing_markers.is_empty() {
        violations.push(format!(
            "missing required labels: {}",
            missing_markers.join(", ")
        ));
    }

    if !contains_markers_in_order(&normalized_reply, SUMMARY_FIRST_HEADINGS) {
        violations.push(
            "summary-first section order must be Decision Card -> Dual-Horizon Framing -> Verified Facts -> Derived Metrics -> Scenarios -> Triggers -> Judgment"
                .to_string(),
        );
    }

    if !decision_card_has_required_rows(&normalized_reply) {
        violations.push(
            "decision card must show separate `New money` and `Existing holder` rows with action/confidence context"
                .to_string(),
        );
    }

    if !derived_metrics_has_formula(&reply_body, &lowered_reply) {
        violations.push(
            "derived metrics must include at least one formula-like expression or an explicit non-derivable note"
                .to_string(),
        );
    }

    if !triggers_section_has_threshold_markers(&lowered_reply) {
        violations.push(
            "triggers section must include upgrade/add plus trim or exit conditions".to_string(),
        );
    }

    let clickable_link_count = count_clickable_links(&lowered_reply);
    if clickable_link_count < 3 {
        violations.push(format!(
            "expected at least 3 clickable source links, found {}",
            clickable_link_count
        ));
    }

    for heading in LINKED_EVIDENCE_SECTIONS {
        if !section_contains_clickable_link(&lowered_reply, heading) {
            violations.push(format!(
                "section `{}` must include a clickable source link",
                heading
            ));
        }
    }

    if violations.is_empty() {
        Ok(None)
    } else {
        Ok(Some(violations))
    }
}

fn missing_required_markers(normalized_reply: &str) -> Vec<String> {
    let mut missing = Vec::new();

    for label in REQUIRED_INVESTMENT_MARKERS {
        if !normalized_reply.contains(&label.to_ascii_lowercase()) {
            missing.push((*label).to_string());
        }
    }

    missing
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

fn decision_card_has_required_rows(normalized_reply: &str) -> bool {
    [
        "audience",
        "action",
        "confidence",
        "new money",
        "existing holder",
    ]
    .iter()
    .all(|marker| normalized_reply.contains(marker))
}

fn derived_metrics_has_formula(raw_html: &str, lowered_reply: &str) -> bool {
    let Some(section) = extract_section(lowered_reply, "derived metrics") else {
        return false;
    };
    let section_text = rough_html_to_text(section).to_ascii_lowercase();
    section_text.contains('/')
        || section_text.contains('=')
        || section_text.contains("not reliably derivable")
        || section_text.contains("formula")
        || raw_html.to_ascii_lowercase().contains("<table")
}

fn triggers_section_has_threshold_markers(lowered_reply: &str) -> bool {
    let Some(section) = extract_section(lowered_reply, "triggers") else {
        return false;
    };
    let section_text = rough_html_to_text(section).to_ascii_lowercase();
    let has_upgrade = section_text.contains("upgrade to buy");
    let has_add = section_text.contains("add");
    let has_trim_or_exit = section_text.contains("trim/exit")
        || section_text.contains("trim")
        || section_text.contains("exit");

    has_upgrade && has_add && has_trim_or_exit
}

fn count_clickable_links(lowered_reply: &str) -> usize {
    lowered_reply.matches("href=\"http").count() + lowered_reply.matches("href='http").count()
}

fn section_contains_clickable_link(lowered_reply: &str, heading: &str) -> bool {
    let Some(section) = extract_section(lowered_reply, heading) else {
        return false;
    };
    section.contains("href=\"http") || section.contains("href='http")
}

fn extract_section<'a>(lowered_reply: &'a str, heading: &str) -> Option<&'a str> {
    let start = find_heading_position(lowered_reply, heading)?;
    let section_start = start + 1;
    let mut end = lowered_reply.len();

    for next_heading in SECTION_HEADINGS {
        if *next_heading == heading {
            continue;
        }
        if let Some(offset) = find_heading_position(&lowered_reply[section_start..], next_heading) {
            end = end.min(section_start + offset);
        }
    }

    Some(&lowered_reply[start..end])
}

fn find_heading_position(lowered_reply: &str, heading: &str) -> Option<usize> {
    for marker in [format!(">{}", heading), format!(">{} ", heading)] {
        if let Some(idx) = lowered_reply.find(&marker) {
            return Some(idx);
        }
    }
    None
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
    let has_instrument_context = INVESTMENT_INSTRUMENT_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_fundamental_context = INVESTMENT_FUNDAMENTAL_KEYWORDS
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
        || (has_probable_ticker
            && (has_instrument_context
                || has_investment_intent
                || has_investment_research
                || has_fundamental_context))
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
        assert!(is_investment_request(
            "Please analyze Tesla stock and tell me if it is worth buying now."
        ));
        assert!(!is_investment_request(
            "Please buy an NVDA GPU and compare keyboard options."
        ));
        assert!(!is_investment_request(
            "Use Ray Dalio's framework to analyze where we are in the cycle now, explain high valuations in parts of the equity market, and make a PowerPoint slide about potential bubbles in the AI industry."
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
                <tr><th>Audience</th><th>Action</th><th>Confidence</th></tr>
                <tr><td>New money</td><td>Wait</td><td>Medium</td></tr>
                <tr><td>Existing holder</td><td>Hold</td><td>Medium</td></tr>
              </table>
              <p><strong>One-line rationale:</strong> NVIDIA remains a high-quality business, but the near-term setup still asks new buyers to pay up ahead of another demanding print.</p>
            </section>
            <section>
              <h2>Dual-Horizon Framing</h2>
              <h3>Near-Term Timing View</h3>
              <p>The next earnings print is the dominant catalyst, so new money should wait for a cleaner post-print setup.</p>
              <h3>Long-Term Ownership View</h3>
              <p>Existing holders can keep owning the secular AI demand story as long as margin durability and customer spending remain intact.</p>
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
                <li><strong>Upgrade to Buy (new money):</strong> Revenue above $70B and gross margin above 74%.</li>
                <li><strong>Add (existing holder):</strong> Pullback of at least 12% without a fundamental reset.</li>
                <li><strong>Trim/Exit:</strong> Two consecutive quarters of margin pressure or a major customer capex reset.</li>
              </ul>
            </section>
            <section>
              <h2>Judgment</h2>
              <p><em>Inference, Medium confidence.</em> The business still looks strong, but the setup is more compelling for holders than for fresh capital right before the next catalyst.</p>
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
            "<p>NVIDIA is a good business, but I would wait until after earnings and buy in tranches.</p>",
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
