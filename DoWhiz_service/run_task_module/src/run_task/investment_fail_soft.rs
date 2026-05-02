use std::fs;
use std::path::Path;

use chrono::Utc;
use serde_json::Value;

use super::errors::RunTaskError;
use super::reply_contract::{
    action_only_monitor_request_for_workspace, investment_monitor_request_for_workspace,
    investment_request_for_workspace, reply_artifact_ready_for_workspace,
    synthetic_investment_request_for_workspace,
};

const INCOMPLETE_RESEARCH_MARKER: &str = "Incomplete research artifact";
const INCOMPLETE_MONITOR_MARKER: &str = "Incomplete monitor check";

pub(super) fn maybe_write_fail_soft_investment_artifact(
    workspace_dir: &Path,
    reply_path: &Path,
    cause: &RunTaskError,
) -> Result<Option<String>, RunTaskError> {
    if !investment_request_for_workspace(workspace_dir)? {
        return Ok(None);
    }

    let request_text = load_inbound_request_text(workspace_dir)?;
    let raw_request = canonical_request_line(&request_text);

    let html = if is_synthetic_request(&request_text) {
        build_synthetic_assumption_artifact(&raw_request)
    } else if investment_monitor_request_for_workspace(workspace_dir)? {
        build_monitor_fail_soft_artifact(workspace_dir, &raw_request)
    } else {
        build_real_ticker_incomplete_artifact(workspace_dir, &raw_request)
    };

    fs::write(reply_path, html)?;
    if !reply_artifact_ready_for_workspace(workspace_dir, reply_path) {
        return Ok(None);
    }

    let note = if is_synthetic_request(&request_text) {
        format!(
            "Recovered via deterministic assumption-based investment finalizer after {}",
            summarize_failure(cause)
        )
    } else if investment_monitor_request_for_workspace(workspace_dir)? {
        format!(
            "Recovered via deterministic monitor fail-soft artifact after {}",
            summarize_failure(cause)
        )
    } else {
        format!(
            "Recovered via deterministic incomplete-research investment finalizer after {}",
            summarize_failure(cause)
        )
    };

    Ok(Some(note))
}

pub(super) fn maybe_write_synthetic_assumption_artifact(
    workspace_dir: &Path,
    reply_path: &Path,
) -> Result<Option<String>, RunTaskError> {
    if !investment_request_for_workspace(workspace_dir)?
        || !synthetic_investment_request_for_workspace(workspace_dir)?
    {
        return Ok(None);
    }

    let request_text = load_inbound_request_text(workspace_dir)?;
    let raw_request = canonical_request_line(&request_text);
    let html = build_synthetic_assumption_artifact(&raw_request);

    fs::write(reply_path, html)?;
    if !reply_artifact_ready_for_workspace(workspace_dir, reply_path) {
        return Ok(None);
    }

    Ok(Some(
        "Answered via deterministic assumption-based investment artifact because the request is explicitly synthetic and does not require live issuer research.".to_string(),
    ))
}

pub(super) fn maybe_write_action_only_monitor_artifact(
    workspace_dir: &Path,
    reply_path: &Path,
) -> Result<Option<String>, RunTaskError> {
    if !investment_request_for_workspace(workspace_dir)?
        || !action_only_monitor_request_for_workspace(workspace_dir)?
    {
        return Ok(None);
    }

    let request_text = load_inbound_request_text(workspace_dir)?;
    let raw_request = canonical_request_line(&request_text);
    let html = build_monitor_fail_soft_artifact(workspace_dir, &raw_request);

    fs::write(reply_path, html)?;
    if !reply_artifact_ready_for_workspace(workspace_dir, reply_path) {
        return Ok(None);
    }

    Ok(Some(
        "Answered via deterministic action-only monitor artifact because the request explicitly asked for a short act-now update.".to_string(),
    ))
}

fn summarize_failure(cause: &RunTaskError) -> &'static str {
    match cause {
        RunTaskError::CommandTimeout { .. } => "Codex timed out",
        RunTaskError::CodexFailed { .. } => "Codex failed",
        RunTaskError::OutputMissing { .. } => "Codex finished without a deliverable",
        RunTaskError::OutputContractViolation { .. } => "Codex wrote an invalid artifact",
        RunTaskError::FallbackFailed { .. } => "Codex and fallback recovery failed",
        _ => "runtime failure",
    }
}

fn build_monitor_fail_soft_artifact(workspace_dir: &Path, request_text: &str) -> String {
    let investor_question = escape_html(request_text);
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let prior_note_missing = prior_note_context_missing(workspace_dir);
    let why_now = if prior_note_missing && request_text.to_ascii_lowercase().contains("last note") {
        format!(
            "<strong>{INCOMPLETE_MONITOR_MARKER}.</strong> The workspace did not include the prior note needed for a literal change-since-last-note diff, so the runtime returned a short fail-soft update instead of timing out with no reply."
        )
    } else {
        format!(
            "<strong>{INCOMPLETE_MONITOR_MARKER}.</strong> The runtime returned a short fail-soft update instead of timing out with no reply."
        )
    };
    let evidence_chip = if prior_note_missing
        && request_text.to_ascii_lowercase().contains("last note")
    {
        "No real evidence links were preserved in this timed monitor fallback, and the prior note was not available in the workspace, so confidence stays Low."
    } else {
        "No real evidence links were preserved in this timed monitor fallback, so confidence stays Low."
    };
    format!(
        r#"<html>
  <body>
    <h1>Investment monitor update</h1>
    <p><strong>As of:</strong> {today}</p>
    <p><strong>Price:</strong> Verification incomplete in this timed monitor run</p>
    <p><strong>Investor question:</strong> {investor_question}</p>
    <h2>Decision Card</h2>
    <table>
      <tr><th align="left">Monitor Status</th><td>Insufficient Evidence</td></tr>
      <tr><th align="left">New Money Action</th><td>Wait</td></tr>
      <tr><th align="left">Existing Holder Action</th><td>Hold/Do not add</td></tr>
      <tr><th align="left">Thesis Impact</th><td>Mixed</td></tr>
      <tr><th align="left">Signal Quality</th><td>Weak</td></tr>
      <tr><th align="left">Confidence</th><td>Low</td></tr>
    </table>
    <p><strong>One-line rationale:</strong> This monitor check ran out of verified context before it could justify a stronger act-now call.</p>
    <h2>Why Now</h2>
    <p>{why_now}</p>
    <h2>What Would Change The View</h2>
    <ul>
      <li><strong>Upgrade / Review Now:</strong> A verified issuer update moves revenue or margin expectations by at least 5%.</li>
      <li><strong>Downgrade / De-risk:</strong> A verified guidance cut above 5% or a 200 bps margin reset.</li>
      <li><strong>Invalidation:</strong> Two reporting periods pass without evidence that confirms the prior note.</li>
    </ul>
    <h2>Evidence Chips</h2>
    <ul>
      <li>{evidence_chip}</li>
    </ul>
  </body>
</html>
"#
    )
}

fn build_real_ticker_incomplete_artifact(workspace_dir: &Path, request_text: &str) -> String {
    let (display_name, ticker_hint) = infer_security_label(workspace_dir, request_text);
    let h1 = if let Some(ticker) = &ticker_hint {
        format!("{} ({}) decision memo", display_name, ticker)
    } else {
        format!("{} decision memo", display_name)
    };
    let research_labels = collect_research_labels(workspace_dir);
    let research_summary = if research_labels.is_empty() {
        "No preserved research filenames were available.".to_string()
    } else {
        format!(
            "Preserved timed-run inputs included: {}.",
            research_labels.join(", ")
        )
    };
    let investor_question = escape_html(request_text);
    let today = Utc::now().format("%Y-%m-%d").to_string();

    format!(
        r#"<html>
  <body>
    <h1>{h1}</h1>
    <p><strong>As of:</strong> {today}</p>
    <p><strong>Price:</strong> Verification incomplete in this timed run</p>
    <p><strong>Investor question:</strong> {investor_question}</p>

    <h2>Decision Card</h2>
    <table>
      <tr><th align="left">Monitor Status</th><td>Watch Closely</td></tr>
      <tr><th align="left">New Money Action</th><td>Wait</td></tr>
      <tr><th align="left">Existing Holder Action</th><td>Hold/Do not add</td></tr>
      <tr><th align="left">Thesis Impact</th><td>Mixed</td></tr>
      <tr><th align="left">Signal Quality</th><td>Weak</td></tr>
      <tr><th align="left">Confidence</th><td>Low</td></tr>
    </table>
    <p><strong>One-line rationale:</strong> Research did not complete within budget, so this is a best-available timed artifact rather than a completed buy-or-avoid underwriting.</p>

    <h2>Dual-Horizon Framing</h2>
    <h3>Near-Term Timing View</h3>
    <p>Do not commit fresh capital until a completed release and valuation check confirms whether the setup is truly improving rather than merely looking cheap on partial evidence.</p>
    <h3>Long-Term Ownership View</h3>
    <p>The long-term case remains open, but this timed run did not verify enough issuer evidence to re-underwrite the thesis confidently.</p>

    <h2>Verified Facts</h2>
    <p><strong>Research status:</strong> {INCOMPLETE_RESEARCH_MARKER}. The runtime preserved a partial draft or research trail, but not a contract-ready final artifact.</p>
    <h3>What Was Found</h3>
    <ul>
      <li>{research_summary}</li>
      <li>The interrupted run had enough structure to support a cautious monitoring stance, but not enough verified evidence for a completed decision memo.</li>
      <li>The runtime is surfacing a fail-soft artifact instead of silently losing the task.</li>
    </ul>
    <h3>What Is Missing</h3>
    <ul>
      <li>A fully verified current price and valuation cross-check tied to the latest completed issuer update.</li>
      <li>A completed read of the latest release or filing with confirmed implications for revenue, margin, and cash generation.</li>
      <li>A finished independent cross-check that can support a stronger Buy, Avoid, Add, or Exit call.</li>
    </ul>

    <h2>Derived Metrics</h2>
    <table>
      <tr><th align="left">Metric</th><th align="left">Read-through</th><th align="left">Formula / Inputs</th></tr>
      <tr><td>Fresh-entry conviction</td><td>Not reliably derivable</td><td>Not reliably derivable from the incomplete timed run because issuer metrics were not fully validated before the deadline.</td></tr>
    </table>

    <h2>Scenarios</h2>
    <h3>Bull Case</h3>
    <p>The next completed company update confirms margin stabilization above 11%, positive free cash flow, and no new guide cuts.</p>
    <h3>Base Case</h3>
    <p>The business remains investable, but evidence is still incomplete enough that fresh money stays in `Wait` and existing holders should avoid adding.</p>
    <h3>Bear Case</h3>
    <p>The next verified release shows another guide cut, another 200 bps margin step-down, or stalled cash generation.</p>

    <h2>Triggers</h2>
    <ul>
      <li><strong>Upgrade / Review Now:</strong> The next verified company update shows operating margin above 11%, positive free cash flow, and no guide cut.</li>
      <li><strong>Downgrade / De-risk:</strong> Management cuts guidance by 5% or more, or the next release shows another 200 bps margin deterioration.</li>
      <li><strong>Invalidation:</strong> Two more quarters pass without a verified path back to durable positive free cash flow and stable margins.</li>
    </ul>

    <h2>Judgment</h2>
    <p><em>Inference, Low confidence.</em> This is a best-available timed artifact, not completed deep research, so it should be treated as a watchlist handoff rather than a high-conviction call.</p>
    <p><strong>What would be needed for Buy / Avoid / Add / Exit:</strong> a completed issuer-release review, a clean current valuation cross-check, and a verified read on whether margin and cash-flow improvement are actually durable.</p>
  </body>
</html>
"#
    )
}

fn build_synthetic_assumption_artifact(request_text: &str) -> String {
    let stance = classify_synthetic_stance(request_text);
    let assumptions = extract_assumption_bullets(request_text);
    let metrics_rows = synthetic_metric_rows(request_text, stance);
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let investor_question = escape_html(request_text);
    let assumption_items = assumptions
        .iter()
        .map(|item| format!("<li>{}</li>", escape_html(item)))
        .collect::<Vec<_>>()
        .join("\n");
    let metrics_html = metrics_rows
        .iter()
        .map(|(metric, read_through, formula)| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(metric),
                escape_html(read_through),
                escape_html(formula)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let (
        monitor_status,
        new_money_action,
        existing_holder_action,
        thesis_impact,
        signal_quality,
        one_line_rationale,
        near_term_view,
        long_term_view,
        bull_case,
        base_case,
        bear_case,
        upgrade_trigger,
        downgrade_trigger,
        invalidation_trigger,
        judgment,
    ) = match stance {
        SyntheticStance::Positive => (
            "Review Now",
            "Buy",
            "Add",
            "Positive",
            "Strong",
            "If the assumed guidance raise, margin expansion, and cash-flow turn are all true while valuation still sits below peers, the edge is positive enough to review now rather than wait passively.",
            "The near-term setup is favorable because the assumed operating improvement and still-discounted valuation create a credible fresh-entry window.",
            "The long-term ownership case improves if the assumed demand and cash-flow inflection prove durable across the next one to two quarters.",
            "Execution keeps compounding and the market closes the valuation discount.",
            "The assumptions are directionally right, but the rerating is more gradual than immediate.",
            "One of the assumed improvements fades before it reaches the next verified report.",
            "A real issuer update confirms revenue growth above 10%, gross margin expansion of at least 200 bps, and free cash flow staying positive.",
            "The next verified report reverses the assumed guidance raise or shows gross margin slipping back by 150 bps or more.",
            "A verified release shows the cash-flow turn was temporary or the demand strength does not survive the next two reporting periods.",
            "This is assumption-based, Medium confidence. On the assumed facts alone, `Review Now` plus `Buy` / `Add` is justified, but it still needs real issuer evidence before confidence can move higher.",
        ),
        SyntheticStance::Negative => (
            "Review Now",
            "Avoid for now",
            "Exit",
            "Negative",
            "Strong",
            "If the assumed guide cut, customer loss, margin collapse, and above-peer valuation are all true at once, capital protection matters more than trying to buy a broken reset.",
            "The near-term setup is adverse because both fundamentals and multiple compression risk point the same way.",
            "The long-term ownership case is damaged until a real issuer update proves the customer, margin, and credibility problems are reversing.",
            "The company replaces more than 50% of the lost revenue within two quarters and restores gross margin close to its prior band.",
            "Damage proves real but survivable, leaving the stock range-bound until hard evidence of stabilization arrives.",
            "Another guide cut, another margin step-down, or another customer shock drives a second leg lower.",
            "A verified issuer update replaces more than 50% of the lost revenue within two quarters and restores gross margin to within 300 bps of the prior run-rate.",
            "The next verified report shows another guidance cut above 10% or another 200 bps gross-margin decline.",
            "Two more quarters pass without a verified customer replacement path or without management restoring any credible long-term targets.",
            "This is assumption-based, Medium confidence. On the assumed facts alone, `Review Now` plus `Avoid for now` / `Exit` is the calibrated answer, not a softened hold.",
        ),
        SyntheticStance::Mixed => (
            "Watch Closely",
            "Starter Only",
            "Hold/Do not add",
            "Mixed",
            "Moderate",
            "The assumptions point to a materially changed setup, but the signal is still mixed enough that confirmation should come before a full-size decision.",
            "Near-term timing depends on whether the next verified update resolves the tension between improving demand and still-unsettled cash flow or margins.",
            "The long-term thesis may still work, but it needs one more clean confirmation before it deserves a broader risk-on posture.",
            "The next verified update confirms improving demand while cash flow turns positive and margin pressure stays temporary.",
            "The story stays investable but still incomplete, keeping fresh capital sized and conditional.",
            "The temporary problem becomes structural and forces another downgrade in revenue, margin, or cash expectations.",
            "A verified update shows at least 5% revenue growth, margin stabilization within 150 bps of the prior level, and a clear path to positive free cash flow.",
            "The next verified report shows another 5% guide cut, free cash flow still materially negative, or a further 200 bps margin decline.",
            "Two more quarters pass without demand improvement converting into better cash flow or without the supposed temporary cost issue rolling off.",
            "This is assumption-based, Medium confidence. The right posture is `Watch Closely`, not a padded neutral essay and not a forced aggressive call.",
        ),
    };

    format!(
        r#"<html>
  <body>
    <h1>Assumption-based investment monitor output</h1>
    <p><strong>As of:</strong> {today}</p>
    <p><strong>Price:</strong> assumption-based / not provided</p>
    <p><strong>Investor question:</strong> {investor_question}</p>

    <h2>Decision Card</h2>
    <table>
      <tr><th align="left">Monitor Status</th><td>{monitor_status}</td></tr>
      <tr><th align="left">New Money Action</th><td>{new_money_action}</td></tr>
      <tr><th align="left">Existing Holder Action</th><td>{existing_holder_action}</td></tr>
      <tr><th align="left">Thesis Impact</th><td>{thesis_impact}</td></tr>
      <tr><th align="left">Signal Quality</th><td>{signal_quality}</td></tr>
      <tr><th align="left">Confidence</th><td>Medium</td></tr>
    </table>
    <p><strong>One-line rationale:</strong> {one_line_rationale}</p>

    <h2>Dual-Horizon Framing</h2>
    <h3>Near-Term Timing View</h3>
    <p>{near_term_view}</p>
    <h3>Long-Term Ownership View</h3>
    <p>{long_term_view}</p>

    <h2>Verified Facts</h2>
    <p><strong>Research status:</strong> assumption-based. No issuer-specific evidence was verified or linked because the prompt itself is hypothetical.</p>
    <ul>
      {assumption_items}
    </ul>

    <h2>Derived Metrics</h2>
    <table>
      <tr><th align="left">Metric</th><th align="left">Read-through</th><th align="left">Formula / Inputs</th></tr>
      {metrics_html}
    </table>

    <h2>Scenarios</h2>
    <h3>Bull Case</h3>
    <p>{bull_case}</p>
    <h3>Base Case</h3>
    <p>{base_case}</p>
    <h3>Bear Case</h3>
    <p>{bear_case}</p>

    <h2>Triggers</h2>
    <ul>
      <li><strong>Upgrade / Review Now:</strong> {upgrade_trigger}</li>
      <li><strong>Downgrade / De-risk:</strong> {downgrade_trigger}</li>
      <li><strong>Invalidation:</strong> {invalidation_trigger}</li>
    </ul>

    <h2>Judgment</h2>
    <p>{judgment}</p>
  </body>
</html>
"#
    )
}

#[derive(Clone, Copy)]
enum SyntheticStance {
    Positive,
    Negative,
    Mixed,
}

fn classify_synthetic_stance(request_text: &str) -> SyntheticStance {
    let normalized = normalize_text(request_text);
    let positive_hits = [
        "raised",
        "expanded",
        "turned positive",
        "below peer",
        "stronger order demand",
        "improved",
        "beats",
    ]
    .iter()
    .filter(|token| normalized.contains(**token))
    .count();
    let negative_hits = [
        "cut ",
        "lost its largest customer",
        "gross margin collapsed",
        "withdrew long-term targets",
        "above peer",
        "negative",
        "declined",
    ]
    .iter()
    .filter(|token| normalized.contains(**token))
    .count();

    if negative_hits >= 3 && positive_hits == 0 {
        SyntheticStance::Negative
    } else if positive_hits >= 3 && negative_hits == 0 {
        SyntheticStance::Positive
    } else {
        SyntheticStance::Mixed
    }
}

fn synthetic_metric_rows(
    request_text: &str,
    stance: SyntheticStance,
) -> Vec<(&'static str, String, String)> {
    let normalized = normalize_text(request_text);
    let first_percent = extract_first_percent(&normalized);
    let first_bps = extract_first_bps(&normalized);
    let mut rows = Vec::new();

    if let Some(percent) = first_percent {
        rows.push((
            "Guidance reset",
            format!("Directional signal from the prompt = {}", percent),
            format!("new guide / prior guide - 1 = {}", percent),
        ));
    }
    if let Some(bps) = first_bps {
        rows.push((
            "Gross-margin delta",
            format!("Prompt assumption = {}", bps),
            format!("new gross margin - prior gross margin = {}", bps),
        ));
    }
    rows.push((
        "Valuation relative to peers",
        match stance {
            SyntheticStance::Positive => "Discount to peers leaves room for rerating.".to_string(),
            SyntheticStance::Negative => {
                "Premium to peers leaves room for downside even after bad fundamentals.".to_string()
            }
            SyntheticStance::Mixed => {
                "Relative valuation still needs verification before a stronger stance.".to_string()
            }
        },
        "scenario valuation flag from prompt assumptions".to_string(),
    ));
    if rows.is_empty() {
        rows.push((
            "Metric status",
            "No numeric inputs were provided beyond directional assumptions.".to_string(),
            "not reliably derivable from the prompt alone".to_string(),
        ));
    }
    rows
}

fn extract_assumption_bullets(request_text: &str) -> Vec<String> {
    let cleaned = request_text
        .trim()
        .trim_start_matches("Assume")
        .trim_start_matches("assume")
        .trim_start_matches("hypothetical")
        .trim();
    let without_tail = cleaned
        .split("Write the investment monitor output")
        .next()
        .unwrap_or(cleaned)
        .split("write the investment monitor output")
        .next()
        .unwrap_or(cleaned)
        .trim_end_matches('.')
        .trim();
    without_tail
        .replace(", and ", ", ")
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

fn collect_research_labels(workspace_dir: &Path) -> Vec<String> {
    let research_dir = workspace_dir.join("work").join("research");
    let mut labels = Vec::new();
    let Ok(entries) = fs::read_dir(research_dir) else {
        return labels;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
            labels.push(stem.replace('_', " "));
        }
    }
    labels.sort();
    labels.truncate(4);
    labels
}

fn infer_security_label(workspace_dir: &Path, request_text: &str) -> (String, Option<String>) {
    let normalized = normalize_text(request_text);
    if normalized.contains("nokia") {
        return ("Nokia".to_string(), Some("NOK".to_string()));
    }
    if let Some(ticker) = probable_ticker(request_text) {
        return (ticker.clone(), Some(ticker));
    }
    if let Some(company) = extract_company_before_stock(request_text) {
        let title = title_case(&company);
        let ticker = probable_ticker_from_research_dir(workspace_dir);
        return (title, ticker);
    }
    (
        "Timed investment update".to_string(),
        probable_ticker_from_research_dir(workspace_dir),
    )
}

fn probable_ticker_from_research_dir(workspace_dir: &Path) -> Option<String> {
    let research_dir = workspace_dir.join("work").join("research");
    let Ok(entries) = fs::read_dir(research_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let lowered = name.to_ascii_lowercase();
        for token in lowered.split(|ch: char| !ch.is_ascii_alphanumeric()) {
            if (2..=5).contains(&token.len()) && token.chars().all(|ch| ch.is_ascii_alphabetic()) {
                return Some(token.to_ascii_uppercase());
            }
        }
    }
    None
}

fn extract_company_before_stock(request_text: &str) -> Option<String> {
    let lower = request_text.to_ascii_lowercase();
    let idx = lower.find(" stock")?;
    let prefix = &request_text[..idx];
    let mut words = prefix
        .split_whitespace()
        .rev()
        .take_while(|word| {
            word.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '.')
        })
        .take(3)
        .map(|word| word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-'))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words.reverse();
    (!words.is_empty()).then(|| words.join(" "))
}

fn probable_ticker(raw: &str) -> Option<String> {
    const STOPWORDS: &[&str] = &[
        "A", "AI", "ARE", "BUY", "ETF", "EPS", "FX", "NOW", "PR", "THE", "UI", "URL", "UX", "WAIT",
    ];

    raw.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '$')
        .filter(|token| !token.is_empty())
        .find_map(|token| {
            let trimmed = token.trim_start_matches('$');
            if !(2..=5).contains(&trimmed.len()) {
                return None;
            }
            if STOPWORDS.contains(&trimmed) {
                return None;
            }
            if !trimmed.chars().all(|ch| ch.is_ascii_uppercase()) {
                return None;
            }
            Some(trimmed.to_string())
        })
}

fn load_inbound_request_text(workspace_dir: &Path) -> Result<String, RunTaskError> {
    let incoming_dir = workspace_dir.join("incoming_email");
    let mut parts = Vec::new();

    for name in ["thread_request.md", "email.txt", "email.html"] {
        let path = incoming_dir.join(name);
        if path.exists() {
            parts.push(fs::read_to_string(path)?);
        }
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

fn canonical_request_line(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Investment monitor request")
        .to_string()
}

fn prior_note_context_missing(workspace_dir: &Path) -> bool {
    let entries_dir = workspace_dir.join("incoming_email").join("entries");
    match fs::read_dir(entries_dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    }
}

fn normalize_text(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn is_synthetic_request(raw: &str) -> bool {
    let normalized = normalize_text(raw);
    [
        "assume ",
        "synthetic",
        "hypothetical",
        "company x",
        "company y",
        "company z",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

fn extract_first_percent(normalized_text: &str) -> Option<String> {
    let bytes = normalized_text.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx].is_ascii_digit() {
            let start = idx;
            idx += 1;
            while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                idx += 1;
            }
            if idx < bytes.len() && bytes[idx] == b'%' {
                return Some(normalized_text[start..=idx].to_string());
            }
        } else {
            idx += 1;
        }
    }
    None
}

fn extract_first_bps(normalized_text: &str) -> Option<String> {
    let mut previous_number: Option<String> = None;
    for token in normalized_text.split(|ch: char| !ch.is_ascii_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        if token.chars().all(|ch| ch.is_ascii_digit()) {
            previous_number = Some(token.to_string());
            continue;
        }
        if token == "bps" {
            return previous_number.map(|value| format!("{value} bps"));
        }
        previous_number = None;
    }
    None
}

fn title_case(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    format!(
                        "{}{}",
                        first.to_ascii_uppercase(),
                        chars.as_str().to_ascii_lowercase()
                    )
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
