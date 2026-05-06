use std::fs;
use std::path::Path;

use kuchiki::traits::*;
use kuchiki::NodeRef;
use serde_json::Value;

use super::errors::RunTaskError;

#[allow(dead_code)]
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

#[allow(dead_code)]
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
    "Why Now",
    "What Would Change The View",
    "Evidence Chips",
    "Upgrade / Review Now",
    "Downgrade / De-risk",
    "Invalidation",
];

#[allow(dead_code)]
const FULL_ORDER: &[&str] = &[
    "decision card",
    "dual-horizon framing",
    "verified facts",
    "derived metrics",
    "scenarios",
    "triggers",
    "judgment",
];

#[allow(dead_code)]
const SHORT_ORDER: &[&str] = &[
    "decision card",
    "why now",
    "what would change the view",
    "evidence chips",
];

#[allow(dead_code)]
const GENERIC_PHRASES: &[&str] = &[
    "good company, but do not chase",
    "great business, but wait",
    "hold for now",
    "buy in tranches",
    "wait for clarity",
    "do not chase",
    "not a broken asset",
    "good business, but not a good all-in buy",
    "wait until after earnings",
];
#[allow(dead_code)]
const INCOMPLETE_FAIL_SOFT_MARKERS: &[&str] = &[
    "incomplete research artifact",
    "research did not complete within budget",
    "best-available timed artifact",
    "best-available timed reply",
    "incomplete monitor check",
];
#[allow(dead_code)]
const INCOMPLETE_RESEARCH_MARKERS: &[&str] = &[
    "incomplete research artifact",
    "research did not complete within budget",
    "best-available timed artifact",
    "best-available timed reply",
];
#[allow(dead_code)]
const INTERNAL_THREAD_METADATA_MARKERS: &[&str] = &[
    "canonical thread request",
    "auto-generated merged view for reruns",
    "thread timeline",
    "merged attachments for this rerun",
];
#[allow(dead_code)]
const INTERNAL_RUNTIME_LANGUAGE_MARKERS: &[&str] = &[
    "verification incomplete in this timed monitor run",
    "verification incomplete in this timed run",
    "timed monitor fallback",
    "short fail-soft update",
    "fail-soft artifact",
    "best-available timed artifact",
    "best-available timed reply",
    "contract-ready final artifact",
    "the runtime returned",
    "silently losing the task",
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
    "starter position",
    "buy now",
    "add or trim",
    "trim or exit",
    "avoid for now",
];

const INVESTMENT_RESEARCH_KEYWORDS: &[&str] = &["deep research"];
const INVESTMENT_MONITOR_KEYWORDS: &[&str] = &[
    "material changed",
    "material change",
    "what changed",
    "since your last note",
    "should i act",
    "only tell me if i should act",
    "do i add",
    "do i trim",
    "add or trim",
    "is there any edge",
    "review now",
    "investment monitor",
    "monitor output",
];
const SYNTHETIC_SCENARIO_KEYWORDS: &[&str] = &[
    "assume ",
    "assumption-based",
    "synthetic",
    "company x",
    "company y",
];
const ACTION_ONLY_MONITOR_KEYWORDS: &[&str] = &[
    "only tell me if i should act",
    "just tell me if i should act",
];
#[allow(dead_code)]
const SHORT_ARTIFACT_VISIBLE_CHAR_LIMIT: usize = 1200;
const MAX_REPLY_ARTIFACT_BYTES: usize = 64 * 1024;
const DOWHIZ_EMAIL_CONTENT_START: &str = "<!-- dowhiz-email-content:start -->";
const DOWHIZ_EMAIL_CONTENT_END: &str = "<!-- dowhiz-email-content:end -->";
const EMAIL_SHELL_BOILERPLATE: &[&str] = &[
    "DoWhiz digital employee",
    "Reply directly to continue this thread with DoWhiz.",
    "Sent by DoWhiz. If you reply, the same task thread will continue.",
];
const BLOCK_HTML_TAGS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "br",
    "dd",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
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

pub(super) fn investment_request_for_workspace(workspace_dir: &Path) -> Result<bool, RunTaskError> {
    Ok(is_investment_request(&load_inbound_request_text(
        workspace_dir,
    )?))
}

pub(super) fn investment_monitor_request_for_workspace(
    workspace_dir: &Path,
) -> Result<bool, RunTaskError> {
    Ok(is_investment_monitor_request(&load_inbound_request_text(
        workspace_dir,
    )?))
}

pub(super) fn synthetic_investment_request_for_workspace(
    workspace_dir: &Path,
) -> Result<bool, RunTaskError> {
    Ok(is_synthetic_investment_request(&load_inbound_request_text(
        workspace_dir,
    )?))
}

pub(super) fn action_only_monitor_request_for_workspace(
    workspace_dir: &Path,
) -> Result<bool, RunTaskError> {
    Ok(is_action_only_monitor_request(&load_inbound_request_text(
        workspace_dir,
    )?))
}

fn investment_contract_violations(
    _workspace_dir: &Path,
    reply_path: &Path,
) -> Result<Option<Vec<String>>, RunTaskError> {
    let reply_body = fs::read_to_string(reply_path)?;
    let mut violations = Vec::new();

    if reply_body.len() > MAX_REPLY_ARTIFACT_BYTES {
        violations.push("reply artifact exceeds bounded monitor size limit".to_string());
    }

    if reply_body.contains('\0') {
        violations.push("reply artifact contains null bytes".to_string());
    }

    let visible_text = if looks_like_html_reply(reply_path, &reply_body) {
        rough_html_to_text(&reply_body)
    } else {
        reply_body.clone()
    };
    if visible_text.trim().is_empty() {
        violations.push("reply artifact has no visible text".to_string());
    }

    if violations.is_empty() {
        Ok(None)
    } else {
        Ok(Some(violations))
    }
}

fn looks_like_html_reply(reply_path: &Path, body: &str) -> bool {
    matches!(
        reply_path.extension().and_then(|value| value.to_str()),
        Some("html") | Some("htm")
    ) || body.contains('<')
}

#[allow(dead_code)]
fn detect_contract_type(normalized_reply: &str) -> &'static str {
    if normalized_reply.contains("why now")
        && normalized_reply.contains("what would change the view")
        && normalized_reply.contains("evidence chips")
        && !normalized_reply.contains("dual-horizon framing")
    {
        "short"
    } else {
        "full"
    }
}

#[allow(dead_code)]
fn is_incomplete_fail_soft_artifact(normalized_reply: &str) -> bool {
    INCOMPLETE_FAIL_SOFT_MARKERS
        .iter()
        .any(|marker| normalized_reply.contains(marker))
}

#[allow(dead_code)]
fn is_incomplete_research_artifact(normalized_reply: &str) -> bool {
    INCOMPLETE_RESEARCH_MARKERS
        .iter()
        .any(|marker| normalized_reply.contains(marker))
}

#[allow(dead_code)]
fn missing_required_markers(normalized_reply: &str, labels: &[&str]) -> Vec<String> {
    labels
        .iter()
        .filter(|label| !normalized_reply.contains(&label.to_ascii_lowercase()))
        .map(|label| (*label).to_string())
        .collect()
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn derived_metrics_has_formula(normalized_reply: &str) -> bool {
    let section = extract_text_section(normalized_reply, "derived metrics", &["scenarios"]);
    section.contains("formula / inputs")
        || section.contains('/')
        || section.contains('=')
        || section.contains("not reliably derivable")
}

#[allow(dead_code)]
fn neutral_actions_present(normalized_reply: &str) -> bool {
    normalized_reply.contains("new money action wait")
        || normalized_reply.contains("existing holder action hold")
        || normalized_reply.contains("existing holder action hold/do not add")
}

#[allow(dead_code)]
fn clickable_links(raw_reply: &str) -> Vec<String> {
    let fragment = visible_html_fragment(raw_reply);
    let document = kuchiki::parse_html().one(fragment);
    let Ok(nodes) = document.select("a[href]") else {
        return Vec::new();
    };

    nodes
        .filter_map(|node| {
            let attrs = node.attributes.borrow();
            let href = attrs.get("href")?;
            href.to_ascii_lowercase()
                .starts_with("http")
                .then(|| href.to_string())
        })
        .collect()
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
    count
        + text.matches('%').count()
        + text.matches('$').count()
        + text.matches("bps").count()
        + count_number_word_time_hits(text)
}

#[allow(dead_code)]
fn count_number_word_time_hits(text: &str) -> usize {
    let number_words = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
        "twelve",
    ];
    let time_units = [
        "day", "days", "week", "weeks", "month", "months", "quarter", "quarters", "year", "years",
    ];
    let tokens = text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    let mut count = 0;
    for window in tokens.windows(3) {
        let first = window[0];
        let second = window[1];
        let third = window[2];
        if number_words.contains(&first)
            && second.eq_ignore_ascii_case("more")
            && time_units.contains(&third)
        {
            count += 1;
        }
    }
    for window in tokens.windows(2) {
        let first = window[0];
        let second = window[1];
        if number_words.contains(&first) && time_units.contains(&second) {
            count += 1;
        }
    }
    count
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
    let has_monitor_intent = INVESTMENT_MONITOR_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_probable_ticker = contains_probable_ticker(raw);
    let has_explicit_monitor_contract = normalized.contains("investment monitor");
    let has_synthetic_monitor_contract =
        is_synthetic_investment_request(raw) && has_explicit_monitor_contract;

    (has_instrument_context
        && (has_investment_intent || has_investment_research || has_monitor_intent))
        || (has_probable_ticker && (has_investment_intent || has_monitor_intent))
        || has_synthetic_monitor_contract
}

fn is_investment_monitor_request(raw: &str) -> bool {
    let normalized = normalize_search_text(raw);
    let has_monitor_intent = INVESTMENT_MONITOR_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_probable_ticker = contains_probable_ticker(raw);
    let has_instrument_context = INVESTMENT_INSTRUMENT_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_explicit_monitor_contract = normalized.contains("investment monitor");

    (has_monitor_intent && (has_probable_ticker || has_instrument_context))
        || (is_synthetic_investment_request(raw) && has_explicit_monitor_contract)
}

fn is_synthetic_investment_request(raw: &str) -> bool {
    let normalized = normalize_search_text(raw);
    SYNTHETIC_SCENARIO_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword))
}

fn is_action_only_monitor_request(raw: &str) -> bool {
    let normalized = normalize_search_text(raw);
    is_investment_monitor_request(raw)
        && ACTION_ONLY_MONITOR_KEYWORDS
            .iter()
            .any(|keyword| normalized.contains(keyword))
}

fn contains_probable_ticker(raw: &str) -> bool {
    const STOPWORDS: &[&str] = &[
        "A", "AI", "ACI", "AM", "AND", "API", "ARE", "AWS", "BUY", "CI", "CLI", "CSS", "CSV",
        "ETF", "EPS", "HTML", "HTTP", "I", "JSON", "NOW", "PR", "SDK", "SQL", "SSH", "THE", "TSV",
        "UI", "URL", "UX", "WAIT", "XML", "YAML",
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
    let fragment = visible_html_fragment(raw);
    let document = kuchiki::parse_html().one(fragment);
    prune_invisible_nodes(&document);
    let mut text = String::new();
    collect_visible_text(&document, &mut text);
    for boilerplate in EMAIL_SHELL_BOILERPLATE {
        text = text.replace(boilerplate, " ");
    }
    text
}

fn collect_visible_text(node: &NodeRef, out: &mut String) {
    if let Some(text_node) = node.as_text() {
        out.push_str(&text_node.borrow());
        return;
    }

    let is_block = node
        .as_element()
        .map(|element| BLOCK_HTML_TAGS.contains(&element.name.local.as_ref()))
        .unwrap_or(false);
    if is_block {
        push_text_boundary(out);
    }

    for child in node.children() {
        collect_visible_text(&child, out);
    }

    if is_block {
        push_text_boundary(out);
    }
}

fn push_text_boundary(out: &mut String) {
    if out.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
        out.push(' ');
    }
}

fn visible_html_fragment(raw: &str) -> String {
    let Some(start) = raw.find(DOWHIZ_EMAIL_CONTENT_START) else {
        return raw.to_string();
    };
    let start = start + DOWHIZ_EMAIL_CONTENT_START.len();
    let tail = &raw[start..];
    let Some(end) = tail.find(DOWHIZ_EMAIL_CONTENT_END) else {
        return raw.to_string();
    };
    tail[..end].to_string()
}

fn prune_invisible_nodes(document: &NodeRef) {
    for selector in [
        "style",
        "script",
        "noscript",
        "template",
        "[hidden]",
        ".dw-preheader",
    ] {
        detach_selector_matches(document, selector);
    }
    detach_selector_matches(document, r#"[aria-hidden="true"]"#);

    let Ok(nodes) = document.select("[style]") else {
        return;
    };
    let hidden_nodes = nodes
        .filter_map(|node| {
            let attrs = node.attributes.borrow();
            let style = attrs.get("style")?;
            style_hides_content(style).then(|| node.as_node().clone())
        })
        .collect::<Vec<_>>();
    for node in hidden_nodes {
        node.detach();
    }
}

fn detach_selector_matches(document: &NodeRef, selector: &str) {
    let Ok(nodes) = document.select(selector) else {
        return;
    };
    let matches = nodes.map(|node| node.as_node().clone()).collect::<Vec<_>>();
    for node in matches {
        node.detach();
    }
}

fn style_hides_content(style: &str) -> bool {
    let normalized = style
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase();
    normalized.contains("display:none")
        || normalized.contains("visibility:hidden")
        || normalized.contains("mso-hide:all")
}

#[allow(dead_code)]
fn is_generic_placeholder_link(link: &str) -> bool {
    let lower = link.to_ascii_lowercase();
    let path = url_path(&lower);
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return true;
    }

    let generic_domains = [
        "reuters.com",
        "bloomberg.com",
        "nasdaq.com",
        "finance.yahoo.com",
        "marketwatch.com",
        "wsj.com",
        "ft.com",
        "seekingalpha.com",
        "sec.gov",
    ];
    let generic_paths = [
        "markets",
        "investing",
        "quote",
        "quotes",
        "stocks",
        "news",
        "finance",
        "research",
    ];

    generic_domains
        .iter()
        .any(|domain| lower.contains(domain) && generic_paths.contains(&trimmed))
}

#[allow(dead_code)]
fn link_looks_specific(link: &str) -> bool {
    let lower = link.to_ascii_lowercase();
    let path = url_path(&lower);
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .count()
        >= 2
        || trimmed.ends_with(".html")
        || trimmed.contains("filing")
        || trimmed.contains("earnings")
        || trimmed.contains("article")
}

#[allow(dead_code)]
fn contains_any_marker(normalized_reply: &str, markers: &[&str]) -> bool {
    markers
        .iter()
        .any(|marker| normalized_reply.contains(marker))
}

#[allow(dead_code)]
fn reply_looks_like_templated_incomplete_investment_output(normalized_reply: &str) -> bool {
    let has_decision_card = normalized_reply.contains("decision card");
    let has_action_table = normalized_reply.contains("new money action")
        || normalized_reply.contains("existing holder action");
    let has_incomplete_marker = contains_any_marker(normalized_reply, INCOMPLETE_FAIL_SOFT_MARKERS)
        || contains_any_marker(normalized_reply, INTERNAL_RUNTIME_LANGUAGE_MARKERS);

    (has_decision_card || has_action_table) && has_incomplete_marker
}

#[allow(dead_code)]
fn url_path(link: &str) -> &str {
    let without_scheme = link.split_once("://").map(|(_, rest)| rest).unwrap_or(link);
    let Some(path_start) = without_scheme.find('/') else {
        return "/";
    };
    let path = &without_scheme[path_start..];
    let path_end = path.find(['?', '#']).unwrap_or(path.len());
    &path[..path_end]
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_expected_reply_artifact, is_action_only_monitor_request,
        is_investment_monitor_request, is_investment_request, reply_artifact_ready_for_workspace,
    };
    use send_emails_module::normalize_email_html;
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
        assert!(is_investment_request(
            "Assume company Z reported revenue slightly above expectations, but lowered next-quarter margin guidance because of temporary supply-chain costs. Demand commentary improved, but free cash flow remained negative. Write the investment monitor output."
        ));
        assert!(is_investment_monitor_request(
            "Assume company Z reported revenue slightly above expectations, but lowered next-quarter margin guidance because of temporary supply-chain costs. Demand commentary improved, but free cash flow remained negative. Write the investment monitor output."
        ));
        assert!(is_action_only_monitor_request(
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act."
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
    fn generic_investment_commentary_passes_mechanical_validation() {
        let workspace = write_workspace(
            "Give me deep research on NVDA and tell me whether now is a good time to buy.",
            "<p>NVIDIA is a good company, but I would wait for clarity and buy in tranches.</p>",
        );
        let reply_path = workspace.join("reply_email_draft.html");

        ensure_expected_reply_artifact(&workspace, &reply_path, "tail")
            .expect("mechanical validation should not reject generic commentary");
        assert!(reply_artifact_ready_for_workspace(&workspace, &reply_path));
    }

    #[test]
    fn canonical_thread_metadata_is_not_blocked_by_mechanical_gate() {
        let workspace = write_workspace(
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.",
            r#"
            <section>
              <p><strong>As of:</strong> 2026-05-01</p>
              <p><strong>Investor question:</strong> # Canonical thread request</p>
            </section>
            "#,
        );
        let reply_path = workspace.join("reply_email_draft.html");

        ensure_expected_reply_artifact(&workspace, &reply_path, "")
            .expect("mechanical validation should not reject based on semantics");
    }

    #[test]
    fn trigger_numeric_hits_accept_spelled_out_time_windows() {
        let text = "upgrade / review now if margin improves by 150 basis points within one quarter and invalidation applies if free cash flow stays negative for three more quarters";
        assert!(super::count_numeric_hits(text) >= 3);
    }

    #[test]
    fn wrapped_no_material_change_artifact_ignores_email_shell_css() {
        let raw_reply = r#"
        <section>
          <p><strong>As of:</strong> 2026-05-01 · <strong>Price:</strong> $120.00</p>
          <p><strong>Investor question:</strong> Check whether anything material changed for NVDA since your last note. Only tell me if I should act.</p>
        </section>
        <section>
          <h2>Decision Card</h2>
          <table>
            <tr><td>Monitor Status</td><td>No Material Change</td></tr>
            <tr><td>New Money Action</td><td>Wait</td></tr>
            <tr><td>Existing Holder Action</td><td>Hold/Do not add</td></tr>
            <tr><td>Thesis Impact</td><td>No Material Change</td></tr>
            <tr><td>Signal Quality</td><td>Moderate</td></tr>
            <tr><td>Confidence</td><td>Medium</td></tr>
          </table>
          <p><strong>One-line rationale:</strong> Nothing material changed, so there is still no new edge today.</p>
        </section>
        <section>
          <h2>Why Now</h2>
          <p>No new filing, guide change, or disclosed demand break changes the call today.</p>
        </section>
        <section>
          <h2>What Would Change The View</h2>
          <ul>
            <li><strong>Upgrade / Review Now:</strong> Revenue guide rises by more than 5% or gross margin expands above 75%.</li>
            <li><strong>Downgrade / De-risk:</strong> Gross margin slips below 71% or datacenter growth drops under 15%.</li>
            <li><strong>Invalidation:</strong> A customer capex cut threatens more than 10% of revenue.</li>
          </ul>
        </section>
        <section>
          <h2>Evidence Chips</h2>
          <ul>
            <li>Latest management comments were consistent with prior guidance. <a href="https://investor.nvidia.com/en-us/">NVIDIA IR</a></li>
            <li>Independent reporting did not show a formal guide cut. <a href="https://www.reuters.com/world/china/prices-nvidias-b300-server-1-million-china-us-curbs-sources-say-2026-04-30/">Reuters</a></li>
          </ul>
        </section>
        "#;
        let wrapped_reply = normalize_email_html("DoWhiz update", raw_reply);
        let workspace = write_workspace(
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.",
            &wrapped_reply,
        );
        let reply_path = workspace.join("reply_email_draft.html");

        ensure_expected_reply_artifact(&workspace, &reply_path, "")
            .expect("wrapped short contract should still validate");
    }

    #[test]
    fn oversized_reply_artifact_fails_mechanical_validation() {
        let repeated =
            "The latest public information does not change the call today. ".repeat(2000);
        let workspace = write_workspace(
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.",
            &format!(
                r#"
                <section>
                  <p><strong>As of:</strong> 2026-05-01 · <strong>Price:</strong> $120.00</p>
                  <p><strong>Investor question:</strong> Check whether anything material changed for NVDA since your last note. Only tell me if I should act.</p>
                </section>
                <section>
                  <h2>Decision Card</h2>
                  <table>
                    <tr><td>Monitor Status</td><td>No Material Change</td></tr>
                    <tr><td>New Money Action</td><td>Wait</td></tr>
                    <tr><td>Existing Holder Action</td><td>Hold/Do not add</td></tr>
                    <tr><td>Thesis Impact</td><td>No Material Change</td></tr>
                    <tr><td>Signal Quality</td><td>Moderate</td></tr>
                    <tr><td>Confidence</td><td>Medium</td></tr>
                  </table>
                  <p><strong>One-line rationale:</strong> Nothing material changed.</p>
                </section>
                <section>
                  <h2>Why Now</h2>
                  <p>{repeated}</p>
                </section>
                <section>
                  <h2>What Would Change The View</h2>
                  <ul>
                    <li><strong>Upgrade / Review Now:</strong> Revenue guide rises by more than 5%.</li>
                    <li><strong>Downgrade / De-risk:</strong> Gross margin slips below 71%.</li>
                    <li><strong>Invalidation:</strong> A customer capex cut threatens more than 10% of revenue.</li>
                  </ul>
                </section>
                <section>
                  <h2>Evidence Chips</h2>
                  <ul>
                    <li>Primary guidance held. <a href="https://investor.nvidia.com/en-us/">NVIDIA IR</a></li>
                    <li>Independent reporting stayed consistent. <a href="https://www.reuters.com/world/china/prices-nvidias-b300-server-1-million-china-us-curbs-sources-say-2026-04-30/">Reuters</a></li>
                  </ul>
                </section>
                "#
            ),
        );
        let reply_path = workspace.join("reply_email_draft.html");

        let err = ensure_expected_reply_artifact(&workspace, &reply_path, "")
            .expect_err("expected bounded monitor size violation");
        assert!(err
            .to_string()
            .contains("reply artifact exceeds bounded monitor size limit"));
    }

    #[test]
    fn html_reply_with_no_visible_text_fails_mechanical_validation() {
        let workspace = write_workspace(
            "Give me deep research on NVDA and tell me whether now is a good time to buy.",
            r#"
            <html><body><style>.hidden { display:none; }</style><div aria-hidden="true">hidden</div></body></html>
            "#,
        );
        let reply_path = workspace.join("reply_email_draft.html");

        let err = ensure_expected_reply_artifact(&workspace, &reply_path, "tail")
            .expect_err("empty visible html should fail");
        assert!(err
            .to_string()
            .contains("reply artifact has no visible text"));
    }

    #[test]
    fn incomplete_research_pseudo_memo_passes_mechanical_validation() {
        let workspace = write_workspace(
            "Give me a deep research about the Nokia stock, and tell me whether it is a good time to buy.",
            r#"
            <section>
              <p><strong>As of:</strong> 2026-05-01 · <strong>Price:</strong> Verification incomplete in this timed run</p>
              <p><strong>Investor question:</strong> Give me a deep research about the Nokia stock, and tell me whether it is a good time to buy.</p>
            </section>
            <section>
              <h2>Decision Card</h2>
              <table>
                <tr><td>Monitor Status</td><td>Watch Closely</td></tr>
                <tr><td>New Money Action</td><td>Wait</td></tr>
                <tr><td>Existing Holder Action</td><td>Hold/Do not add</td></tr>
                <tr><td>Thesis Impact</td><td>Mixed</td></tr>
                <tr><td>Signal Quality</td><td>Weak</td></tr>
                <tr><td>Confidence</td><td>Low</td></tr>
              </table>
              <p><strong>One-line rationale:</strong> Research did not complete within budget, so this is a best-available timed artifact rather than a completed underwriting.</p>
            </section>
            <section>
              <h2>Dual-Horizon Framing</h2>
              <h3>Near-Term Timing View</h3>
              <p>Do not commit fresh capital until the next verified issuer update closes the evidence gap.</p>
              <h3>Long-Term Ownership View</h3>
              <p>The thesis remains open, but the timed run stayed incomplete.</p>
            </section>
            <section>
              <h2>Verified Facts</h2>
              <p><strong>Research status:</strong> Incomplete research artifact.</p>
              <h3>What Was Found</h3>
              <ul>
                <li>Preserved research files existed from the interrupted run.</li>
                <li>The runtime kept the task from disappearing silently.</li>
              </ul>
              <h3>What Is Missing</h3>
              <ul>
                <li>A completed current-price cross-check.</li>
                <li>A finished issuer-release review.</li>
              </ul>
            </section>
            <section>
              <h2>Derived Metrics</h2>
              <table>
                <tr><th>Metric</th><th>Read-through</th><th>Formula / Inputs</th></tr>
                <tr><td>Fresh-entry conviction</td><td>Not reliably derivable</td><td>Not reliably derivable from the incomplete timed run because issuer metrics were not fully validated before the deadline.</td></tr>
              </table>
            </section>
            <section>
              <h2>Scenarios</h2>
              <h3>Bull Case</h3>
              <p>The next issuer update confirms margin above 11% and positive free cash flow.</p>
              <h3>Base Case</h3>
              <p>The story remains investable but evidence stays incomplete.</p>
              <h3>Bear Case</h3>
              <p>The next issuer update shows another 5% guide cut or another 200 bps margin reset.</p>
            </section>
            <section>
              <h2>Triggers</h2>
              <ul>
                <li><strong>Upgrade / Review Now:</strong> The next verified update shows operating margin above 11% and positive free cash flow.</li>
                <li><strong>Downgrade / De-risk:</strong> Guidance is cut by 5% or more, or margins step down another 200 bps.</li>
                <li><strong>Invalidation:</strong> Two more quarters pass without a verified path back to durable positive free cash flow.</li>
              </ul>
            </section>
            <section>
              <h2>Judgment</h2>
              <p>This is a best-available timed artifact.</p>
              <p><strong>What would be needed for Buy / Avoid / Add / Exit:</strong> a completed release review, a current valuation check, and verified margin plus cash-flow evidence.</p>
            </section>
            "#,
        );
        let reply_path = workspace.join("reply_email_draft.html");

        ensure_expected_reply_artifact(&workspace, &reply_path, "")
            .expect("runtime should not block based on semantic completeness");
    }

    #[test]
    fn incomplete_short_monitor_artifact_passes_mechanical_validation() {
        let workspace = write_workspace(
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.",
            r#"
            <section>
              <p><strong>As of:</strong> 2026-05-01 · <strong>Price:</strong> Verification incomplete in this timed monitor run</p>
              <p><strong>Investor question:</strong> Check whether anything material changed for NVDA since your last note. Only tell me if I should act.</p>
            </section>
            <section>
              <h2>Decision Card</h2>
              <table>
                <tr><td>Monitor Status</td><td>Insufficient Evidence</td></tr>
                <tr><td>New Money Action</td><td>Wait</td></tr>
                <tr><td>Existing Holder Action</td><td>Hold/Do not add</td></tr>
                <tr><td>Thesis Impact</td><td>Mixed</td></tr>
                <tr><td>Signal Quality</td><td>Weak</td></tr>
                <tr><td>Confidence</td><td>Low</td></tr>
              </table>
              <p><strong>One-line rationale:</strong> The monitor request did not preserve enough verified prior-note context inside the time budget to support a stronger act-now call.</p>
            </section>
            <section>
              <h2>Why Now</h2>
              <p><strong>Incomplete monitor check.</strong> This was forced into a short fail-soft update instead of timing out with no reply.</p>
            </section>
            <section>
              <h2>What Would Change The View</h2>
              <ul>
                <li><strong>Upgrade / Review Now:</strong> A verified issuer update changes revenue or margin expectations by at least 5%.</li>
                <li><strong>Downgrade / De-risk:</strong> A verified guidance cut of 5% or more, or a 200 bps margin reset.</li>
                <li><strong>Invalidation:</strong> Two more quarters pass without enough verified evidence to confirm the earlier thesis.</li>
              </ul>
            </section>
            <section>
              <h2>Evidence Chips</h2>
              <ul>
                <li>No real evidence links were preserved in this timed monitor fallback, so this artifact stays explicitly low-confidence.</li>
              </ul>
            </section>
            "#,
        );
        let reply_path = workspace.join("reply_email_draft.html");

        ensure_expected_reply_artifact(&workspace, &reply_path, "")
            .expect("runtime should not block based on semantic completeness");
    }
}
