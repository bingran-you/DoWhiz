use std::fs;
use std::path::Path;

use serde_json::Value;

use super::errors::RunTaskError;

const REQUIRED_INVESTMENT_MARKERS: &[(&str, bool)] = &[
    ("Request Framing", false),
    ("Ticker", true),
    ("Name", true),
    ("Type", true),
    ("Research Mode", true),
    ("User Objective", true),
    ("Horizon Basis", true),
    ("Question Type", true),
    ("Decision Card", false),
    ("New Money Action", true),
    ("Existing Holder Action", true),
    ("Near-Term Timing View", true),
    ("Long-Term Ownership View", true),
    ("Confidence", true),
    ("One-Line Rationale", true),
    ("Why in 3 bullets", false),
    ("What Is Priced In", true),
    ("What Keeps This From Being Stronger", true),
    ("What Would Change The View", true),
    ("Trigger Block", false),
    ("Upgrade / Add Triggers", true),
    ("Stay Wait Unless", true),
    ("Invalidation Criteria", true),
    ("Verified Facts", false),
    ("Derived Metrics", false),
    ("Expectations", false),
    ("What the Next Catalyst Must Show", true),
    ("What Could Disappoint Even If Fundamentals Are Fine", true),
    ("Opportunity-Cost / Peer Check", false),
    ("Inference / Judgment", false),
    ("Bull Case", true),
    ("Base Case", true),
    ("Bear Case", true),
    ("Source Notes", false),
    ("Disclaimer", false),
];

const SUMMARY_FIRST_HEADINGS: &[&str] = &[
    "request framing",
    "decision card",
    "why in 3 bullets",
    "trigger block",
    "verified facts",
];

const SECTION_HEADINGS: &[&str] = &[
    "request framing",
    "decision card",
    "why in 3 bullets",
    "trigger block",
    "verified facts",
    "derived metrics",
    "expectations",
    "opportunity-cost / peer check",
    "inference / judgment",
    "scenario analysis",
    "source notes",
    "disclaimer",
];

const LINKED_EVIDENCE_SECTIONS: &[&str] = &[
    "verified facts",
    "derived metrics",
    "expectations",
    "source notes",
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

const NEW_MONEY_ACTIONS: &[&str] = &["buy", "wait", "starter only", "avoid for now"];
const EXISTING_HOLDER_ACTIONS: &[&str] = &["hold", "add", "trim", "exit", "hold / do not add"];

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
            "summary-first section order must be Request Framing -> Decision Card -> Why in 3 bullets -> Trigger Block -> Verified Facts"
                .to_string(),
        );
    }

    if let Some(issue) = validate_action_field(&reply_body, "New Money Action", NEW_MONEY_ACTIONS) {
        violations.push(issue);
    }
    if let Some(issue) = validate_action_field(
        &reply_body,
        "Existing Holder Action",
        EXISTING_HOLDER_ACTIONS,
    ) {
        violations.push(issue);
    }

    let evidence_chip_count = lowered_reply.matches("dw-evidence-chip").count();
    if evidence_chip_count < 4 {
        violations.push(format!(
            "expected at least 4 evidence chips, found {}",
            evidence_chip_count
        ));
    }

    for tier in ["primary", "independent", "reference"] {
        if !contains_source_tier(&lowered_reply, tier) {
            violations.push(format!("missing source tier evidence chip: {}", tier));
        }
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

    for (label, require_colon) in REQUIRED_INVESTMENT_MARKERS {
        let present = if *require_colon {
            normalized_reply.contains(&format!("{}:", label.to_ascii_lowercase()))
        } else {
            normalized_reply.contains(&label.to_ascii_lowercase())
        };
        if !present {
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

fn validate_action_field(raw_html: &str, label: &str, allowed_values: &[&str]) -> Option<String> {
    let value = extract_labeled_value(raw_html, label)?;
    let value = normalize_action_value(&value);

    if allowed_values
        .iter()
        .any(|candidate| normalize_action_value(candidate) == value)
    {
        return None;
    }

    Some(format!(
        "`{}` must be one of: {}",
        label,
        allowed_values.join(", ")
    ))
}

fn extract_labeled_value(raw_html: &str, label: &str) -> Option<String> {
    let lower = raw_html.to_ascii_lowercase();
    let marker = format!("{}:", label.to_ascii_lowercase());
    let start = lower.find(&marker)?;
    let tail = &raw_html[start..];
    let lower_tail = &lower[start..];
    let mut end = tail.len();
    for boundary in ["</li", "</p", "</div", "<br", "\n"] {
        if let Some(idx) = lower_tail.find(boundary) {
            end = end.min(idx);
        }
    }
    let fragment = rough_html_to_text(&tail[..end]);
    let lowered_fragment = fragment.to_ascii_lowercase();
    let marker_index = lowered_fragment.find(&marker)?;
    let value = fragment[marker_index + marker.len()..].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn normalize_action_value(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn contains_source_tier(lowered_reply: &str, tier: &str) -> bool {
    lowered_reply.contains(&format!("data-source-tier=\"{}\"", tier))
        || lowered_reply.contains(&format!("data-source-tier='{}'", tier))
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
    for marker in [format!(">{}</", heading), format!(">{}<", heading)] {
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
            <section><h2>Request Framing</h2><ul>
              <li><strong>Ticker:</strong> NVDA</li>
              <li><strong>Name:</strong> NVIDIA</li>
              <li><strong>Type:</strong> Stock</li>
              <li><strong>Research Mode:</strong> Deep research</li>
              <li><strong>User Objective:</strong> Decide whether now is actionable (stated)</li>
              <li><strong>Horizon Basis:</strong> Dual-horizon default because the user did not specify one (inferred)</li>
              <li><strong>Question Type:</strong> Long-term accumulation</li>
            </ul></section>
            <section class="dw-investment-card"><h2>Decision Card</h2><ul>
              <li><strong>New Money Action:</strong> Starter Only</li>
              <li><strong>Existing Holder Action:</strong> Hold / Do not add</li>
              <li><strong>Near-Term Timing View:</strong> Wait for a cleaner post-earnings setup.</li>
              <li><strong>Long-Term Ownership View:</strong> Attractive if AI demand durability remains intact.</li>
              <li><strong>Confidence:</strong> Medium</li>
              <li><strong>One-Line Rationale:</strong> Quality is high, but expectations and valuation leave a thin near-term margin for error.</li>
            </ul></section>
            <section><h2>Why in 3 bullets</h2><ul>
              <li><strong>What Is Priced In:</strong> Sustained AI spending and another strong quarter.</li>
              <li><strong>What Keeps This From Being Stronger:</strong> Valuation already assumes very little execution slippage.</li>
              <li><strong>What Would Change The View:</strong> Better evidence that demand durability is outrunning already-high expectations.</li>
            </ul></section>
            <section><h2>Trigger Block</h2><ul>
              <li><strong>Upgrade / Add Triggers:</strong> Strong beat plus durable margin guidance.</li>
              <li><strong>Stay Wait Unless:</strong> Setup de-risks after earnings or valuation resets.</li>
              <li><strong>Invalidation Criteria:</strong> Demand or gross margin thesis weakens materially.</li>
            </ul></section>
            <section><h2>Verified Facts</h2><ul><li>Revenue grew. <div class="dw-evidence-row"><a class="dw-evidence-chip" data-source-tier="primary" href="https://investor.nvidia.com">IR</a><a class="dw-evidence-chip" data-source-tier="independent" href="https://www.reuters.com">Reuters</a></div></li></ul></section>
            <section><h2>Derived Metrics</h2><ul><li>Forward P/E: price / forward EPS = 31x. <div class="dw-evidence-row"><a class="dw-evidence-chip" data-source-tier="primary" href="https://www.sec.gov">Filing</a><a class="dw-evidence-chip" data-source-tier="reference" href="https://finance.yahoo.com">Quote</a></div></li></ul></section>
            <section><h2>Expectations</h2><ul>
              <li><strong>What the Next Catalyst Must Show:</strong> Sustained data-center demand plus margin resilience. <div class="dw-evidence-row"><a class="dw-evidence-chip" data-source-tier="independent" href="https://www.reuters.com">Reuters</a></div></li>
              <li><strong>What Could Disappoint Even If Fundamentals Are Fine:</strong> Guidance that is merely good rather than exceptional.</li>
            </ul></section>
            <section><h2>Opportunity-Cost / Peer Check</h2><ul><li>NVIDIA still has the strongest AI platform position, but buying the index avoids single-report valuation compression. <div class="dw-evidence-row"><a class="dw-evidence-chip" data-source-tier="reference" href="https://www.nasdaq.com">Quote</a></div></li></ul></section>
            <section><h2>Inference / Judgment</h2><ul><li>Judgment.</li></ul></section>
            <section><h2>Scenario Analysis</h2>
              <p><strong>Bull Case:</strong> Demand remains strong.</p>
              <p><strong>Base Case:</strong> Growth normalizes.</p>
              <p><strong>Bear Case:</strong> Spending slows.</p>
            </section>
            <section><h2>Source Notes</h2><ul><li>Primary, independent, and reference sources were cross-checked. <div class="dw-evidence-row"><a class="dw-evidence-chip" data-source-tier="primary" href="https://investor.nvidia.com">IR</a><a class="dw-evidence-chip" data-source-tier="independent" href="https://www.reuters.com">Reuters</a><a class="dw-evidence-chip" data-source-tier="reference" href="https://finance.yahoo.com">Quote</a></div></li></ul></section>
            <section><h2>Disclaimer</h2><p>Public-information-based research only, not personalized investment advice or trade execution.</p></section>
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
