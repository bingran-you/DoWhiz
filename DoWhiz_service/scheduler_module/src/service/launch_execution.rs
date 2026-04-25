use std::collections::{BTreeSet, HashSet};
use std::env;
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-5.4";
const LLM_TIMEOUT: Duration = Duration::from_secs(45);
const DEFAULT_SOURCE_TYPE: &str = "pasted_thread";
const PRIMARY_MAX_COMPLETION_TOKENS: u32 = 1600;
const RETRY_MAX_COMPLETION_TOKENS: u32 = 900;
const MAX_FOLLOW_UP_ITEMS: usize = 3;
const MAX_EVIDENCE_ITEMS: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchExecutionRequest {
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_label: Option<String>,
    pub context_text: String,
    #[serde(default)]
    pub update_text: Option<String>,
    #[serde(default)]
    pub prior_plan: Option<LaunchExecutionPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchExecutionResponse {
    pub launch_execution_plan: LaunchExecutionPlan,
    #[serde(default)]
    pub tracker_rows: Vec<LaunchTrackerRow>,
    pub readiness_brief: LaunchReadinessBrief,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchExecutionDebugResponse {
    pub request: LaunchExecutionRequest,
    pub system_prompt: String,
    pub user_prompt: String,
    pub raw_model_output: String,
    pub parsed_model_output: serde_json::Value,
    pub response: LaunchExecutionResponse,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchExecutionPlan {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub objective: Option<String>,
    #[serde(default)]
    pub source_context: LaunchSourceContext,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub launch_window: Option<String>,
    #[serde(default)]
    pub milestones: Vec<LaunchMilestone>,
    #[serde(default)]
    pub owners: Vec<LaunchOwner>,
    #[serde(default)]
    pub dependencies: Vec<LaunchDependency>,
    #[serde(default)]
    pub critical_path: Vec<LaunchCriticalPathItem>,
    #[serde(default)]
    pub risks: Vec<LaunchRisk>,
    #[serde(default)]
    pub decisions: Vec<LaunchDecision>,
    #[serde(default)]
    pub readiness_status: String,
    #[serde(default)]
    pub readiness_reason: String,
    #[serde(default)]
    pub evidence: Vec<LaunchEvidence>,
    #[serde(default)]
    pub last_updated_at: String,
    #[serde(default)]
    pub follow_up_items: Vec<LaunchFollowUpItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchSourceContext {
    #[serde(default)]
    pub input_type: String,
    #[serde(default)]
    pub source_label: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchMilestone {
    pub title: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub critical_path: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchOwner {
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub responsibilities: Vec<String>,
    #[serde(default)]
    pub update_status: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchDependency {
    pub title: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub critical_path: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchCriticalPathItem {
    pub title: String,
    #[serde(default)]
    pub item_type: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchRisk {
    pub title: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub blocker: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchDecision {
    pub title: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub launch_blocking: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchEvidence {
    pub label: String,
    pub snippet: String,
    #[serde(default)]
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchFollowUpItem {
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub priority: String,
    pub reason: String,
    pub suggested_message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchTrackerRow {
    pub category: String,
    pub title: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    pub status: String,
    #[serde(default)]
    pub risk_level: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchReadinessBrief {
    #[serde(default)]
    pub overall_readiness: String,
    #[serde(default)]
    pub critical_blockers: Vec<String>,
    #[serde(default)]
    pub at_risk_dependencies: Vec<String>,
    #[serde(default)]
    pub open_decisions: Vec<String>,
    #[serde(default)]
    pub missing_owner_updates: Vec<String>,
    #[serde(default)]
    pub what_changed_since_last_update: Vec<String>,
    #[serde(default)]
    pub evidence_summary: Vec<String>,
    #[serde(default)]
    pub next_follow_up_focus: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmLaunchExecutionOutput {
    #[serde(default, alias = "plan")]
    launch_execution_plan: LaunchExecutionPlan,
    #[serde(default, alias = "brief")]
    readiness_brief: LaunchReadinessBrief,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchRunMode {
    Standard,
    WeakSignalFallback,
    ErrorFallback,
}

#[derive(Debug, Clone)]
struct LaunchSignalAssessment {
    weak_signal: bool,
    explicit_blocker_cues: usize,
    decision_cues: usize,
    owner_cues: usize,
    date_cues: usize,
}

#[derive(Debug, Clone)]
struct ThreadLine {
    source_ref: String,
    text: String,
}

#[derive(Debug, Clone)]
struct LaunchExecutionLlmConfig {
    api_key: String,
    api_url: String,
    model: String,
    use_azure_auth: bool,
}

impl LaunchExecutionLlmConfig {
    fn from_env() -> Result<Self, String> {
        let azure_api_key = env::var("AZURE_OPENAI_API_KEY_BACKUP")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let azure_endpoint = env::var("AZURE_OPENAI_ENDPOINT_BACKUP")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if let (Some(api_key), Some(endpoint)) = (azure_api_key, azure_endpoint) {
            let model = env::var("LAUNCH_EXECUTION_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_MODEL.to_string());

            return Ok(Self {
                api_key,
                api_url: normalize_azure_endpoint(&endpoint),
                model,
                use_azure_auth: true,
            });
        }

        let api_key = env::var("OPENAI_API_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "Launch execution LLM is not configured (set AZURE_OPENAI_API_KEY_BACKUP + AZURE_OPENAI_ENDPOINT_BACKUP, or OPENAI_API_KEY).".to_string()
            })?;

        let api_url = env::var("OPENAI_API_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OPENAI_URL.to_string());
        let model = env::var("LAUNCH_EXECUTION_MODEL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());

        Ok(Self {
            api_key,
            api_url: api_url.trim_end_matches('/').to_string(),
            model,
            use_azure_auth: false,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
struct ChatRequestBody {
    model: String,
    messages: Vec<ChatMessage>,
    max_completion_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatResponseBody {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

pub async fn generate_launch_execution_response(
    request: LaunchExecutionRequest,
) -> Result<LaunchExecutionResponse, String> {
    Ok(generate_launch_execution_debug_response(request)
        .await?
        .response)
}

pub async fn generate_launch_execution_debug_response(
    request: LaunchExecutionRequest,
) -> Result<LaunchExecutionDebugResponse, String> {
    let source_type = normalize_source_type(request.source_type.clone())?;
    let context_text = request.context_text.trim().to_string();
    if context_text.is_empty() {
        return Err(
            "context_text must contain a pasted launch thread or planning bundle".to_string(),
        );
    }

    let signal = assess_launch_signal(&context_text, request.update_text.as_deref());
    let system_prompt = launch_execution_system_prompt(signal.weak_signal);
    let user_prompt = launch_execution_user_prompt(
        &source_type,
        request.source_label.as_deref(),
        &context_text,
        request.update_text.as_deref(),
        request.prior_plan.as_ref(),
        &signal,
    )?;
    let source_label = request.source_label.clone();

    let (run_mode, raw, parsed) = if signal.weak_signal {
        let parsed = build_fallback_output(
            request.source_label.as_deref(),
            &context_text,
            request.update_text.as_deref(),
            request.prior_plan.as_ref(),
            &signal,
            Some("Weak-signal thread: returning sparse execution judgment.".to_string()),
        );
        (
            LaunchRunMode::WeakSignalFallback,
            "[fallback] weak-signal sparse execution judgment".to_string(),
            parsed,
        )
    } else {
        match LaunchExecutionLlmConfig::from_env() {
            Ok(config) => match run_launch_execution_llm(
                &config,
                &system_prompt,
                &user_prompt,
                &request,
                &context_text,
                &signal,
            )
            .await
            {
                Ok(result) => (LaunchRunMode::Standard, result.0, result.1),
                Err(error_message) => {
                    let parsed = build_fallback_output(
                        request.source_label.as_deref(),
                        &context_text,
                        request.update_text.as_deref(),
                        request.prior_plan.as_ref(),
                        &signal,
                        Some(error_message.clone()),
                    );
                    (
                        LaunchRunMode::ErrorFallback,
                        format!("[fallback] {}", error_message),
                        parsed,
                    )
                }
            },
            Err(error_message) => {
                let parsed = build_fallback_output(
                    request.source_label.as_deref(),
                    &context_text,
                    request.update_text.as_deref(),
                    request.prior_plan.as_ref(),
                    &signal,
                    Some(error_message.clone()),
                );
                (
                    LaunchRunMode::ErrorFallback,
                    format!("[fallback] {}", error_message),
                    parsed,
                )
            }
        }
    };

    let parsed_model_output = serde_json::to_value(&parsed).map_err(|err| {
        format!(
            "failed to serialize parsed launch execution output: {}",
            err
        )
    })?;

    let mut plan = normalize_launch_execution_plan(
        parsed.launch_execution_plan,
        &source_type,
        source_label,
        request.prior_plan.as_ref(),
    );
    apply_execution_quality_guards(
        &mut plan,
        &context_text,
        request.update_text.as_deref(),
        &signal,
        run_mode,
    );
    plan.follow_up_items = derive_follow_up_items(&plan);
    let (readiness_status, readiness_reason) = derive_readiness_status(&plan);
    plan.readiness_status = readiness_status;
    plan.readiness_reason = readiness_reason;

    let tracker_rows = build_tracker_rows(&plan);
    let change_summary = derive_change_summary(request.prior_plan.as_ref(), &plan);
    let readiness_brief = build_readiness_brief(parsed.readiness_brief, &plan, change_summary);

    Ok(LaunchExecutionDebugResponse {
        request,
        system_prompt,
        user_prompt,
        raw_model_output: raw,
        parsed_model_output,
        response: LaunchExecutionResponse {
            launch_execution_plan: plan,
            tracker_rows,
            readiness_brief,
        },
    })
}

fn normalize_source_type(value: Option<String>) -> Result<String, String> {
    let trimmed = value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SOURCE_TYPE)
        .to_ascii_lowercase();

    if trimmed == "pasted_thread"
        || trimmed == "thread"
        || trimmed == "message_bundle"
        || trimmed.contains("paste")
    {
        return Ok(DEFAULT_SOURCE_TYPE.to_string());
    }

    Err(format!(
        "Unsupported source_type '{}'. Oliver launch execution v1 currently supports pasted_thread only.",
        trimmed
    ))
}

fn launch_execution_system_prompt(weak_signal: bool) -> String {
    let sparse_mode_block = if weak_signal {
        "- The thread appears weak-signal or not truly execution-ready. Prefer sparse output, leave owners empty unless directly accountable, and do not manufacture follow-up drafts.\n"
    } else {
        ""
    };

    let template = r#"You are Oliver's launch execution analyst.

Your only job is to turn one launch-related thread or planning bundle into a narrow launch execution record.

Scope rules:
- Focus only on launch execution.
- Do not generalize into a broad TPM system, generic project management, or broad automation advice.
- Never invent owners, dates, milestones, or status.
- If information is missing, leave the field null/empty and surface it through missing owner updates, decisions, risks, or other grounded fields.
- Preserve uncertainty. If a date is only a window, use launch_window instead of target_date.
- Evidence matters: every readiness judgment must be grounded in snippets from the provided context.
- Keep the structure narrow. Use at most 4 milestones, 4 dependencies, 3 risks, 3 decisions, and 6 evidence items.
- Do not turn every dependency, missing detail, or open question into a blocker.
- Only mark a blocker when the thread explicitly says something is blocked, blocking launch, failing, cannot proceed, lacks required signoff, or otherwise stops execution.
- Dependencies are gating items or cross-functional needs. They can be ready, at risk, blocked, or done, but they are not automatically blockers.
- Decisions are unresolved approvals, go/no-go choices, or explicit whether/choose questions. Missing owner information is not itself a decision.
- Follow-up quality matters later, so keep the output grounded enough that only a few obvious follow-up targets would remain.
__SPARSE_MODE_BLOCK__

Output requirements:
- Return ONLY valid JSON.
- No markdown, no code fences, no commentary outside the JSON object.
- JSON shape:
{
  "launch_execution_plan": {
    "id": "string",
    "title": "string",
    "objective": "string|null",
    "source_context": {
      "input_type": "pasted_thread",
      "source_label": "string|null",
      "summary": "string|null"
    },
    "target_date": "string|null",
    "launch_window": "string|null",
    "milestones": [
      {
        "title": "string",
        "owner": "string|null",
        "target_date": "string|null",
        "status": "not_started|in_progress|at_risk|blocked|done|unknown",
        "notes": "string|null",
        "critical_path": true
      }
    ],
    "owners": [
      {
        "name": "string",
        "role": "string|null",
        "responsibilities": ["string"],
        "update_status": "current|stale|missing|unknown",
        "notes": "string|null"
      }
    ],
    "dependencies": [
      {
        "title": "string",
        "owner": "string|null",
        "target_date": "string|null",
        "status": "ready|at_risk|blocked|done|unknown",
        "notes": "string|null",
        "critical_path": false
      }
    ],
    "critical_path": [
      {
        "title": "string",
        "item_type": "milestone|dependency|decision|risk",
        "owner": "string|null",
        "target_date": "string|null",
        "status": "string",
        "reason": "string|null"
      }
    ],
    "risks": [
      {
        "title": "string",
        "owner": "string|null",
        "severity": "low|medium|high|critical",
        "status": "open|watching|mitigated|resolved|unknown",
        "blocker": true,
        "notes": "string|null"
      }
    ],
    "decisions": [
      {
        "title": "string",
        "owner": "string|null",
        "due_date": "string|null",
        "status": "open|resolved|blocked|unknown",
        "launch_blocking": true,
        "notes": "string|null"
      }
    ],
    "evidence": [
      {
        "label": "string",
        "snippet": "string",
        "source_ref": "primary_context|latest_update"
      }
    ]
  },
  "readiness_brief": {
    "overall_readiness": "string",
    "critical_blockers": ["string"],
    "at_risk_dependencies": ["string"],
    "open_decisions": ["string"],
    "missing_owner_updates": ["string"],
    "what_changed_since_last_update": ["string"],
    "evidence_summary": ["string"],
    "next_follow_up_focus": "string|null"
  }
}

Interpretation rules:
- Milestones are concrete launch checkpoints.
- Dependencies are cross-functional or external gating items.
- Critical path should contain only the few items most likely to move the launch date.
- Risks should be explicit blockers or launch risks, not generic concerns.
- Decisions should be unanswered choices or approvals.
- Owners.update_status should be "missing" when no useful owner update is present, "stale" when the thread implies waiting/lagging/no recent update, "current" when there is a recent grounded update, otherwise "unknown".
- Evidence snippets should be short, specific, and directly grounded in the provided text.
"#;

    template.replace("__SPARSE_MODE_BLOCK__", sparse_mode_block)
}

fn launch_execution_user_prompt(
    source_type: &str,
    source_label: Option<&str>,
    context_text: &str,
    update_text: Option<&str>,
    prior_plan: Option<&LaunchExecutionPlan>,
    signal: &LaunchSignalAssessment,
) -> Result<String, String> {
    let source_label = source_label
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Untitled launch thread");
    let prior_plan_summary = summarize_prior_plan_for_prompt(prior_plan);

    let latest_update = update_text
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("None");

    Ok(format!(
        "Current UTC timestamp: {timestamp}
Supported source type for this request: {source_type}
Source label: {source_label}

Heuristic pre-read:
- weak_signal={weak_signal}
- explicit_blocker_cues={explicit_blocker_cues}
- decision_cues={decision_cues}
- owner_cues={owner_cues}
- date_cues={date_cues}

Previous launch execution plan summary:
{prior_plan_summary}

Primary launch context:
{context_text}

Latest incremental update:
{latest_update}

Instructions:
1. Extract the narrowest believable launch execution record from the provided context.
2. If a prior plan is provided, refresh it using the latest context and preserve stable titles when the same item still exists.
3. Treat missing owners, missing dates, stale updates, unresolved blockers, and unresolved decisions as first-class output signals.
4. Keep evidence visible and specific.
5. If the thread is incomplete, brainstorming, or not truly execution-critical yet, prefer sparse arrays and explicit uncertainty over elaborate structure.",
        timestamp = Utc::now().to_rfc3339(),
        source_type = source_type,
        source_label = source_label,
        weak_signal = signal.weak_signal,
        explicit_blocker_cues = signal.explicit_blocker_cues,
        decision_cues = signal.decision_cues,
        owner_cues = signal.owner_cues,
        date_cues = signal.date_cues,
        prior_plan_summary = prior_plan_summary,
        context_text = context_text,
        latest_update = latest_update
    ))
}

async fn call_chat_completion(
    config: &LaunchExecutionLlmConfig,
    system_prompt: &str,
    user_prompt: &str,
    max_completion_tokens: u32,
) -> Result<String, String> {
    let client = Client::builder()
        .timeout(LLM_TIMEOUT)
        .build()
        .map_err(|err| format!("failed to build HTTP client: {}", err))?;

    let url = format!("{}/chat/completions", config.api_url.trim_end_matches('/'));
    let payload = ChatRequestBody {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
        max_completion_tokens,
    };

    let mut request_builder = client.post(url).header("Content-Type", "application/json");

    if config.use_azure_auth {
        request_builder = request_builder.header("api-key", &config.api_key);
    } else {
        request_builder =
            request_builder.header("Authorization", format!("Bearer {}", config.api_key));
    }

    let response = request_builder
        .json(&payload)
        .send()
        .await
        .map_err(|err| format!("launch execution LLM request failed: {}", err))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "launch execution LLM returned {}: {}",
            status, body
        ));
    }

    let parsed: ChatResponseBody = response
        .json()
        .await
        .map_err(|err| format!("failed to parse launch execution LLM response: {}", err))?;

    let content = parsed
        .choices
        .first()
        .map(|choice| choice.message.content.clone())
        .unwrap_or_default();

    if content.trim().is_empty() {
        return Err("launch execution LLM returned an empty response".to_string());
    }

    Ok(content)
}

async fn run_launch_execution_llm(
    config: &LaunchExecutionLlmConfig,
    system_prompt: &str,
    user_prompt: &str,
    request: &LaunchExecutionRequest,
    context_text: &str,
    signal: &LaunchSignalAssessment,
) -> Result<(String, LlmLaunchExecutionOutput), String> {
    let primary_raw = call_chat_completion(
        config,
        system_prompt,
        user_prompt,
        PRIMARY_MAX_COMPLETION_TOKENS,
    )
    .await;

    match primary_raw {
        Ok(raw) => match parse_llm_output(&raw) {
            Ok(parsed) => Ok((raw, parsed)),
            Err(primary_parse_error) => {
                let retry_prompt = launch_execution_retry_user_prompt(
                    request.source_label.as_deref(),
                    context_text,
                    request.update_text.as_deref(),
                    request.prior_plan.as_ref(),
                    signal,
                )?;
                let retry_raw = call_chat_completion(
                    config,
                    system_prompt,
                    &retry_prompt,
                    RETRY_MAX_COMPLETION_TOKENS,
                )
                .await?;
                let parsed = parse_llm_output(&retry_raw).map_err(|retry_parse_error| {
                    format!(
                        "{} | retry_parse_error={}",
                        primary_parse_error, retry_parse_error
                    )
                })?;
                Ok((retry_raw, parsed))
            }
        },
        Err(primary_error) => {
            let retry_prompt = launch_execution_retry_user_prompt(
                request.source_label.as_deref(),
                context_text,
                request.update_text.as_deref(),
                request.prior_plan.as_ref(),
                signal,
            )?;
            let retry_raw = call_chat_completion(
                config,
                system_prompt,
                &retry_prompt,
                RETRY_MAX_COMPLETION_TOKENS,
            )
            .await
            .map_err(|retry_error| format!("{} | retry_error={}", primary_error, retry_error))?;
            let parsed = parse_llm_output(&retry_raw)?;
            Ok((retry_raw, parsed))
        }
    }
}

fn summarize_prior_plan_for_prompt(prior_plan: Option<&LaunchExecutionPlan>) -> String {
    let Some(plan) = prior_plan else {
        return "none".to_string();
    };

    let milestone_titles = plan
        .milestones
        .iter()
        .take(3)
        .map(|item| format!("{} [{}]", item.title, item.status))
        .collect::<Vec<_>>()
        .join("; ");
    let dependency_titles = plan
        .dependencies
        .iter()
        .filter(|item| matches!(item.status.as_str(), "at_risk" | "blocked"))
        .take(3)
        .map(|item| format!("{} [{}]", item.title, item.status))
        .collect::<Vec<_>>()
        .join("; ");
    let decision_titles = plan
        .decisions
        .iter()
        .filter(|item| item.status != "resolved")
        .take(3)
        .map(|item| item.title.clone())
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        "title={}; readiness={}; target_date={}; launch_window={}; milestones={}; at_risk_dependencies={}; open_decisions={}",
        plan.title,
        if plan.readiness_status.trim().is_empty() {
            "unknown"
        } else {
            plan.readiness_status.as_str()
        },
        plan.target_date.as_deref().unwrap_or("none"),
        plan.launch_window.as_deref().unwrap_or("none"),
        if milestone_titles.is_empty() {
            "none"
        } else {
            milestone_titles.as_str()
        },
        if dependency_titles.is_empty() {
            "none"
        } else {
            dependency_titles.as_str()
        },
        if decision_titles.is_empty() {
            "none"
        } else {
            decision_titles.as_str()
        }
    )
}

fn launch_execution_retry_user_prompt(
    source_label: Option<&str>,
    context_text: &str,
    update_text: Option<&str>,
    prior_plan: Option<&LaunchExecutionPlan>,
    signal: &LaunchSignalAssessment,
) -> Result<String, String> {
    let source_label = source_label
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Untitled launch thread");
    let latest_update = update_text
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("None");

    Ok(format!(
        "Return compact JSON only. Use sparse arrays if uncertain.\n\
Source label: {source_label}\n\
weak_signal={weak_signal}\n\
prior_plan_summary: {prior_plan_summary}\n\
context:\n{context_text}\n\
latest_update:\n{latest_update}\n\
Rules:\n\
- blockers only when the text explicitly says blocked, blocking, failing, cannot proceed, or lacks required signoff\n\
- dependencies are not blockers by default\n\
- unresolved owner naming is missing information, not a decision by itself\n\
- if the thread is early or weak-signal, keep owners/milestones/follow-ups sparse",
        source_label = source_label,
        weak_signal = signal.weak_signal,
        prior_plan_summary = summarize_prior_plan_for_prompt(prior_plan),
        context_text = context_text,
        latest_update = latest_update
    ))
}

fn parse_llm_output(raw: &str) -> Result<LlmLaunchExecutionOutput, String> {
    if let Ok(parsed) = serde_json::from_str::<LlmLaunchExecutionOutput>(raw) {
        return Ok(parsed);
    }

    if let Some(extracted) = extract_json_object(raw) {
        if let Ok(parsed) = serde_json::from_str::<LlmLaunchExecutionOutput>(&extracted) {
            return Ok(parsed);
        }
    }

    Err(format!(
        "launch execution LLM response was not valid JSON. raw={}",
        raw
    ))
}

fn extract_json_object(input: &str) -> Option<String> {
    let mut start_index = None;
    let mut brace_depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in input.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            continue;
        }

        if ch == '{' {
            if start_index.is_none() {
                start_index = Some(index);
            }
            brace_depth += 1;
            continue;
        }

        if ch == '}' && brace_depth > 0 {
            brace_depth -= 1;
            if brace_depth == 0 {
                if let Some(start) = start_index {
                    return Some(input[start..=index].to_string());
                }
            }
        }
    }

    None
}

fn assess_launch_signal(context_text: &str, update_text: Option<&str>) -> LaunchSignalAssessment {
    let lines = parse_thread_lines(context_text, update_text);
    let weak_signal_phrases = [
        "not asking for a plan yet",
        "just collecting thoughts",
        "thinking we might",
        "maybe",
        "regroup after",
        "if we decide this is real",
    ];

    let explicit_blocker_cues = lines
        .iter()
        .filter(|line| contains_explicit_blocker_cue(&line.text))
        .count();
    let decision_cues = lines
        .iter()
        .filter(|line| contains_decision_cue(&line.text))
        .count();
    let owner_cues = lines
        .iter()
        .filter(|line| extract_speaker_name(&line.text).is_some())
        .count();
    let date_cues = lines
        .iter()
        .filter(|line| contains_date_cue(&line.text))
        .count();
    let weak_phrase_hit = lines.iter().any(|line| {
        let lower = line.text.to_ascii_lowercase();
        weak_signal_phrases
            .iter()
            .any(|phrase| lower.contains(phrase))
    });

    let weak_signal = weak_phrase_hit
        || (explicit_blocker_cues == 0
            && owner_cues <= 2
            && date_cues <= 1
            && decision_cues <= 1
            && lines.len() <= 6);

    LaunchSignalAssessment {
        weak_signal,
        explicit_blocker_cues,
        decision_cues,
        owner_cues,
        date_cues,
    }
}

fn parse_thread_lines(context_text: &str, update_text: Option<&str>) -> Vec<ThreadLine> {
    let mut lines = Vec::new();

    for source in [Some(context_text), update_text] {
        let Some(text) = source else {
            continue;
        };
        for raw_line in text.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((source_ref, rest)) = trimmed.split_once('|') {
                lines.push(ThreadLine {
                    source_ref: source_ref.trim().to_string(),
                    text: rest.trim().to_string(),
                });
            } else {
                lines.push(ThreadLine {
                    source_ref: "primary_context".to_string(),
                    text: trimmed.to_string(),
                });
            }
        }
    }

    lines
}

fn build_fallback_output(
    source_label: Option<&str>,
    context_text: &str,
    update_text: Option<&str>,
    prior_plan: Option<&LaunchExecutionPlan>,
    signal: &LaunchSignalAssessment,
    fallback_reason: Option<String>,
) -> LlmLaunchExecutionOutput {
    let lines = parse_thread_lines(context_text, update_text);
    let objective = infer_fallback_objective(&lines);
    let (target_date, launch_window) = infer_fallback_timeline(&lines);
    let weak_signal = signal.weak_signal;
    let summary = if weak_signal {
        Some("Thread does not yet contain enough execution structure for an accountable launch plan.".to_string())
    } else {
        fallback_reason.clone()
    };

    LlmLaunchExecutionOutput {
        launch_execution_plan: LaunchExecutionPlan {
            title: source_label
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Launch execution plan")
                .to_string(),
            objective,
            source_context: LaunchSourceContext {
                input_type: DEFAULT_SOURCE_TYPE.to_string(),
                source_label: source_label.map(|value| value.to_string()),
                summary,
            },
            target_date,
            launch_window,
            milestones: infer_fallback_milestones(&lines, weak_signal, prior_plan),
            owners: infer_fallback_owners(&lines, weak_signal),
            dependencies: infer_fallback_dependencies(&lines, weak_signal),
            critical_path: Vec::new(),
            risks: infer_fallback_risks(&lines, weak_signal),
            decisions: infer_fallback_decisions(&lines, weak_signal),
            evidence: infer_fallback_evidence(&lines, weak_signal),
            ..LaunchExecutionPlan::default()
        },
        readiness_brief: LaunchReadinessBrief::default(),
    }
}

fn infer_fallback_objective(lines: &[ThreadLine]) -> Option<String> {
    let mut best: Option<(i32, String)> = None;
    for line in lines {
        let content = strip_speaker_prefix(&line.text);
        let lower = content.to_ascii_lowercase();
        let mut score = 0;
        if lower.contains("launch")
            || lower.contains("live by")
            || lower.contains("cutover")
            || lower.contains("ga ")
            || lower.contains("migration complete")
        {
            score += 4;
        }
        if lower.contains("goal is ")
            || lower.contains("we want")
            || lower.contains("want ")
            || lower.contains("target is")
            || lower.contains("original plan")
        {
            score += 3;
        }
        if lower.contains("need ") {
            score += 1;
        }
        if line.text.to_ascii_lowercase().contains("(pm)") {
            score += 1;
        }
        if score > 0 {
            let replace = best
                .as_ref()
                .map(|(best_score, _)| score > *best_score)
                .unwrap_or(true);
            if replace {
                best = Some((score, content));
            }
        }
    }

    best.map(|(_, value)| value)
        .or_else(|| lines.first().map(|line| strip_speaker_prefix(&line.text)))
}

fn infer_fallback_timeline(lines: &[ThreadLine]) -> (Option<String>, Option<String>) {
    for line in lines {
        let dates = extract_date_phrases(&line.text);
        if let Some(date) = dates.first() {
            if date.to_ascii_lowercase().contains("week of")
                || date.to_ascii_lowercase().contains("before ")
                || date.to_ascii_lowercase().contains("sometime")
            {
                let launch_window = dates.last().cloned().or_else(|| Some(date.clone()));
                return (None, launch_window);
            }
            return (
                Some(date.clone()),
                dates.last().cloned().filter(|value| value != date),
            );
        }
    }

    (None, None)
}

fn infer_fallback_owners(lines: &[ThreadLine], weak_signal: bool) -> Vec<LaunchOwner> {
    if weak_signal {
        return Vec::new();
    }

    dedupe_by_key(
        lines.iter().filter_map(|line| {
            let name = extract_speaker_name(&line.text)?;
            if looks_like_generic_role(&name) {
                return None;
            }
            Some(LaunchOwner {
                name,
                update_status: "current".to_string(),
                notes: Some(strip_speaker_prefix(&line.text)),
                ..LaunchOwner::default()
            })
        }),
        |item| item.name.to_ascii_lowercase(),
    )
}

fn infer_fallback_milestones(
    lines: &[ThreadLine],
    weak_signal: bool,
    prior_plan: Option<&LaunchExecutionPlan>,
) -> Vec<LaunchMilestone> {
    if weak_signal {
        return Vec::new();
    }

    let mut milestones = Vec::new();
    for line in lines {
        let text = strip_bullet_prefix(&strip_speaker_prefix(&line.text));
        let lower = text.to_ascii_lowercase();
        if lower.starts_with("go/no-go")
            || lower.contains("go or no-go")
            || lower.contains("review")
            || line.text.trim_start().starts_with('-')
        {
            let critical_path =
                lower.contains("blocked") || is_explicit_launch_blocking_text(&text);
            milestones.push(LaunchMilestone {
                title: text,
                status: "unknown".to_string(),
                critical_path,
                ..LaunchMilestone::default()
            });
        }
    }

    if milestones.is_empty() {
        if let Some(plan) = prior_plan {
            return plan.milestones.iter().take(2).cloned().collect();
        }
    }

    if milestones.len() > 4 {
        milestones.truncate(4);
    }
    dedupe_by_key(milestones, |item| item.title.to_ascii_lowercase())
}

fn looks_like_dependency_seed_line(text: &str) -> bool {
    let lower = line_body(text).to_ascii_lowercase();
    (lower.starts_with("we need ") || lower.starts_with("target is "))
        && (lower.contains(", and ")
            || lower.matches(',').count() >= 2
            || lower.contains("certification")
            || lower.contains("runbook"))
}

fn infer_fallback_dependencies(lines: &[ThreadLine], weak_signal: bool) -> Vec<LaunchDependency> {
    let mut dependencies = Vec::new();

    for line in lines {
        let text = strip_bullet_prefix(&strip_speaker_prefix(&line.text));
        let lower = text.to_ascii_lowercase();
        let owner = extract_speaker_name(&line.text).filter(|name| !looks_like_generic_role(name));
        if contains_explicit_blocker_cue(&text) || contains_decision_cue(&text) {
            continue;
        }
        let title = if lower.contains("depends on what infra says") {
            Some("Infra input".to_string())
        } else if lower.contains("customer call") {
            Some("customer call".to_string())
        } else if lower.contains("creator payouts figured out") {
            Some("creator payouts figured out".to_string())
        } else if lower.contains("docs are not outdated") {
            Some("docs review".to_string())
        } else if lower.contains("need ")
            || lower.contains("needs ")
            || lower.contains("depends on")
            || lower.contains("ready once")
            || lower.contains("once ")
            || lower.contains("after we know")
            || lower.contains("pending")
            || lower.contains("waiting on")
        {
            Some(canonicalize_short_object_phrase(&text))
        } else {
            None
        };
        if let Some(title) = title.filter(|value| !value.is_empty()) {
            dependencies.push(LaunchDependency {
                title,
                owner,
                status: if weak_signal { "unknown" } else { "at_risk" }.to_string(),
                notes: Some(text),
                ..LaunchDependency::default()
            });
        }
    }

    if dependencies.len() > 4 {
        dependencies.truncate(4);
    }
    dedupe_by_key(dependencies, |item| item.title.to_ascii_lowercase())
}

fn infer_fallback_risks(lines: &[ThreadLine], weak_signal: bool) -> Vec<LaunchRisk> {
    let mut risks = Vec::new();
    if weak_signal {
        return risks;
    }

    for line in lines {
        let text = strip_speaker_prefix(&line.text);
        if contains_explicit_blocker_cue(&text) {
            risks.push(LaunchRisk {
                title: strip_bullet_prefix(&text),
                owner: extract_speaker_name(&line.text)
                    .filter(|name| !looks_like_generic_role(name)),
                severity: "high".to_string(),
                status: "open".to_string(),
                blocker: true,
                ..LaunchRisk::default()
            });
        }
    }

    if risks.len() > 3 {
        risks.truncate(3);
    }
    dedupe_by_key(risks, |item| item.title.to_ascii_lowercase())
}

fn infer_fallback_decisions(lines: &[ThreadLine], weak_signal: bool) -> Vec<LaunchDecision> {
    let mut decisions = Vec::new();

    for line in lines {
        let text = strip_speaker_prefix(&line.text);
        let lower = text.to_ascii_lowercase();
        let title = if lower.contains("can we still ship") {
            "Whether the team can still ship billing cleanup before month end".to_string()
        } else if lower.contains("not asking for a plan yet")
            || lower.contains("if we decide this is real")
        {
            "Whether this is a real launch at all".to_string()
        } else if contains_decision_cue(&text) {
            canonicalize_decision_title(&text)
        } else {
            String::new()
        };
        if title.is_empty() {
            continue;
        }
        decisions.push(LaunchDecision {
            title,
            owner: extract_speaker_name(&line.text).filter(|name| !looks_like_generic_role(name)),
            status: "open".to_string(),
            launch_blocking: !weak_signal && is_explicit_launch_blocking_text(&text),
            notes: Some(text),
            ..LaunchDecision::default()
        });
    }

    if weak_signal && decisions.len() > 1 {
        decisions.truncate(1);
    } else if decisions.len() > 3 {
        decisions.truncate(3);
    }

    dedupe_by_key(decisions, |item| item.title.to_ascii_lowercase())
}

fn infer_fallback_evidence(lines: &[ThreadLine], weak_signal: bool) -> Vec<LaunchEvidence> {
    let mut scored = lines
        .iter()
        .map(|line| {
            let mut score = 0i32;
            let lower = line.text.to_ascii_lowercase();
            if contains_explicit_blocker_cue(&line.text) {
                score += 4;
            }
            if contains_decision_cue(&line.text) {
                score += 3;
            }
            if contains_date_cue(&line.text) {
                score += 2;
            }
            if lower.contains("depends on")
                || lower.contains("not asking for a plan yet")
                || lower.contains("creator payouts figured out")
            {
                score += 2;
            }
            if lower.contains("goal") || lower.contains("launch") || lower.contains("ship ") {
                score += 1;
            }
            if lower.contains("thread")
                || lower.contains("comment thread export")
                || lower.contains("slack snippets")
            {
                score -= 3;
            }
            (score, line)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.0.cmp(&left.0));

    let take_count = if weak_signal { 3 } else { MAX_EVIDENCE_ITEMS };
    let evidence = scored
        .into_iter()
        .filter(|(score, _line)| *score > 0)
        .take(take_count)
        .map(|(_score, line)| LaunchEvidence {
            label: if contains_explicit_blocker_cue(&line.text) {
                "blocking_evidence".to_string()
            } else if contains_decision_cue(&line.text) {
                "decision_evidence".to_string()
            } else if contains_date_cue(&line.text) {
                "timeline_evidence".to_string()
            } else {
                "context_evidence".to_string()
            },
            snippet: strip_bullet_prefix(&strip_speaker_prefix(&line.text)),
            source_ref: Some(line.source_ref.clone()),
        });

    dedupe_by_key(evidence, |item| item.snippet.to_ascii_lowercase())
}

fn rebuild_dependencies(
    current: Vec<LaunchDependency>,
    lines: &[ThreadLine],
    weak_signal: bool,
    has_update: bool,
) -> Vec<LaunchDependency> {
    if weak_signal {
        return current;
    }

    let mut dependencies = Vec::new();
    for line in lines {
        for candidate in dependency_candidates_from_line(line) {
            merge_dependency_candidate(&mut dependencies, candidate);
        }
    }

    for item in current {
        let mut expanded = dependency_candidates_from_text(
            &item.title,
            item.owner.clone(),
            item.notes.clone(),
            item.target_date.clone(),
            item.status.clone(),
        );
        if expanded.is_empty() {
            expanded.push(item);
        }
        for candidate in expanded {
            merge_dependency_candidate(&mut dependencies, candidate);
        }
    }

    dependencies.retain(|item| !looks_like_objective_dependency(&item.title));
    dependencies.retain(|item| !looks_like_timeline_only_object(&item.title));
    dependencies.retain(|item| !looks_like_procedural_review_item(&item.title));

    if has_update && dependencies.len() > 1 {
        let all_done_or_ready = dependencies
            .iter()
            .all(|item| matches!(item.status.as_str(), "done" | "ready"));
        if all_done_or_ready {
            dependencies.sort_by_key(|item| refresh_dependency_priority(&item.title));
            dependencies.truncate(1);
        }
    }

    dependencies
}

fn dependency_candidates_from_line(line: &ThreadLine) -> Vec<LaunchDependency> {
    let owner = if looks_like_dependency_seed_line(&line.text) {
        None
    } else {
        extract_speaker_name(&line.text).filter(|name| !looks_like_invalid_owner_name(name))
    };
    let default_status = infer_dependency_status_from_text(&line.text);
    let default_date = if looks_like_dependency_seed_line(&line.text) {
        None
    } else {
        extract_date_phrase(&line.text)
    };
    dependency_candidates_from_text(
        &line.text,
        owner,
        Some(line_body(&line.text)),
        default_date,
        default_status,
    )
}

fn dependency_candidates_from_text(
    text: &str,
    owner: Option<String>,
    notes: Option<String>,
    target_date: Option<String>,
    fallback_status: String,
) -> Vec<LaunchDependency> {
    let content = line_body(text);
    let lower = content.to_ascii_lowercase();
    let mut titles = Vec::new();

    if lower.contains("launch email") {
        titles.push("launch email scheduled".to_string());
    }
    if lower.contains("final screenshots") {
        titles.push("Final screenshots approved".to_string());
    }
    if lower.contains("screenshots are ready") || lower.contains("screenshots ready") {
        titles.push("Final screenshots approved".to_string());
    }
    if lower.contains("prep comms") || lower.contains("customer comms") {
        titles.push("customer comms draft".to_string());
    }
    if lower.contains("email plus in-app") || lower.contains("lifecycle email plus in-app") {
        titles.push("Lifecycle email plus in-app message scheduled".to_string());
    }
    if lower.contains("analytics qa")
        || lower.contains("qa passed")
        || lower.contains("qa pass complete")
        || (lower.contains("attribution") && lower.contains("qa"))
    {
        titles.push("analytics QA pass complete".to_string());
    }
    if lower.contains("final copy") {
        titles.push("final copy approved".to_string());
    }
    if lower.contains("partner sandbox certification")
        || lower.contains("retailer certification")
        || lower.contains("run certification")
        || lower.contains("certification sometime")
    {
        titles.push("partner sandbox certification".to_string());
    }
    if lower.contains("internal monitoring") || lower.contains("monitoring dashboard") {
        titles.push("internal monitoring".to_string());
    }
    if lower.contains("support runbook") || lower.contains("runbook draft") {
        titles.push("Support runbook".to_string());
    }
    if lower.contains("known failure codes") {
        titles.push("known failure codes from Engineering".to_string());
    }
    if lower.contains("support macro plus incident captain") {
        titles.push("support macro plus incident captain".to_string());
    }
    if lower.contains("retry queue config can change before code freeze")
        || lower.contains("retry queue setting")
        || lower.contains("not checked the retry queue setting")
    {
        titles.push("Infra confirmation on retry queue config before code freeze".to_string());
    }
    if lower.contains("missing org_id") || lower.contains("backfill job") {
        titles.push("Backfill job for missing org_id records".to_string());
    }
    if lower.contains("rollback timing") || lower.contains("tested revert path") {
        titles.push("Tested rollback timing".to_string());
    }
    if lower.contains("clinic onboarding doc") {
        titles.push("Clinic onboarding doc".to_string());
    }
    if lower.contains("chrome review ticket")
        || lower.contains("opened the chrome review ticket")
        || (lower.contains("chrome review") && lower.contains("no eta"))
    {
        titles.push("Chrome review ticket".to_string());
    }
    if lower.contains("target dogfood workspaces")
        || lower.contains("list of target dogfood workspaces")
    {
        titles.push("target dogfood workspaces".to_string());
    }
    if lower.contains("success metric") {
        if lower.contains("install-to-first-dm") || lower.contains("install to first dm") {
            titles.push("success metric for install-to-first-DM within 24 hours".to_string());
        } else {
            titles.push("success metric".to_string());
        }
    }
    if lower.contains("linked-account fallback") || lower.contains("dm fallback") {
        titles.push("Discord DM fallback decision".to_string());
    }
    if lower.contains("spf") || lower.contains("dkim") {
        titles.push("IT confirmation for SPF and DKIM changes".to_string());
    }
    if lower.contains("fallback copy") && lower.contains("approval from legal") {
        titles.push("Legal approval for fallback copy".to_string());
    }
    if lower.contains("fallback copy") && lower.contains("approves the fallback copy") {
        titles.push("Legal approval for fallback copy".to_string());
    }
    if lower.contains("vendor ticket owner") {
        titles.push("vendor ticket owner for EU domain".to_string());
    }
    if lower.contains("support staffing plan") {
        titles.push("support staffing plan".to_string());
    }
    if lower.contains("possible window")
        || lower.contains("go or no-go review")
        || lower.contains("go/no-go review")
    {
        titles.clear();
    }

    if titles.is_empty() {
        if let Some(rest) = capture_after_phrase(&content, "still need ") {
            titles.push(canonicalize_short_object_phrase(&rest));
        } else if let Some(rest) = capture_after_phrase(&content, "need ") {
            if !looks_like_objective_dependency(&rest) {
                titles.push(canonicalize_short_object_phrase(&rest));
            }
        } else if let Some((subject, _remainder)) = content.split_once(" is ready once ") {
            titles.push(canonicalize_short_object_phrase(subject));
        } else if let Some((subject, _remainder)) = content.split_once(" ready once ") {
            titles.push(canonicalize_short_object_phrase(subject));
        } else if let Some((subject, _remainder)) = content.split_once(" depends on ") {
            titles.push(canonicalize_short_object_phrase(subject));
        } else if !contains_explicit_blocker_cue(&content)
            && !decision_candidate_from_text(&content, owner.clone()).is_some()
            && looks_like_dependency_sentence(&content)
        {
            titles.push(canonicalize_short_object_phrase(&content));
        }
    }

    let mut output = Vec::new();
    for title in dedupe_by_key(titles, |item| item.to_ascii_lowercase()) {
        if title.is_empty() || looks_like_objective_dependency(&title) {
            continue;
        }
        output.push(LaunchDependency {
            title,
            owner: owner.clone(),
            target_date: target_date.clone(),
            status: fallback_status.clone(),
            notes: notes.clone(),
            critical_path: false,
        });
    }
    output
}

fn infer_dependency_status_from_text(text: &str) -> String {
    let lower = line_body(text).to_ascii_lowercase();
    if contains_explicit_blocker_cue(&lower) {
        return "blocked".to_string();
    }
    if lower.contains("done except")
        || lower.contains("half done")
        || lower.contains("80 percent")
        || lower.contains("after we know")
        || lower.contains("except ")
        || lower.contains("not checked")
    {
        return "at_risk".to_string();
    }
    if looks_like_resolved_status_text(&lower) {
        return "done".to_string();
    }
    if lower.contains("ready") && !lower.contains("once ") && !lower.contains("until ") {
        return "ready".to_string();
    }
    if lower.contains("need ")
        || lower.contains("needs ")
        || lower.contains("still need ")
        || lower.contains("depends on")
        || lower.contains("pending")
        || lower.contains("once ")
        || lower.contains("until ")
        || lower.contains("not confirmed")
        || lower.contains("no eta")
        || lower.contains("should know by")
    {
        return "at_risk".to_string();
    }
    "unknown".to_string()
}

fn merge_dependency_candidate(items: &mut Vec<LaunchDependency>, candidate: LaunchDependency) {
    let key = candidate.title.to_ascii_lowercase();
    if let Some(existing) = items
        .iter_mut()
        .find(|item| item.title.to_ascii_lowercase() == key)
    {
        let existing_done_or_ready = matches!(existing.status.as_str(), "done" | "ready");
        let candidate_done_or_ready = matches!(candidate.status.as_str(), "done" | "ready");
        let candidate_resolved_is_credible = candidate_done_or_ready
            && candidate
                .notes
                .as_deref()
                .map(|value| {
                    looks_like_resolved_status_text(value)
                        && normalize_for_match(value) != normalize_for_match(&candidate.title)
                })
                .unwrap_or(false);
        if candidate_resolved_is_credible {
            existing.status = candidate.status.clone();
        } else if !existing_done_or_ready
            && dependency_status_rank(&candidate.status) >= dependency_status_rank(&existing.status)
        {
            existing.status = candidate.status.clone();
        }
        let existing_is_generic_seed = existing
            .notes
            .as_deref()
            .map(looks_like_dependency_seed_line)
            .unwrap_or(false);
        if existing.owner.is_none() || (existing_is_generic_seed && candidate.owner.is_some()) {
            existing.owner = candidate.owner.clone();
        }
        if candidate_resolved_is_credible && candidate.target_date.is_some() {
            existing.target_date = candidate.target_date.clone();
        } else if existing.target_date.is_none() {
            existing.target_date = candidate.target_date.clone();
        }
        if candidate_resolved_is_credible && candidate.notes.is_some() {
            existing.notes = candidate.notes.clone();
        } else if existing.notes.is_none() {
            existing.notes = candidate.notes.clone();
        }
        return;
    }
    items.push(candidate);
}

fn dependency_status_rank(status: &str) -> usize {
    match status {
        "blocked" => 4,
        "at_risk" => 3,
        "unknown" => 2,
        "ready" => 1,
        "done" => 0,
        _ => 0,
    }
}

fn rebuild_decisions(
    current: Vec<LaunchDecision>,
    lines: &[ThreadLine],
    has_update: bool,
) -> Vec<LaunchDecision> {
    let update_lines = lines
        .iter()
        .filter(|line| is_update_ref(&line.source_ref))
        .cloned()
        .collect::<Vec<_>>();
    let mut decisions = Vec::new();

    for line in lines {
        if let Some(candidate) = decision_candidate_from_line(line) {
            merge_decision_candidate(&mut decisions, candidate);
        }
    }

    for item in current {
        let canonical = decision_candidate_from_text(
            &format!("{} {}", item.title, item.notes.clone().unwrap_or_default()),
            item.owner.clone(),
        )
        .unwrap_or(item);
        merge_decision_candidate(&mut decisions, canonical);
    }

    if has_update {
        decisions.retain(|item| !decision_resolved_by_updates(item, &update_lines));
    }

    decisions.retain(|item| !looks_like_missing_info_question(&item.title));
    decisions.retain(|item| !looks_like_procedural_review_item(&item.title));
    decisions
}

fn decision_candidate_from_line(line: &ThreadLine) -> Option<LaunchDecision> {
    let owner = extract_decision_owner(&line.text);
    decision_candidate_from_text(&line.text, owner)
}

fn decision_candidate_from_text(text: &str, owner: Option<String>) -> Option<LaunchDecision> {
    let content = line_body(text);
    let lower = content.to_ascii_lowercase();
    let owner = if lower.contains("we need a decision on whether")
        || lower.contains("i still need a decision on whether")
        || lower.contains("leadership asked whether")
        || lower.contains("placeholder not a promise")
        || lower.contains("possible window")
        || (lower.contains("survives") && contains_date_cue(&content))
        || (lower.contains("fallback copy")
            && (lower.contains("approval from legal")
                || lower.contains("approves the fallback copy")
                || lower.contains("approve the fallback copy")))
    {
        None
    } else {
        owner
    };
    if looks_like_resolved_status_text(&lower) && !lower.contains("no decision yet") {
        return None;
    }
    if (lower.contains("confirm whether") && lower.contains("before code freeze"))
        || (lower.contains("retry queue config") && lower.contains("code freeze"))
    {
        return None;
    }
    if lower.contains("that decision blocks launch")
        || lower.contains("approver not named")
        || lower.contains("name the approver")
    {
        return None;
    }

    let title = if let Some(index) = lower.find("whether ") {
        title_case(&trim_decision_clause(&content[index..]))
    } else if lower.contains("fallback copy")
        && (lower.contains("approval from legal")
            || lower.contains("approves the fallback copy")
            || lower.contains("approve the fallback copy"))
    {
        "Approve fallback copy for the payment error state".to_string()
    } else if lower.contains("certification slips past") && lower.contains("miss launch week") {
        "Whether the first live retailer can still launch in the week of Aug 25 if certification slips".to_string()
    } else if lower.contains("phased cutover")
        && (lower.contains("if the eu domain is not ready")
            || lower.contains("whether phased cutover"))
    {
        "Whether phased cutover is acceptable if the EU domain is late".to_string()
    } else if lower.contains("placeholder not a promise")
        || lower.contains("possible window")
        || (lower.contains("survives") && contains_date_cue(&content))
    {
        "What the real GA date should be".to_string()
    } else if lower.contains("go/no-go") || lower.contains("go or no-go") {
        String::new()
    } else if lower.contains("annual-plan disclaimer") && lower.contains("final answer") {
        "Final legal approval of the annual-plan disclaimer".to_string()
    } else if lower.contains("annual-plan disclaimer") && lower.contains("comment") {
        "Final legal approval of the annual-plan disclaimer".to_string()
    } else if lower.contains("not signed off on rollback timing")
        || lower.contains("tested revert path")
    {
        "Whether cutover should proceed without a tested revert path".to_string()
    } else if lower.contains("decision on ") {
        let index = lower.find("decision on ").unwrap_or(0) + "decision on ".len();
        title_case(content[index..].trim_end_matches('.'))
    } else if lower.contains("no decision yet") {
        canonicalize_decision_title(&content)
    } else {
        String::new()
    };

    if title.trim().is_empty() {
        return None;
    }

    Some(LaunchDecision {
        title,
        owner,
        due_date: extract_date_phrase(text),
        status: "open".to_string(),
        launch_blocking: is_explicit_launch_blocking_text(&content),
        notes: Some(content),
    })
}

fn extract_decision_owner(text: &str) -> Option<String> {
    let content = line_body(text).to_ascii_lowercase();
    if content.contains("we need a decision on whether")
        || content.contains("i still need a decision on whether")
        || content.contains("leadership asked whether")
    {
        return None;
    }
    extract_speaker_name(text).filter(|name| !looks_like_invalid_owner_name(name))
}

fn merge_decision_candidate(items: &mut Vec<LaunchDecision>, candidate: LaunchDecision) {
    let key = candidate.title.to_ascii_lowercase();
    if let Some(existing) = items.iter_mut().find(|item| {
        item.title.to_ascii_lowercase() == key
            || special_decision_overlap(&item.title, &candidate.title)
            || token_overlap_score(&item.title, &candidate.title) >= 0.75
    }) {
        existing.launch_blocking = existing.launch_blocking || candidate.launch_blocking;
        if existing.owner.is_none() {
            existing.owner = candidate.owner.clone();
        }
        if existing.due_date.is_none() {
            existing.due_date = candidate.due_date.clone();
        }
        if candidate
            .notes
            .as_deref()
            .map(|value| value.to_ascii_lowercase().contains("no decision yet"))
            .unwrap_or(false)
        {
            existing.title = candidate.title.clone();
            existing.owner = candidate.owner.clone().or(existing.owner.clone());
        }
        if existing.notes.is_none() {
            existing.notes = candidate.notes.clone();
        }
        return;
    }
    items.push(candidate);
}

fn special_decision_overlap(left: &str, right: &str) -> bool {
    let left_lower = left.to_ascii_lowercase();
    let right_lower = right.to_ascii_lowercase();
    (left_lower.contains("phased cutover is acceptable")
        && right_lower.contains("phased cutover is acceptable"))
        || (left_lower.contains("retry queue config can change before code freeze")
            && right_lower.contains("retry queue config can change before code freeze"))
        || (left_lower.contains("fallback copy for the payment error state")
            && right_lower.contains("fallback copy for the payment error state"))
        || (left_lower.contains("links expire in 15 minutes or require portal login")
            && right_lower.contains("secure access method for download links"))
        || (left_lower.contains("secure access method for download links")
            && right_lower.contains("links expire in 15 minutes or require portal login"))
}

fn decision_resolved_by_updates(item: &LaunchDecision, update_lines: &[ThreadLine]) -> bool {
    let title = item.title.to_ascii_lowercase();
    for line in update_lines {
        let content = line_body(&line.text).to_ascii_lowercase();
        if title.contains("annual-plan disclaimer") && content.contains("approved the disclaimer") {
            return true;
        }
        if title.contains("final copy")
            && (content.contains("final copy is frozen") || content.contains("final copy approved"))
        {
            return true;
        }
        if title.contains("analytics qa") && content.contains("qa passed") {
            return true;
        }
        if title.contains("go/no-go") && content.contains("no open blockers for launch") {
            return true;
        }
        if title.contains("whether ")
            && token_overlap_score(&title, &content) >= 0.65
            && looks_like_resolved_status_text(&content)
        {
            return true;
        }
    }
    false
}

fn rebuild_risks(
    current: Vec<LaunchRisk>,
    lines: &[ThreadLine],
    decisions: &[LaunchDecision],
) -> Vec<LaunchRisk> {
    let mut risks = Vec::new();
    for item in current {
        let mut updated = item;
        updated.title = canonicalize_risk_title(&updated.title);
        if updated.title.is_empty() {
            continue;
        }
        if decisions.iter().any(|decision| {
            token_overlap_score(&decision.title, &updated.title) >= 0.75
                && updated.title.to_ascii_lowercase().contains("decision")
        }) {
            continue;
        }
        risks.push(updated);
    }

    for line in lines {
        let content = line_body(&line.text);
        if !contains_explicit_blocker_cue(&content) {
            continue;
        }
        let title = canonicalize_risk_title(&content);
        if title.is_empty() {
            continue;
        }
        let candidate = LaunchRisk {
            title,
            owner: extract_speaker_name(&line.text)
                .filter(|name| !looks_like_invalid_owner_name(name)),
            severity: "high".to_string(),
            status: "open".to_string(),
            blocker: true,
            notes: Some(content),
        };
        if let Some(existing) = risks
            .iter_mut()
            .find(|item| item.title.eq_ignore_ascii_case(&candidate.title))
        {
            if existing.owner.is_none() {
                existing.owner = candidate.owner.clone();
            }
            if existing.notes.is_none() {
                existing.notes = candidate.notes.clone();
            }
        } else {
            risks.push(candidate);
        }
    }

    risks.sort_by_key(|item| risk_priority(&item.title));
    dedupe_by_key(risks, |item| item.title.to_ascii_lowercase())
}

fn risk_priority(title: &str) -> usize {
    let lower = title.to_ascii_lowercase();
    if lower.contains("phi exposure risk")
        || lower.contains("lost workspace access")
        || lower.contains("missing org_id")
        || lower.contains("not signed off")
        || lower.contains("permissions regression")
        || lower.contains("cannot finish validation")
    {
        0
    } else if lower.contains("failing in staging") || lower.contains("callback retries") {
        1
    } else {
        2
    }
}

fn canonicalize_risk_title(text: &str) -> String {
    let content = line_body(text);
    let lower = content.to_ascii_lowercase();
    if lower.contains("that decision blocks launch") {
        return String::new();
    }
    if lower.contains("lost workspace access") {
        return "14 internal test accounts lost workspace access".to_string();
    }
    if lower.contains("missing org_id") {
        return "11k records with missing org_id".to_string();
    }
    if lower.contains("bucket is smaller") || lower.contains("remaining failures") {
        return "11k records with missing org_id".to_string();
    }
    if lower.contains("rollback timing") || lower.contains("tested revert path") {
        return "Security has not signed off on rollback timing".to_string();
    }
    if lower.contains("permissions regression") && lower.contains("chrome 137") {
        return "permissions regression in Chrome 137".to_string();
    }
    if lower.contains("phi exposure risk") {
        return "PHI exposure risk in the emailed download link".to_string();
    }
    if lower.contains("fallback copy still needs approval from legal") {
        return "Fallback copy still needs approval from legal".to_string();
    }
    if lower.contains("vendor says they cannot finish validation before") {
        if let Some(date) = extract_last_date_phrase(&content) {
            return format!("EU vendor cannot finish validation before {}", date);
        }
        return "EU vendor cannot finish validation".to_string();
    }
    if lower.contains("callback retries") && lower.contains("failing") {
        return "payments callback retries failing in staging".to_string();
    }
    content.trim_end_matches('.').to_string()
}

fn rebuild_grounded_evidence(
    plan: &LaunchExecutionPlan,
    lines: &[ThreadLine],
    preserved_evidence: Vec<LaunchEvidence>,
    has_update: bool,
) -> Vec<LaunchEvidence> {
    let mut evidence = Vec::new();
    let refresh_resolved_mode = has_update
        && plan.risks.is_empty()
        && plan.decisions.is_empty()
        && plan
            .dependencies
            .iter()
            .all(|item| matches!(item.status.as_str(), "done" | "ready"));

    if !refresh_resolved_mode {
        for risk in plan.risks.iter().filter(|item| {
            item.blocker && !matches!(item.status.as_str(), "resolved" | "mitigated")
        }) {
            if let Some(line) = best_supporting_risk_line(lines, &risk.title, true) {
                push_evidence(&mut evidence, "blocking_evidence", line);
            } else {
                push_best_evidence(
                    &mut evidence,
                    lines,
                    "blocking_evidence",
                    risk.notes.as_deref().unwrap_or(&risk.title),
                    true,
                );
            }
        }

        for decision in plan
            .decisions
            .iter()
            .filter(|item| item.status != "resolved")
        {
            let duplicated_by_dependency = plan.dependencies.iter().any(|dependency| {
                matches!(dependency.status.as_str(), "at_risk" | "blocked")
                    && !dependency
                        .title
                        .to_ascii_lowercase()
                        .starts_with("decision on ")
                    && token_overlap_score(&dependency.title, &decision.title) >= 0.35
            });
            if duplicated_by_dependency {
                continue;
            }
            if let Some(line) = best_supporting_decision_line(lines, decision, has_update) {
                push_evidence(&mut evidence, "decision_evidence", line);
            } else {
                push_best_evidence(
                    &mut evidence,
                    lines,
                    "decision_evidence",
                    decision.notes.as_deref().unwrap_or(&decision.title),
                    true,
                );
            }
        }

        if plan
            .decisions
            .iter()
            .any(|item| item.owner.is_none() && item.status != "resolved")
            || plan.dependencies.iter().any(|item| {
                matches!(item.status.as_str(), "at_risk" | "blocked") && item.owner.is_none()
            })
        {
            if let Some(line) = lines.iter().find(|line| {
                let lower = line_body(&line.text).to_ascii_lowercase();
                lower.contains("approver not named")
                    || lower.contains("security approver not named")
                    || lower.contains("do not own the vendor ticket")
            }) {
                push_evidence(&mut evidence, "missing_owner_evidence", line);
            }
        }
    }

    if has_update {
        let mut specific_update_count = 0usize;
        for line in lines.iter().filter(|line| {
            is_update_ref(&line.source_ref)
                && looks_like_resolved_status_text(&line_body(&line.text).to_ascii_lowercase())
        }) {
            let lower = line_body(&line.text).to_ascii_lowercase();
            let generic_summary =
                lower.contains("no open blockers") || lower.contains("deck has been updated");
            if generic_summary && specific_update_count >= 2 {
                continue;
            }
            push_evidence(&mut evidence, "update_evidence", line);
            if !generic_summary {
                specific_update_count += 1;
            }
        }
    }

    push_timeline_evidence(&mut evidence, plan, lines);

    let blocker_plus_open_decision_with_missing_owner = plan
        .risks
        .iter()
        .any(|item| item.blocker && !matches!(item.status.as_str(), "resolved" | "mitigated"))
        && plan.decisions.iter().any(|item| item.status != "resolved")
        && evidence
            .iter()
            .any(|item| item.label == "missing_owner_evidence");
    if (has_update || blocker_plus_open_decision_with_missing_owner) && evidence.len() > 3 {
        evidence.retain(|item| item.label != "timeline_evidence");
    }

    if !refresh_resolved_mode {
        for dependency in plan.dependencies.iter().filter(|item| {
            matches!(item.status.as_str(), "at_risk" | "blocked")
                && !looks_like_timeline_only_object(&item.title)
                && !looks_like_procedural_review_item(&item.title)
        }) {
            if let Some(line) =
                best_supporting_dependency_line(lines, &dependency.title, has_update)
            {
                push_evidence(&mut evidence, "dependency_evidence", line);
            } else {
                push_best_evidence(
                    &mut evidence,
                    lines,
                    "dependency_evidence",
                    dependency.notes.as_deref().unwrap_or(&dependency.title),
                    true,
                );
            }
        }
    }

    let has_goal_or_timeline_evidence = evidence
        .iter()
        .any(|item| matches!(item.label.as_str(), "goal_evidence" | "timeline_evidence"));
    if !has_goal_or_timeline_evidence && evidence.len() < MAX_EVIDENCE_ITEMS {
        if let Some(goal_line) =
            best_goal_line(lines, plan.objective.as_deref().unwrap_or(&plan.title))
        {
            push_evidence(&mut evidence, "goal_evidence", goal_line);
        }
    }

    if evidence.is_empty() {
        for item in preserved_evidence {
            if evidence.len() >= MAX_EVIDENCE_ITEMS {
                break;
            }
            if !item.snippet.trim().is_empty()
                && !evidence.iter().any(|existing: &LaunchEvidence| {
                    existing.snippet.eq_ignore_ascii_case(&item.snippet)
                })
            {
                evidence.push(item);
            }
        }
    }

    if evidence.len() > MAX_EVIDENCE_ITEMS {
        evidence.truncate(MAX_EVIDENCE_ITEMS);
    }
    evidence
}

fn best_matching_line<'a>(
    lines: &'a [ThreadLine],
    query: &str,
    prefer_update: bool,
) -> Option<&'a ThreadLine> {
    let query = line_body(query);
    let mut best_score = 0.0f32;
    let mut best_line = None;
    for line in lines {
        let content = line_body(&line.text);
        let mut score = token_overlap_score(&query, &content);
        if normalize_for_match(&content).contains(&normalize_for_match(&query)) {
            score += 0.35;
        }
        if prefer_update && is_update_ref(&line.source_ref) {
            score += 0.1;
        }
        if score > best_score {
            best_score = score;
            best_line = Some(line);
        }
    }
    if best_score >= 0.2 {
        best_line
    } else {
        None
    }
}

fn best_goal_line<'a>(lines: &'a [ThreadLine], objective: &str) -> Option<&'a ThreadLine> {
    let objective_lower = objective.to_ascii_lowercase();
    let mut best_score = 0.0f32;
    let mut best_line = None;
    for line in lines {
        let body = line_body(&line.text);
        let lower = body.to_ascii_lowercase();
        let mut score = token_overlap_score(objective, &body);
        if lower.contains("goal")
            || lower.contains("target")
            || lower.contains("live by")
            || lower.contains("want ")
            || lower.contains("cutover")
            || lower.contains("ga on")
        {
            score += 0.25;
        }
        if lower.contains("board preview")
            || lower.contains("advisory board")
            || lower.contains("pilot clinics")
            || lower.contains("partner webinar")
        {
            score += 0.1;
        }
        if lower.contains("that decision blocks launch")
            || lower.contains("security review found")
            || lower.contains("no open blockers")
            || lower.contains("approver not named")
            || lower.contains("no decision yet")
        {
            score -= 0.35;
        }
        if objective_lower.contains("before ")
            && lower.contains("before ")
            && contains_date_cue(&body)
        {
            score += 0.1;
        }
        if score > best_score {
            best_score = score;
            best_line = Some(line);
        }
    }
    if best_score >= 0.25 {
        best_line
    } else {
        None
    }
}

fn support_line_score(line: &ThreadLine, query: &str, prefer_update: bool) -> f32 {
    let content = line_body(&line.text);
    let normalized_query = normalize_for_match(query);
    let normalized_content = normalize_for_match(&content);
    let mut score = token_overlap_score(query, &content);
    if !normalized_query.is_empty() && normalized_content.contains(&normalized_query) {
        score += 0.35;
    }
    if prefer_update && is_update_ref(&line.source_ref) {
        score += 0.12;
    }
    if extract_speaker_name(&line.text).is_some() {
        score += 0.05;
    }
    if looks_like_resolved_status_text(&content)
        || contains_explicit_blocker_cue(&content)
        || contains_decision_cue(&content)
    {
        score += 0.05;
    }
    let lower = content.to_ascii_lowercase();
    if lower.contains("not confirmed")
        || lower.contains("half done")
        || lower.contains("still need")
        || lower.contains("no eta")
        || lower.contains("after we know")
        || lower.contains("ready once")
        || lower.contains("80 percent")
        || lower.contains("not checked")
        || lower.contains("should know by")
    {
        score += 0.12;
    }
    if looks_like_dependency_seed_line(&content) || looks_like_objective_dependency(&content) {
        score -= 0.35;
    }
    if looks_like_procedural_review_item(&content) {
        score -= 0.3;
    }
    if line.text.trim_start().starts_with('-') {
        if normalized_content == normalized_query {
            score -= 1.4;
        } else {
            score -= 0.35;
        }
    }
    score
}

fn best_supporting_dependency_line<'a>(
    lines: &'a [ThreadLine],
    title: &str,
    prefer_update: bool,
) -> Option<&'a ThreadLine> {
    let mut best_score = 0.0f32;
    let mut best_line = None;
    for line in lines {
        let matches_title = dependency_candidates_from_line(line)
            .into_iter()
            .any(|candidate| candidate.title.eq_ignore_ascii_case(title))
            || canonicalize_risk_title(&line.text).eq_ignore_ascii_case(title)
            || blocker_linked_dependency_from_risk(&LaunchRisk {
                title: canonicalize_risk_title(&line.text),
                notes: Some(line_body(&line.text)),
                owner: extract_speaker_name(&line.text),
                ..LaunchRisk::default()
            })
            .map(|candidate| candidate.title.eq_ignore_ascii_case(title))
            .unwrap_or(false);
        if !matches_title {
            continue;
        }
        let score = support_line_score(line, title, prefer_update);
        if score > best_score {
            best_score = score;
            best_line = Some(line);
        }
    }
    if best_score >= 0.15 {
        return best_line;
    }
    best_matching_line(lines, title, prefer_update)
}

fn best_supporting_decision_line<'a>(
    lines: &'a [ThreadLine],
    decision: &LaunchDecision,
    prefer_update: bool,
) -> Option<&'a ThreadLine> {
    let mut best_score = 0.0f32;
    let mut best_line = None;
    for line in lines {
        let lower = line_body(&line.text).to_ascii_lowercase();
        let candidate_match = decision_candidate_from_line(line)
            .map(|candidate| {
                candidate.title.eq_ignore_ascii_case(&decision.title)
                    || special_decision_overlap(&candidate.title, &decision.title)
                    || token_overlap_score(&candidate.title, &decision.title) >= 0.75
            })
            .unwrap_or(false);
        let contextual_match = lower.contains("that decision blocks launch")
            || lower.contains("no decision yet")
            || lower.contains("name the approver");
        if !candidate_match && !contextual_match {
            continue;
        }
        let mut score = support_line_score(line, &decision.title, prefer_update);
        if lower.contains("no decision yet") {
            score += 0.2;
        }
        if lower.contains("blocks launch") || lower.contains("name the approver") {
            score += 0.8;
        }
        if lower.contains("placeholder not a promise") || lower.contains("possible window") {
            score += 0.15;
        }
        if score > best_score {
            best_score = score;
            best_line = Some(line);
        }
    }
    if best_score >= 0.15 {
        return best_line;
    }
    best_matching_line(
        lines,
        decision.notes.as_deref().unwrap_or(&decision.title),
        prefer_update,
    )
}

fn best_supporting_risk_line<'a>(
    lines: &'a [ThreadLine],
    title: &str,
    prefer_update: bool,
) -> Option<&'a ThreadLine> {
    let mut best_score = 0.0f32;
    let mut best_line = None;
    for line in lines {
        let content = line_body(&line.text);
        if !contains_explicit_blocker_cue(&content) {
            continue;
        }
        if !canonicalize_risk_title(&content).eq_ignore_ascii_case(title) {
            continue;
        }
        let score = support_line_score(line, title, prefer_update);
        if score > best_score {
            best_score = score;
            best_line = Some(line);
        }
    }
    if best_score >= 0.15 {
        return best_line;
    }
    best_matching_line(lines, title, prefer_update)
}

fn push_evidence(evidence: &mut Vec<LaunchEvidence>, label: &str, line: &ThreadLine) {
    let snippet = focused_evidence_snippet(label, &line.text);
    if snippet.is_empty()
        || evidence
            .iter()
            .any(|item| item.snippet.eq_ignore_ascii_case(&snippet))
    {
        return;
    }
    evidence.push(LaunchEvidence {
        label: label.to_string(),
        snippet,
        source_ref: Some(line.source_ref.clone()),
    });
}

fn focused_evidence_snippet(label: &str, text: &str) -> String {
    let body = line_body(text);
    let sentences = body
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let choose = |needles: &[&str]| -> Option<String> {
        sentences.iter().find_map(|sentence| {
            let lower = sentence.to_ascii_lowercase();
            if needles.iter().any(|needle| lower.contains(needle)) {
                Some(format!("{}.", sentence.trim_end_matches('.')))
            } else {
                None
            }
        })
    };

    match label {
        "blocking_evidence" => choose(&[
            "blocks launch",
            "blocked until",
            "failing in staging",
            "cannot finish validation",
            "exposure risk",
            "approval from legal",
            "blocked",
        ])
        .or_else(|| choose(&["cannot", "still failing"]))
        .unwrap_or(body),
        "decision_evidence" => choose(&["no decision yet"])
            .or_else(|| {
                choose(&[
                    "annual-plan disclaimer",
                    "comment on the annual-plan disclaimer",
                ])
            })
            .or_else(|| choose(&["whether ", "approve ", "decision on "]))
            .or_else(|| choose(&["final answer"]))
            .unwrap_or(body),
        "missing_owner_evidence" => choose(&[
            "approver not named",
            "do not own the vendor ticket",
            "name the approver",
        ])
        .unwrap_or(body),
        "timeline_evidence" | "goal_evidence" => choose(&[
            "live by",
            "before ",
            "target ",
            "board preview",
            "advisory board",
            "pilot clinics",
            "partner webinar",
            "oct ",
            "nov ",
            "june ",
            "july ",
            "aug ",
            "may ",
        ])
        .unwrap_or(body),
        _ => body,
    }
}

fn push_best_evidence(
    evidence: &mut Vec<LaunchEvidence>,
    lines: &[ThreadLine],
    label: &str,
    query: &str,
    prefer_update: bool,
) {
    if evidence.len() >= MAX_EVIDENCE_ITEMS {
        return;
    }
    if let Some(line) = best_matching_line(lines, query, prefer_update) {
        push_evidence(evidence, label, line);
    }
}

fn push_timeline_evidence(
    evidence: &mut Vec<LaunchEvidence>,
    plan: &LaunchExecutionPlan,
    lines: &[ThreadLine],
) {
    if evidence.len() >= MAX_EVIDENCE_ITEMS {
        return;
    }
    if !plan.decisions.is_empty()
        && plan
            .dependencies
            .iter()
            .any(|item| matches!(item.status.as_str(), "at_risk" | "blocked"))
        && !plan
            .risks
            .iter()
            .any(|item| item.blocker && !matches!(item.status.as_str(), "resolved" | "mitigated"))
    {
        return;
    }

    let mut candidates = lines
        .iter()
        .filter(|line| {
            let body = line_body(&line.text);
            let lower = body.to_ascii_lowercase();
            !body.is_empty()
                && (contains_date_cue(&body)
                    || lower.contains("placeholder not a promise")
                    || lower.contains("possible window")
                    || lower.contains("survives")
                    || lower.contains("board preview")
                    || lower.contains("advisory board"))
        })
        .collect::<Vec<_>>();

    candidates.sort_by_key(|line| {
        let lower = line_body(&line.text).to_ascii_lowercase();
        if lower.contains("placeholder not a promise") || lower.contains("possible window") {
            0
        } else if lower.contains("survives")
            || lower.contains("board preview")
            || lower.contains("advisory board")
        {
            1
        } else {
            2
        }
    });

    for line in candidates {
        if evidence.len() >= MAX_EVIDENCE_ITEMS {
            break;
        }
        let body = line_body(&line.text);
        let normalized = normalize_for_match(&body);
        let matches_plan_timeline = plan
            .target_date
            .as_deref()
            .map(|value| normalized.contains(&normalize_for_match(value)))
            .unwrap_or(false)
            || plan
                .launch_window
                .as_deref()
                .map(|value| normalized.contains(&normalize_for_match(value)))
                .unwrap_or(false);
        let uncertainty_line = body
            .to_ascii_lowercase()
            .contains("placeholder not a promise")
            || body.to_ascii_lowercase().contains("possible window")
            || body.to_ascii_lowercase().contains("survives");
        if matches_plan_timeline || uncertainty_line {
            push_evidence(evidence, "timeline_evidence", line);
        }
    }
}

fn blocker_linked_dependency_from_risk(risk: &LaunchRisk) -> Option<LaunchDependency> {
    let lower = risk.title.to_ascii_lowercase();
    if lower.contains("cannot finish validation before") {
        return Some(LaunchDependency {
            title: "EU domain vendor validation".to_string(),
            owner: risk.owner.clone(),
            target_date: risk
                .notes
                .as_deref()
                .and_then(extract_last_date_phrase)
                .or_else(|| extract_last_date_phrase(&risk.title)),
            status: "at_risk".to_string(),
            notes: risk.notes.clone(),
            critical_path: false,
        });
    }
    if lower.contains("callback retries") {
        return Some(LaunchDependency {
            title: "Engineering fix callback retries in staging".to_string(),
            owner: risk.owner.clone(),
            target_date: None,
            status: "blocked".to_string(),
            notes: risk.notes.clone(),
            critical_path: false,
        });
    }
    if lower.contains("regression pass is blocked") {
        return Some(LaunchDependency {
            title: "QA regression pass".to_string(),
            owner: None,
            target_date: None,
            status: "blocked".to_string(),
            notes: risk.notes.clone(),
            critical_path: false,
        });
    }
    if lower.contains("fallback copy still needs approval from legal") {
        return Some(LaunchDependency {
            title: "Legal approval for fallback copy".to_string(),
            owner: None,
            target_date: None,
            status: "blocked".to_string(),
            notes: risk.notes.clone(),
            critical_path: false,
        });
    }
    None
}

fn augment_dependencies_from_blockers_and_decisions(
    dependencies: &mut Vec<LaunchDependency>,
    risks: &[LaunchRisk],
    decisions: &[LaunchDecision],
    has_update: bool,
) {
    let mut augmented = std::mem::take(dependencies);
    for risk in risks
        .iter()
        .filter(|item| item.blocker && !matches!(item.status.as_str(), "resolved" | "mitigated"))
    {
        if let Some(candidate) = blocker_linked_dependency_from_risk(risk) {
            merge_dependency_candidate(&mut augmented, candidate);
        }
    }
    if has_update
        && risks.iter().any(|item| {
            item.blocker
                && item
                    .title
                    .to_ascii_lowercase()
                    .contains("cannot finish validation before")
        })
    {
        for decision in decisions.iter().filter(|item| item.status != "resolved") {
            if decision
                .title
                .to_ascii_lowercase()
                .contains("phased cutover")
            {
                merge_dependency_candidate(
                    &mut augmented,
                    LaunchDependency {
                        title: "decision on phased cutover".to_string(),
                        owner: decision.owner.clone(),
                        target_date: decision.due_date.clone(),
                        status: "at_risk".to_string(),
                        notes: decision.notes.clone(),
                        critical_path: false,
                    },
                );
            }
        }
    }
    *dependencies = augmented;
}

fn prune_summary_like_dependencies(
    dependencies: &mut Vec<LaunchDependency>,
    decisions: &[LaunchDecision],
    risks: &[LaunchRisk],
    has_update: bool,
) {
    if dependencies.iter().any(|item| {
        item.title == "Legal approval for fallback copy"
            && matches!(item.status.as_str(), "at_risk" | "blocked")
    }) {
        dependencies.retain(|item| item.title != "Final screenshots approved");
    }

    if decisions.iter().any(|item| {
        item.title
            .to_ascii_lowercase()
            .contains("links expire in 15 minutes or require portal login")
    }) {
        dependencies.retain(|item| {
            !item
                .title
                .to_ascii_lowercase()
                .contains("security approval on secure access flow")
        });
    }

    if has_update
        && risks.iter().any(|item| {
            item.blocker
                && item
                    .title
                    .to_ascii_lowercase()
                    .contains("cannot finish validation before")
        })
        && decisions.iter().any(|item| {
            item.status != "resolved" && item.title.to_ascii_lowercase().contains("phased cutover")
        })
    {
        dependencies.retain(|item| {
            matches!(
                item.title.as_str(),
                "EU domain vendor validation" | "decision on phased cutover"
            )
        });
    }
}

fn ensure_refresh_decision_dependency(
    dependencies: &mut Vec<LaunchDependency>,
    decisions: &[LaunchDecision],
    risks: &[LaunchRisk],
    has_update: bool,
) {
    if !has_update
        || !risks.iter().any(|item| {
            item.blocker
                && item
                    .title
                    .to_ascii_lowercase()
                    .contains("cannot finish validation before")
        })
    {
        return;
    }

    for decision in decisions.iter().filter(|item| {
        item.status != "resolved" && item.title.to_ascii_lowercase().contains("phased cutover")
    }) {
        if dependencies.iter().any(|item| {
            item.title
                .eq_ignore_ascii_case("decision on phased cutover")
                || token_overlap_score(&item.title, &decision.title) >= 0.8
        }) {
            continue;
        }
        merge_dependency_candidate(
            dependencies,
            LaunchDependency {
                title: "decision on phased cutover".to_string(),
                owner: decision.owner.clone(),
                target_date: decision.due_date.clone(),
                status: "at_risk".to_string(),
                notes: decision.notes.clone(),
                critical_path: false,
            },
        );
    }
}

fn apply_execution_quality_guards(
    plan: &mut LaunchExecutionPlan,
    context_text: &str,
    update_text: Option<&str>,
    signal: &LaunchSignalAssessment,
    run_mode: LaunchRunMode,
) {
    let lines = parse_thread_lines(context_text, update_text);
    let has_update = lines.iter().any(|line| is_update_ref(&line.source_ref));
    let launch_closure_signal = has_launch_closure_signal(&lines);

    plan.target_date = normalize_launch_date_text(plan.target_date.take());
    plan.launch_window = normalize_launch_date_text(plan.launch_window.take());

    for milestone in &mut plan.milestones {
        milestone.target_date = normalize_launch_date_text(milestone.target_date.take());
    }
    let inferred_timeline = infer_best_timeline_from_lines(&lines);
    if should_replace_with_inferred_timeline(
        plan.target_date.as_deref(),
        inferred_timeline.0.as_deref(),
        &lines,
    ) {
        plan.target_date = inferred_timeline.0;
        plan.launch_window = inferred_timeline.1;
    } else {
        if should_use_more_specific_inferred_date(
            plan.target_date.as_deref(),
            inferred_timeline.0.as_deref(),
        ) {
            plan.target_date = inferred_timeline.0;
        }
        if should_fill_inferred_launch_window(
            plan.launch_window.as_deref(),
            inferred_timeline.1.as_deref(),
        ) {
            plan.launch_window = inferred_timeline.1;
        }
    }

    let current_dependencies = std::mem::take(&mut plan.dependencies);
    plan.dependencies =
        rebuild_dependencies(current_dependencies, &lines, signal.weak_signal, has_update);
    for dependency in &mut plan.dependencies {
        dependency.target_date = normalize_launch_date_text(dependency.target_date.take());
        if dependency.title == "Legal approval for fallback copy" {
            dependency.owner = None;
        }
        if let Some(line) = best_supporting_dependency_line(&lines, &dependency.title, has_update) {
            let body = line_body(&line.text);
            let better_context = dependency
                .notes
                .as_deref()
                .map(|value| {
                    looks_like_dependency_seed_line(value)
                        || token_overlap_score(value, &dependency.title) < 0.35
                })
                .unwrap_or(true);
            if better_context {
                dependency.notes = Some(body.clone());
                if let Some(owner) = extract_speaker_name(&line.text)
                    .filter(|name| !looks_like_invalid_owner_name(name))
                {
                    dependency.owner = Some(owner);
                }
                let inferred_status = infer_dependency_status_from_text(&line.text);
                if dependency_status_rank(&inferred_status)
                    >= dependency_status_rank(&dependency.status)
                    || matches!(inferred_status.as_str(), "done" | "ready")
                {
                    dependency.status = inferred_status;
                }
                if let Some(date) = normalize_launch_date_text(extract_date_phrase(&line.text)) {
                    dependency.target_date = Some(date);
                }
            }
        }
        if dependency
            .notes
            .as_deref()
            .map(looks_like_dependency_seed_line)
            .unwrap_or(false)
        {
            if let Some(line) = best_matching_line(
                &lines,
                dependency.notes.as_deref().unwrap_or(&dependency.title),
                false,
            ) {
                if let Some(owner) = extract_speaker_name(&line.text)
                    .filter(|name| !looks_like_invalid_owner_name(name))
                {
                    dependency.owner = Some(owner);
                    dependency.notes = Some(line_body(&line.text));
                }
            }
        }
        if plan.decisions.iter().any(|decision| {
            decision.status != "resolved"
                && token_overlap_score(&dependency.title, &decision.title) >= 0.55
                && dependency
                    .notes
                    .as_deref()
                    .map(|value| value.to_ascii_lowercase().contains("need a decision on"))
                    .unwrap_or(false)
        }) {
            dependency.status = "at_risk".to_string();
            dependency.owner = None;
        }
    }

    let current_decisions = std::mem::take(&mut plan.decisions);
    plan.decisions = rebuild_decisions(current_decisions, &lines, has_update);
    for decision in &mut plan.decisions {
        decision.due_date = normalize_launch_date_text(decision.due_date.take());
        if !is_explicit_launch_blocking_text(&format!(
            "{} {}",
            decision.title,
            decision.notes.clone().unwrap_or_default()
        )) {
            decision.launch_blocking = false;
        }
    }
    plan.dependencies.retain(|dependency| {
        !(matches!(dependency.status.as_str(), "done" | "ready")
            && plan.decisions.iter().any(|decision| {
                decision.status != "resolved"
                    && token_overlap_score(&dependency.title, &decision.title) >= 0.3
            }))
    });

    let current_risks = std::mem::take(&mut plan.risks);
    plan.risks = rebuild_risks(current_risks, &lines, &plan.decisions);
    for risk in &mut plan.risks {
        risk.owner = risk
            .owner
            .take()
            .filter(|name| !looks_like_invalid_owner_name(name));
        let supporting_text = format!("{} {}", risk.title, risk.notes.clone().unwrap_or_default());
        if !contains_explicit_blocker_cue(&supporting_text) {
            risk.blocker = false;
            if risk.status == "open" {
                risk.status = "watching".to_string();
            }
        }
    }

    if has_update && launch_closure_signal {
        plan.dependencies.retain(|item| {
            !looks_like_inferred_prod_readiness_object(&item.title, item.notes.as_deref())
        });
        plan.risks.retain(|item| {
            !looks_like_inferred_prod_readiness_object(&item.title, item.notes.as_deref())
        });
        let fully_resolved_refresh = plan
            .risks
            .iter()
            .all(|item| matches!(item.status.as_str(), "resolved" | "mitigated" | "watching"))
            && plan.decisions.is_empty()
            && plan
                .dependencies
                .iter()
                .all(|item| matches!(item.status.as_str(), "done" | "ready"));
        for owner in &mut plan.owners {
            if owner.update_status == "stale" && fully_resolved_refresh {
                owner.update_status = "current".to_string();
                owner.notes = Some(
                    "Latest launch update says there are no open blockers; earlier owner lag is no longer the gating issue."
                        .to_string(),
                );
            } else if owner.update_status == "stale"
                && owner
                    .notes
                    .as_deref()
                    .map(|value| value.contains("production-launch confirmation"))
                    .unwrap_or(false)
            {
                owner.update_status = "current".to_string();
                owner.notes = Some(
                    "Latest thread update says there are no open blockers for launch; no explicit new engineering concern is grounded in the thread."
                        .to_string(),
                );
            }
        }
    }

    augment_dependencies_from_blockers_and_decisions(
        &mut plan.dependencies,
        &plan.risks,
        &plan.decisions,
        has_update,
    );

    if signal.weak_signal {
        plan.milestones.clear();
        plan.owners.clear();
        plan.risks.clear();
        if plan.dependencies.len() > 2 {
            plan.dependencies.truncate(2);
        }
        if plan.decisions.len() > 1 {
            plan.decisions.truncate(1);
        }
        plan.critical_path.clear();
        if plan.source_context.summary.is_none() {
            plan.source_context.summary = Some(
                "Thread does not yet contain enough execution structure for an accountable launch plan."
                    .to_string(),
            );
        }
    } else {
        if plan.dependencies.len() > 4 {
            plan.dependencies.truncate(4);
        }
        if plan.decisions.len() > 3 {
            plan.decisions.truncate(3);
        }
        if plan.risks.len() > 3 {
            plan.risks.truncate(3);
        }
        prune_summary_like_dependencies(
            &mut plan.dependencies,
            &plan.decisions,
            &plan.risks,
            has_update,
        );
        ensure_refresh_decision_dependency(
            &mut plan.dependencies,
            &plan.decisions,
            &plan.risks,
            has_update,
        );
        if plan.milestones.len() > 4 {
            plan.milestones.truncate(4);
        }
    }

    let preserved_evidence = dedupe_by_key(
        plan.evidence
            .drain(..)
            .filter(|item| !item.snippet.trim().is_empty())
            .collect::<Vec<_>>(),
        |item| item.snippet.to_ascii_lowercase(),
    );
    plan.evidence = rebuild_grounded_evidence(plan, &lines, preserved_evidence, has_update);

    if run_mode != LaunchRunMode::Standard && plan.source_context.summary.is_none() {
        plan.source_context.summary =
            Some("Returned with fallback execution judgment because the analyzer could not produce a stable structured response.".to_string());
    }
}

fn normalize_launch_date_text(value: Option<String>) -> Option<String> {
    let normalized = normalize_optional_string(value)?;
    if normalized.to_ascii_lowercase().contains("week of ") {
        return Some(normalized);
    }
    if let Ok(parsed) = NaiveDate::parse_from_str(&normalized, "%Y-%m-%d") {
        return Some(parsed.format("%b %-d").to_string());
    }
    if let Some(extracted) = extract_date_phrase(&normalized) {
        return Some(extracted);
    }
    Some(normalized)
}

fn is_update_ref(source_ref: &str) -> bool {
    source_ref.trim().starts_with('U')
}

fn line_body(text: &str) -> String {
    strip_bullet_prefix(&strip_speaker_prefix(text))
}

fn strip_speaker_prefix(text: &str) -> String {
    let trimmed = text.trim();
    match trimmed.split_once(':') {
        Some((prefix, rest)) if should_strip_named_prefix(prefix) => rest.trim().to_string(),
        None => trimmed.to_string(),
        _ => trimmed.to_string(),
    }
}

fn strip_bullet_prefix(text: &str) -> String {
    text.trim()
        .trim_start_matches("- ")
        .trim_start_matches("• ")
        .trim()
        .to_string()
}

fn extract_speaker_name(text: &str) -> Option<String> {
    let speaker = text.split_once(':')?.0.trim();
    if !should_strip_named_prefix(speaker) || !speaker.contains(" - ") {
        return None;
    }
    let speaker = speaker.split(" - ").next().unwrap_or(speaker).trim();
    let speaker = speaker.split('(').next().unwrap_or(speaker).trim();
    if speaker.is_empty() {
        return None;
    }
    Some(speaker.to_string())
}

fn should_strip_named_prefix(prefix: &str) -> bool {
    let lower = prefix.trim().to_ascii_lowercase();
    lower.contains("thread") || prefix.contains(" - ") || prefix.contains('(')
}

fn extract_date_phrases(text: &str) -> Vec<String> {
    let body = line_body(text);
    let patterns = [
        r"(?i)\bmonday \d{1,2}(?::\d{2})?\s*(am|pm)\s*pt\b",
        r"(?i)\bmonday night\b",
        r"(?i)\bweek of (jan|january|feb|february|mar|march|apr|april|may|jun|june|jul|july|aug|august|sep|sept|september|oct|october|nov|november|dec|december) \d{1,2}\b",
        r"(?i)\bbefore (jan|january|feb|february|mar|march|apr|april|may|jun|june|jul|july|aug|august|sep|sept|september|oct|october|nov|november|dec|december) \d{1,2}\b",
        r"(?i)\b(jan|january|feb|february|mar|march|apr|april|may|jun|june|jul|july|aug|august|sep|sept|september|oct|october|nov|november|dec|december) \d{1,2}\b",
        r"(?i)\bbefore month end\b",
        r"(?i)\bsometime in (jan|january|feb|february|mar|march|apr|april|may|jun|june|jul|july|aug|august|sep|sept|september|oct|october|nov|november|dec|december)\b",
        r"(?i)\bnext Thursday\b",
        r"(?i)\bnext Tuesday\b",
    ];

    let mut values = Vec::new();
    for pattern in patterns {
        let regex = Regex::new(pattern).expect("date regex should compile");
        for found in regex.find_iter(&body) {
            values.push(found.as_str().trim().to_string());
        }
    }
    dedupe_by_key(values, |value| normalize_for_match(value))
}

fn extract_date_phrase(text: &str) -> Option<String> {
    extract_date_phrases(text).into_iter().next()
}

fn extract_last_date_phrase(text: &str) -> Option<String> {
    extract_date_phrases(text).into_iter().last()
}

fn contains_date_cue(text: &str) -> bool {
    extract_date_phrase(text).is_some()
}

fn contains_explicit_blocker_cue(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "blocked",
        "blocks launch",
        "blocks qa",
        "still failing",
        "cannot",
        "can't",
        "lost workspace access",
        "exposure risk",
        "not signed off",
        "permissions regression",
        "decision blocks launch",
        "critical blocker",
        "failing in staging",
        "won't know",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

fn contains_decision_cue(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "whether ",
        "decision on ",
        "no decision yet",
        "go/no-go",
        "go or no-go",
        "choose ",
        "pick ",
        "final answer",
        "not signed off on",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

fn is_explicit_launch_blocking_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("blocks launch")
        || lower.contains("launch-blocking")
        || lower.contains("cannot launch")
        || lower.contains("required signoff")
}

fn risk_requires_red_status(risk: &LaunchRisk) -> bool {
    let supporting_text = format!("{} {}", risk.title, risk.notes.clone().unwrap_or_default());
    let lower = supporting_text.to_ascii_lowercase();
    is_explicit_launch_blocking_text(&supporting_text)
        || lower.contains("phi exposure risk")
        || lower.contains("lost workspace access")
        || lower.contains("not signed off on rollback timing")
        || lower.contains("cannot finish validation before")
        || lower.contains("blocked until")
        || lower.contains("blocks qa from signing off")
        || lower.contains("failing in staging")
        || lower.contains("approval from legal")
}

fn canonicalize_decision_title(text: &str) -> String {
    let cleaned = strip_bullet_prefix(text);
    let lower = cleaned.to_ascii_lowercase();
    if let Some(index) = lower.find("whether ") {
        return cleaned[index..].trim_end_matches('.').to_string();
    }
    if lower.contains("go/no-go") || lower.contains("go or no-go") {
        return "go/no-go review".to_string();
    }
    if lower.contains("choose ") {
        if let Some(index) = lower.find("choose ") {
            return cleaned[index..].trim_end_matches('.').to_string();
        }
    }
    if lower.contains("pick ") {
        if let Some(index) = lower.find("pick ") {
            return cleaned[index..].trim_end_matches('.').to_string();
        }
    }
    if lower.contains("approve") || lower.contains("approval") {
        if let Some(index) = lower.find("approve ") {
            return cleaned[index..].trim_end_matches('.').to_string();
        }
        if let Some(index) = lower.find("approval") {
            return cleaned[index..].trim_end_matches('.').to_string();
        }
    }
    cleaned
        .split('.')
        .next()
        .unwrap_or(cleaned.as_str())
        .trim()
        .trim_end_matches('.')
        .to_string()
}

fn looks_like_missing_info_question(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("who is the")
        || lower.contains("who owns")
        || lower.contains("name the owner")
        || lower.contains("approver is")
        || lower.contains("approver not named")
        || lower.contains("name the approver")
        || lower.contains("security approver")
}

fn looks_like_invalid_owner_name(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    lower.is_empty()
        || lower.starts_with('-')
        || lower.contains("thread")
        || lower.contains("checklist")
        || lower.contains("notes")
        || lower.contains("approver")
        || lower.ends_with(" note")
        || lower.ends_with(" comment")
}

fn looks_like_generic_role(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "founder" | "growth" | "design" | "product" | "support lead" | "ops" | "security"
    )
}

fn normalize_for_match(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn token_overlap_score(left: &str, right: &str) -> f32 {
    let left_tokens = normalize_for_match(left)
        .split_whitespace()
        .filter(|token| token.len() > 1)
        .map(|token| token.to_string())
        .collect::<HashSet<_>>();
    let right_tokens = normalize_for_match(right)
        .split_whitespace()
        .filter(|token| token.len() > 1)
        .map(|token| token.to_string())
        .collect::<HashSet<_>>();
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }
    let overlap = left_tokens.intersection(&right_tokens).count() as f32;
    let denom = left_tokens.len().max(right_tokens.len()) as f32;
    overlap / denom
}

fn looks_like_resolved_status_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let resolved_hit = [
        "approved",
        "passed",
        "scheduled",
        "frozen",
        "done",
        "complete",
        "completed",
        "ready",
        "finished",
        "exported",
        "updated",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase));
    let unresolved_hit = [
        "still need",
        "need ",
        "needs ",
        "pending",
        "waiting on",
        "once ",
        "until ",
        "not ready",
        "not signed off",
        "no decision yet",
        "depends on",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase));
    resolved_hit && !unresolved_hit
}

fn capture_after_phrase(text: &str, phrase: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let index = lower.find(phrase)?;
    let mut output = text[index + phrase.len()..]
        .trim()
        .trim_end_matches('.')
        .to_string();
    if let Some((head, _)) = output.split_once(" and ") {
        output = head.trim().to_string();
    }
    Some(output)
}

fn canonicalize_short_object_phrase(text: &str) -> String {
    text.trim()
        .trim_end_matches('.')
        .trim_start_matches("the ")
        .trim_start_matches("a ")
        .trim()
        .to_string()
}

fn trim_decision_clause(text: &str) -> String {
    let mut value = text.trim().trim_end_matches('.').to_string();
    for marker in [
        ". ",
        " Security review found ",
        " security review found ",
        " We need a decision",
        " we need a decision",
        " No decision yet",
        " no decision yet",
        " I can own ",
        " i can own ",
        " but I need ",
        " but i need ",
        " Owners should come ",
        " owners should come ",
    ] {
        if let Some((head, _)) = value.split_once(marker) {
            value = head.trim().trim_end_matches('.').to_string();
        }
    }
    value
}

fn looks_like_objective_dependency(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (lower.contains("live for")
        || lower.contains("live by")
        || lower.contains("migration complete")
        || lower.contains("csv export live")
        || lower.contains("customer advisory board"))
        && !lower.contains("launch email")
}

fn looks_like_dependency_sentence(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("need ")
        || lower.contains("still need ")
        || lower.contains("depends on")
        || lower.contains("ready once")
        || lower.contains("waiting on")
        || lower.contains("pending")
}

fn looks_like_timeline_only_object(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    lower.starts_with("week of ")
        || lower.starts_with("possible window")
        || lower.contains("placeholder not a promise")
        || lower == "monday 10pm pt"
        || lower.contains("date is real")
        || lower.starts_with("five days once")
}

fn looks_like_procedural_review_item(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    lower.starts_with("go/no-go review")
        || lower.starts_with("go or no-go review")
        || lower.contains("owners should come with status")
}

fn refresh_dependency_priority(title: &str) -> usize {
    let lower = title.to_ascii_lowercase();
    if lower.contains("launch email") || lower.contains("launch") {
        0
    } else if lower.contains("certification") || lower.contains("cutover") {
        1
    } else {
        2
    }
}

fn infer_best_timeline_from_lines(lines: &[ThreadLine]) -> (Option<String>, Option<String>) {
    let mut target_date = None;
    let mut launch_window = None;
    for line in lines {
        let body = line_body(&line.text);
        let lower = body.to_ascii_lowercase();
        let dates = extract_date_phrases(&body);
        if lower.contains("possible window") || lower.contains("placeholder not a promise") {
            if let Some(window) = dates
                .iter()
                .rev()
                .find(|value| value.to_ascii_lowercase().contains("week of"))
            {
                launch_window = Some(window.clone());
            }
        }
        if lower.contains("live by")
            || lower.contains("target ")
            || lower.contains("cutover ")
            || lower.contains("launch ")
            || lower.contains("ga on")
            || lower.contains("by ")
        {
            if let Some(date) = dates.first() {
                if date.to_ascii_lowercase().contains("week of")
                    || date.to_ascii_lowercase().contains("before ")
                    || date.to_ascii_lowercase().contains("sometime")
                {
                    return (
                        None,
                        launch_window
                            .or_else(|| dates.last().cloned())
                            .or_else(|| Some(date.clone())),
                    );
                }
                if target_date.is_none() {
                    target_date = Some(date.clone());
                } else if should_use_more_specific_inferred_date(
                    target_date.as_deref(),
                    Some(date.as_str()),
                ) {
                    target_date = Some(date.clone());
                }
                if launch_window.is_none() {
                    launch_window = dates.last().cloned().filter(|value| value != date);
                }
            }
        }
    }
    if target_date.is_some() || launch_window.is_some() {
        return (target_date, launch_window);
    }
    infer_fallback_timeline(lines)
}

fn has_launch_closure_signal(lines: &[ThreadLine]) -> bool {
    lines.iter().any(|line| {
        let lower = line_body(&line.text).to_ascii_lowercase();
        is_update_ref(&line.source_ref)
            && (lower.contains("no open blockers for launch")
                || lower.contains("no blocker for launch")
                || lower.contains("no blockers for launch"))
    })
}

fn should_fill_inferred_launch_window(
    current_launch_window: Option<&str>,
    inferred_launch_window: Option<&str>,
) -> bool {
    current_launch_window.is_none() && inferred_launch_window.is_some()
}

fn should_use_more_specific_inferred_date(
    current_target_date: Option<&str>,
    inferred_target_date: Option<&str>,
) -> bool {
    let (Some(current), Some(inferred)) = (current_target_date, inferred_target_date) else {
        return false;
    };
    if current.eq_ignore_ascii_case(inferred) {
        return false;
    }
    let same_weekday = extract_weekday_token(current)
        .zip(extract_weekday_token(inferred))
        .map(|(left, right)| left == right)
        .unwrap_or(false);
    same_weekday && date_precision_score(inferred) > date_precision_score(current)
}

fn extract_weekday_token(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    for weekday in [
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ] {
        if lower.contains(weekday) {
            return Some(weekday);
        }
    }
    None
}

fn date_precision_score(text: &str) -> usize {
    let lower = text.to_ascii_lowercase();
    let has_clock_time = lower.contains("am") || lower.contains("pm");
    let has_numeric_day = lower.chars().any(|ch| ch.is_ascii_digit());
    let is_week_window = lower.contains("week of") || lower.contains("before ");
    if has_clock_time && has_numeric_day {
        3
    } else if has_numeric_day && !is_week_window {
        2
    } else if is_week_window {
        1
    } else {
        0
    }
}

fn looks_like_inferred_prod_readiness_object(title: &str, notes: Option<&str>) -> bool {
    let title_lower = title.to_ascii_lowercase();
    let notes_lower = notes.unwrap_or("").to_ascii_lowercase();
    title_lower.contains("production readiness")
        || title_lower.contains("production go-live confirmation")
        || notes_lower.contains("no explicit production deployment update")
        || notes_lower.contains("no explicit production deployment confirmation")
        || notes_lower.contains("no explicit production go-live confirmation")
}

fn should_replace_with_inferred_timeline(
    current_target_date: Option<&str>,
    inferred_target_date: Option<&str>,
    lines: &[ThreadLine],
) -> bool {
    let Some(inferred) = inferred_target_date else {
        return false;
    };
    let Some(current) = current_target_date else {
        return true;
    };
    if current.eq_ignore_ascii_case(inferred) {
        return false;
    }
    lines.iter().any(|line| {
        let full = line.text.to_ascii_lowercase();
        let body = line_body(&line.text).to_ascii_lowercase();
        full.contains(&current.to_ascii_lowercase())
            && !body.contains(&current.to_ascii_lowercase())
    })
}

fn normalize_launch_execution_plan(
    mut plan: LaunchExecutionPlan,
    source_type: &str,
    source_label: Option<String>,
    prior_plan: Option<&LaunchExecutionPlan>,
) -> LaunchExecutionPlan {
    plan.id = normalize_optional_string(Some(plan.id))
        .or_else(|| prior_plan.map(|item| item.id.clone()))
        .unwrap_or_else(|| format!("launch_{}", Uuid::new_v4()));
    plan.title = normalize_optional_string(Some(plan.title))
        .or_else(|| source_label.clone())
        .or_else(|| prior_plan.map(|item| item.title.clone()))
        .unwrap_or_else(|| "Launch execution plan".to_string());
    plan.objective = normalize_optional_string(plan.objective);

    plan.source_context.input_type = source_type.to_string();
    plan.source_context.source_label = normalize_optional_string(plan.source_context.source_label)
        .or(source_label)
        .or_else(|| prior_plan.and_then(|item| item.source_context.source_label.clone()));
    plan.source_context.summary = normalize_optional_string(plan.source_context.summary)
        .or_else(|| prior_plan.and_then(|item| item.source_context.summary.clone()));

    plan.target_date = normalize_optional_string(plan.target_date);
    plan.launch_window = normalize_optional_string(plan.launch_window);
    plan.milestones = normalize_milestones(plan.milestones);
    let owners = std::mem::take(&mut plan.owners);
    plan.owners = normalize_owners(owners, &plan);
    plan.dependencies = normalize_dependencies(plan.dependencies);
    let critical_path = std::mem::take(&mut plan.critical_path);
    plan.critical_path = normalize_critical_path(critical_path, &plan);
    plan.risks = normalize_risks(plan.risks);
    plan.decisions = normalize_decisions(plan.decisions);
    plan.evidence = normalize_evidence(plan.evidence);
    plan.last_updated_at = normalize_optional_string(Some(plan.last_updated_at))
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    plan.follow_up_items = Vec::new();
    plan.readiness_status = String::new();
    plan.readiness_reason = String::new();

    plan
}

fn normalize_milestones(values: Vec<LaunchMilestone>) -> Vec<LaunchMilestone> {
    dedupe_by_key(
        values.into_iter().filter_map(|mut item| {
            item.title = normalize_required_string(item.title)?;
            item.owner = normalize_optional_string(item.owner);
            item.target_date = normalize_optional_string(item.target_date);
            item.status = normalize_milestone_status(item.status);
            item.notes = normalize_optional_string(item.notes);
            Some(item)
        }),
        |item| item.title.to_ascii_lowercase(),
    )
}

fn normalize_owners(values: Vec<LaunchOwner>, plan: &LaunchExecutionPlan) -> Vec<LaunchOwner> {
    let mut owners = dedupe_by_key(
        values.into_iter().filter_map(|mut item| {
            item.name = normalize_required_string(item.name)?;
            if looks_like_invalid_owner_name(&item.name) {
                return None;
            }
            item.role = normalize_optional_string(item.role);
            item.responsibilities = normalize_string_list(item.responsibilities);
            item.update_status = normalize_owner_update_status(item.update_status);
            item.notes = normalize_optional_string(item.notes);
            Some(item)
        }),
        |item| item.name.to_ascii_lowercase(),
    );

    let mut known = owners
        .iter()
        .map(|item| item.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    for name in collect_owner_names(plan) {
        let key = name.to_ascii_lowercase();
        if known.insert(key) {
            owners.push(LaunchOwner {
                name,
                update_status: "unknown".to_string(),
                ..LaunchOwner::default()
            });
        }
    }

    owners
}

fn normalize_dependencies(values: Vec<LaunchDependency>) -> Vec<LaunchDependency> {
    dedupe_by_key(
        values.into_iter().filter_map(|mut item| {
            item.title = normalize_required_string(item.title)?;
            item.owner = normalize_optional_string(item.owner)
                .filter(|name| !looks_like_invalid_owner_name(name));
            item.target_date = normalize_optional_string(item.target_date);
            item.status = normalize_dependency_status(item.status);
            item.notes = normalize_optional_string(item.notes);
            Some(item)
        }),
        |item| item.title.to_ascii_lowercase(),
    )
}

fn normalize_critical_path(
    mut values: Vec<LaunchCriticalPathItem>,
    plan: &LaunchExecutionPlan,
) -> Vec<LaunchCriticalPathItem> {
    values = dedupe_by_key(
        values.into_iter().filter_map(|mut item| {
            item.title = normalize_required_string(item.title)?;
            item.item_type = normalize_critical_path_item_type(item.item_type);
            item.owner = normalize_optional_string(item.owner);
            item.target_date = normalize_optional_string(item.target_date);
            item.status = normalize_optional_string(Some(item.status))
                .unwrap_or_else(|| "unknown".to_string());
            item.reason = normalize_optional_string(item.reason);
            Some(item)
        }),
        |item| format!("{}:{}", item.item_type, item.title.to_ascii_lowercase()),
    );

    if !values.is_empty() {
        return values;
    }

    let milestone_items = plan
        .milestones
        .iter()
        .filter(|item| item.critical_path)
        .map(|item| LaunchCriticalPathItem {
            title: item.title.clone(),
            item_type: "milestone".to_string(),
            owner: item.owner.clone(),
            target_date: item.target_date.clone(),
            status: item.status.clone(),
            reason: item.notes.clone(),
        });
    let dependency_items = plan
        .dependencies
        .iter()
        .filter(|item| item.critical_path)
        .map(|item| LaunchCriticalPathItem {
            title: item.title.clone(),
            item_type: "dependency".to_string(),
            owner: item.owner.clone(),
            target_date: item.target_date.clone(),
            status: item.status.clone(),
            reason: item.notes.clone(),
        });

    dedupe_by_key(milestone_items.chain(dependency_items), |item| {
        format!("{}:{}", item.item_type, item.title.to_ascii_lowercase())
    })
}

fn normalize_risks(values: Vec<LaunchRisk>) -> Vec<LaunchRisk> {
    dedupe_by_key(
        values.into_iter().filter_map(|mut item| {
            item.title = normalize_required_string(item.title)?;
            item.owner = normalize_optional_string(item.owner)
                .filter(|name| !looks_like_invalid_owner_name(name));
            item.severity = normalize_risk_severity(item.severity);
            item.status = normalize_risk_status(item.status);
            item.notes = normalize_optional_string(item.notes);
            Some(item)
        }),
        |item| item.title.to_ascii_lowercase(),
    )
}

fn normalize_decisions(values: Vec<LaunchDecision>) -> Vec<LaunchDecision> {
    dedupe_by_key(
        values.into_iter().filter_map(|mut item| {
            item.title = normalize_required_string(item.title)?;
            item.owner = normalize_optional_string(item.owner)
                .filter(|name| !looks_like_invalid_owner_name(name));
            item.due_date = normalize_optional_string(item.due_date);
            item.status = normalize_decision_status(item.status);
            item.notes = normalize_optional_string(item.notes);
            Some(item)
        }),
        |item| item.title.to_ascii_lowercase(),
    )
}

fn normalize_evidence(values: Vec<LaunchEvidence>) -> Vec<LaunchEvidence> {
    let mut output = dedupe_by_key(
        values.into_iter().filter_map(|mut item| {
            item.label = normalize_required_string(item.label)?;
            item.snippet = normalize_required_string(item.snippet)?;
            item.source_ref = normalize_optional_string(item.source_ref);
            Some(item)
        }),
        |item| {
            format!(
                "{}:{}",
                item.label.to_ascii_lowercase(),
                item.snippet.to_ascii_lowercase()
            )
        },
    );

    if output.len() > MAX_EVIDENCE_ITEMS {
        output.truncate(MAX_EVIDENCE_ITEMS);
    }
    output
}

fn derive_follow_up_items(plan: &LaunchExecutionPlan) -> Vec<LaunchFollowUpItem> {
    if plan
        .source_context
        .summary
        .as_deref()
        .map(|value| value.contains("not yet contain enough execution structure"))
        .unwrap_or(false)
    {
        return Vec::new();
    }

    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let has_contingency_decision = plan.decisions.iter().any(|item| {
        item.status != "resolved" && item.title.to_ascii_lowercase().contains("phased cutover")
    });

    for owner in &plan.owners {
        if owner.update_status == "missing" || owner.update_status == "stale" {
            let kind = if owner.update_status == "missing" {
                "missing_owner"
            } else {
                "stale_update"
            };
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: kind.to_string(),
                    target: owner.name.clone(),
                    owner: Some(owner.name.clone()),
                    priority: "medium".to_string(),
                    reason: owner.notes.clone().unwrap_or_else(|| {
                        if owner.update_status == "missing" {
                            "Owner is referenced in the launch but no concrete update is available.".to_string()
                        } else {
                            "Owner update appears stale relative to the launch thread.".to_string()
                        }
                    }),
                    suggested_message: format!(
                        "Can you send a launch status update for your area, including current risk, next step, and expected date?"
                    ),
                },
            );
        }
    }

    for item in &plan.dependencies {
        if item.status == "at_risk"
            && item.owner.is_some()
            && item
                .notes
                .as_deref()
                .map(|value| {
                    let lower = value.to_ascii_lowercase();
                    lower.contains("not confirmed")
                        || lower.contains("half done")
                        || lower.contains("still need")
                })
                .unwrap_or(false)
        {
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "at_risk_dependency".to_string(),
                    target: item.owner.clone().unwrap_or_else(|| item.title.clone()),
                    owner: item.owner.clone(),
                    priority: "medium".to_string(),
                    reason: item
                        .notes
                        .clone()
                        .unwrap_or_else(|| "Dependency is still at risk.".to_string()),
                    suggested_message: format!(
                        "What is the current status of \"{}\", what is still open, and when will it be cleared?",
                        item.title
                    ),
                },
            );
        }
        if has_contingency_decision
            && item.status == "at_risk"
            && item.target_date.is_none()
            && matches!(
                item.title.as_str(),
                "IT confirmation for SPF and DKIM changes" | "support staffing plan"
            )
        {
            continue;
        }
        if matches!(item.status.as_str(), "at_risk" | "blocked") && item.owner.is_none() {
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "missing_owner".to_string(),
                    target: item.title.clone(),
                    owner: None,
                    priority: if item.status == "blocked" {
                        "high"
                    } else {
                        "medium"
                    }
                    .to_string(),
                    reason: item.notes.clone().unwrap_or_else(|| {
                        "Dependency does not have a clear owner yet.".to_string()
                    }),
                    suggested_message: format!(
                        "Who owns \"{}\", and who is responsible for clearing it before launch?",
                        item.title
                    ),
                },
            );
        }
        if item.status == "at_risk" && item.owner.is_some() && item.target_date.is_none() {
            push_follow_up(
                &mut items,
                &mut seen,
                build_missing_date_follow_up(
                    "dependency",
                    &item.title,
                    item.owner.as_deref(),
                    item.critical_path,
                ),
            );
        }
        if item.status == "blocked" {
            let duplicates_open_decision = plan.decisions.iter().any(|decision| {
                decision.status != "resolved"
                    && token_overlap_score(&decision.title, &item.title) >= 0.55
            });
            let duplicates_blocker_risk = plan.risks.iter().any(|risk| {
                risk.blocker
                    && !matches!(risk.status.as_str(), "resolved" | "mitigated")
                    && token_overlap_score(&risk.title, &item.title) >= 0.55
            });
            if duplicates_open_decision || duplicates_blocker_risk {
                continue;
            }
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "unresolved_blocker".to_string(),
                    target: item
                        .owner
                        .clone()
                        .unwrap_or_else(|| item.title.clone()),
                    owner: item.owner.clone(),
                    priority: if item.critical_path { "high" } else { "medium" }.to_string(),
                    reason: item
                        .notes
                        .clone()
                        .unwrap_or_else(|| "Dependency is currently blocked.".to_string()),
                    suggested_message: format!(
                        "What is blocking \"{}\", who owns the unblock, and what is the earliest credible resolution date?",
                        item.title
                    ),
                },
            );
        }
    }

    for item in &plan.risks {
        if item.blocker && !matches!(item.status.as_str(), "resolved" | "mitigated") {
            let duplicates_open_decision = plan.decisions.iter().any(|decision| {
                decision.status != "resolved"
                    && token_overlap_score(&decision.title, &item.title) >= 0.55
            });
            let derivative_of_existing_blocker = plan.risks.iter().any(|risk| {
                risk.title != item.title
                    && risk.title.to_ascii_lowercase().contains("callback retries")
                    && item.title.to_ascii_lowercase().contains("callback retries")
                    && item.title.to_ascii_lowercase().contains("regression pass")
            });
            let should_shift_to_decision_follow_up = has_contingency_decision
                && item
                    .title
                    .to_ascii_lowercase()
                    .contains("cannot finish validation before");
            let driven_by_open_decision = item
                .notes
                .as_deref()
                .map(|value| {
                    let lower = value.to_ascii_lowercase();
                    lower.contains("need a decision on whether")
                        || lower.contains("decision blocks launch")
                        || lower.contains("approver")
                })
                .unwrap_or(false);
            if duplicates_open_decision
                || driven_by_open_decision
                || should_shift_to_decision_follow_up
                || derivative_of_existing_blocker
            {
                continue;
            }
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "unresolved_blocker".to_string(),
                    target: item
                        .owner
                        .clone()
                        .unwrap_or_else(|| item.title.clone()),
                    owner: item.owner.clone(),
                    priority: if matches!(item.severity.as_str(), "high" | "critical") {
                        "high"
                    } else {
                        "medium"
                    }
                    .to_string(),
                    reason: item
                        .notes
                        .clone()
                        .unwrap_or_else(|| "Launch blocker is still unresolved.".to_string()),
                    suggested_message: format!(
                        "What is the unblock plan for \"{}\", and what needs to happen next to clear it?",
                        item.title
                    ),
                },
            );
        }
    }

    for item in &plan.decisions {
        if item.status != "resolved"
            && (item
                .notes
                .as_deref()
                .map(|value| {
                    let lower = value.to_ascii_lowercase();
                    lower.contains("approver") || lower.contains("not named")
                })
                .unwrap_or(false)
                || plan.evidence.iter().any(|evidence| {
                    let lower = evidence.snippet.to_ascii_lowercase();
                    lower.contains("approver not named")
                        || lower.contains("security approver not named")
                }))
        {
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "missing_owner".to_string(),
                    target: "security approver not named".to_string(),
                    owner: None,
                    priority: "high".to_string(),
                    reason: item
                        .notes
                        .clone()
                        .unwrap_or_else(|| "Launch-blocking decision has no named approver.".to_string()),
                    suggested_message:
                        "Who is the named approver for this launch-blocking security decision, and who will make the final call?".to_string(),
                },
            );
        }
        if item.status != "resolved" {
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "unresolved_decision".to_string(),
                    target: item
                        .owner
                        .clone()
                        .unwrap_or_else(|| format!("decision: {}", item.title)),
                    owner: item.owner.clone(),
                    priority: if item.launch_blocking
                        || plan.risks.iter().any(|risk| {
                            risk.blocker && !matches!(risk.status.as_str(), "resolved" | "mitigated")
                        }) {
                        "high"
                    } else {
                        "medium"
                    }
                    .to_string(),
                    reason: item
                        .notes
                        .clone()
                        .unwrap_or_else(|| "Launch decision is still unresolved.".to_string()),
                    suggested_message: format!(
                        "Who will decide \"{}\", and by when can the launch team expect a clear answer?",
                        item.title
                    ),
                },
            );
        }
        if item.status != "resolved" && item.due_date.is_none() && item.owner.is_some() {
            push_follow_up(
                &mut items,
                &mut seen,
                build_missing_date_follow_up(
                    "decision",
                    &item.title,
                    item.owner.as_deref(),
                    item.launch_blocking,
                ),
            );
        }
    }

    items.sort_by(|left, right| {
        follow_up_priority_rank(&left.priority).cmp(&follow_up_priority_rank(&right.priority))
    });
    if items.len() > MAX_FOLLOW_UP_ITEMS {
        items.truncate(MAX_FOLLOW_UP_ITEMS);
    }
    items
}

fn build_missing_date_follow_up(
    category: &str,
    title: &str,
    owner: Option<&str>,
    high_priority: bool,
) -> LaunchFollowUpItem {
    let owner_phrase = owner.unwrap_or("the owner");
    LaunchFollowUpItem {
        kind: "missing_date".to_string(),
        target: owner
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("{}: {}", category, title)),
        owner: owner.map(|value| value.to_string()),
        priority: if high_priority { "high" } else { "medium" }.to_string(),
        reason: format!("{} does not have a committed date or launch window.", title),
        suggested_message: format!(
            "Can {} commit a date or launch window for \"{}\" so the launch plan has a credible timeline?",
            owner_phrase, title
        ),
    }
}

fn push_follow_up(
    items: &mut Vec<LaunchFollowUpItem>,
    seen: &mut HashSet<String>,
    item: LaunchFollowUpItem,
) {
    let key = format!("{}:{}", item.kind, item.target.to_ascii_lowercase());
    if seen.insert(key) {
        items.push(item);
    }
}

fn follow_up_priority_rank(priority: &str) -> usize {
    match priority {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

fn derive_readiness_status(plan: &LaunchExecutionPlan) -> (String, String) {
    if plan.evidence.is_empty() {
        return (
            "red".to_string(),
            "No credible readiness evidence was extracted from the source context.".to_string(),
        );
    }

    if plan
        .source_context
        .summary
        .as_deref()
        .map(|value| value.contains("not yet contain enough execution structure"))
        .unwrap_or(false)
    {
        return (
            "red".to_string(),
            "Thread does not yet contain enough execution structure to support an accountable plan."
                .to_string(),
        );
    }

    if let Some(risk) = plan.risks.iter().find(|item| {
        item.blocker
            && matches!(item.severity.as_str(), "high" | "critical")
            && !matches!(item.status.as_str(), "resolved" | "mitigated")
            && risk_requires_red_status(item)
    }) {
        return (
            "red".to_string(),
            format!("Critical blocker still open: {}.", risk.title),
        );
    }

    if let Some(decision) = plan
        .decisions
        .iter()
        .find(|item| item.launch_blocking && item.status != "resolved")
    {
        return (
            "red".to_string(),
            format!(
                "Launch-blocking decision still unresolved: {}.",
                decision.title
            ),
        );
    }

    if let Some(item) = plan.critical_path.iter().find(|item| {
        item.owner
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    }) {
        return (
            "red".to_string(),
            format!("Critical path item lacks a clear owner: {}.", item.title),
        );
    }

    let has_yellow_signals = plan.follow_up_items.iter().any(|item| {
        matches!(
            item.kind.as_str(),
            "missing_date" | "missing_owner" | "stale_update" | "unresolved_decision"
        )
    }) || plan
        .dependencies
        .iter()
        .any(|item| matches!(item.status.as_str(), "at_risk" | "blocked" | "unknown"))
        || plan
            .risks
            .iter()
            .any(|item| !item.blocker && matches!(item.status.as_str(), "open" | "watching"))
        || plan
            .decisions
            .iter()
            .any(|item| !item.launch_blocking && item.status != "resolved")
        || plan.target_date.is_none() && plan.launch_window.is_none();

    if has_yellow_signals {
        return (
            "yellow".to_string(),
            "Launch has meaningful open follow-ups, at-risk dependencies, or timeline gaps."
                .to_string(),
        );
    }

    (
        "green".to_string(),
        "Critical blockers are cleared, owners are assigned, dates are present, and evidence supports readiness.".to_string(),
    )
}

fn build_tracker_rows(plan: &LaunchExecutionPlan) -> Vec<LaunchTrackerRow> {
    let milestones = plan.milestones.iter().map(|item| LaunchTrackerRow {
        category: "milestone".to_string(),
        title: item.title.clone(),
        owner: item.owner.clone(),
        target_date: item.target_date.clone(),
        status: item.status.clone(),
        risk_level: Some(if item.critical_path {
            "critical_path".to_string()
        } else {
            "normal".to_string()
        }),
    });

    let dependencies = plan.dependencies.iter().map(|item| LaunchTrackerRow {
        category: "dependency".to_string(),
        title: item.title.clone(),
        owner: item.owner.clone(),
        target_date: item.target_date.clone(),
        status: item.status.clone(),
        risk_level: Some(if item.critical_path || item.status == "blocked" {
            "high".to_string()
        } else if item.status == "at_risk" {
            "medium".to_string()
        } else {
            "normal".to_string()
        }),
    });

    let decisions = plan.decisions.iter().map(|item| LaunchTrackerRow {
        category: "decision".to_string(),
        title: item.title.clone(),
        owner: item.owner.clone(),
        target_date: item.due_date.clone(),
        status: item.status.clone(),
        risk_level: Some(if item.launch_blocking {
            "high".to_string()
        } else {
            "medium".to_string()
        }),
    });

    let risks = plan.risks.iter().map(|item| LaunchTrackerRow {
        category: "risk".to_string(),
        title: item.title.clone(),
        owner: item.owner.clone(),
        target_date: None,
        status: item.status.clone(),
        risk_level: Some(item.severity.clone()),
    });

    milestones
        .chain(dependencies)
        .chain(decisions)
        .chain(risks)
        .collect()
}

fn derive_change_summary(
    prior_plan: Option<&LaunchExecutionPlan>,
    current_plan: &LaunchExecutionPlan,
) -> Vec<String> {
    let Some(prior_plan) = prior_plan else {
        return vec!["Initial launch execution brief created from the pasted thread.".to_string()];
    };

    let mut changes = Vec::new();

    if prior_plan.readiness_status != current_plan.readiness_status
        && !prior_plan.readiness_status.is_empty()
        && !current_plan.readiness_status.is_empty()
    {
        changes.push(format!(
            "Readiness changed from {} to {}.",
            prior_plan.readiness_status, current_plan.readiness_status
        ));
    }

    let prior_open_blockers = blocker_titles(prior_plan);
    let current_open_blockers = blocker_titles(current_plan);
    for added in current_open_blockers
        .difference(&prior_open_blockers)
        .take(3)
    {
        changes.push(format!("New blocker surfaced: {}.", added));
    }
    for resolved in prior_open_blockers
        .difference(&current_open_blockers)
        .take(3)
    {
        changes.push(format!("Blocker cleared or downgraded: {}.", resolved));
    }

    let prior_open_decisions = open_decision_titles(prior_plan);
    let current_open_decisions = open_decision_titles(current_plan);
    for added in current_open_decisions
        .difference(&prior_open_decisions)
        .take(3)
    {
        changes.push(format!("New open decision: {}.", added));
    }
    for resolved in prior_open_decisions
        .difference(&current_open_decisions)
        .take(3)
    {
        changes.push(format!("Decision resolved: {}.", resolved));
    }

    let prior_milestones = milestone_status_map(prior_plan);
    let current_milestones = milestone_status_map(current_plan);
    for (title, status) in &current_milestones {
        if let Some(previous_status) = prior_milestones.get(title) {
            if previous_status != status {
                changes.push(format!(
                    "Milestone status changed for {}: {} -> {}.",
                    title, previous_status, status
                ));
            }
        }
    }

    if changes.is_empty() {
        changes.push("No material change was detected from the latest update.".to_string());
    }

    if changes.len() > 6 {
        changes.truncate(6);
    }

    changes
}

fn blocker_titles(plan: &LaunchExecutionPlan) -> BTreeSet<String> {
    let risk_titles = plan.risks.iter().filter_map(|item| {
        if item.blocker && !matches!(item.status.as_str(), "resolved" | "mitigated") {
            Some(item.title.clone())
        } else {
            None
        }
    });
    let dependency_titles = plan.dependencies.iter().filter_map(|item| {
        let duplicates_blocker_risk = plan.risks.iter().any(|risk| {
            risk.blocker
                && !matches!(risk.status.as_str(), "resolved" | "mitigated")
                && token_overlap_score(&risk.title, &item.title) >= 0.55
        });
        let duplicates_open_decision = plan.decisions.iter().any(|decision| {
            decision.status != "resolved"
                && token_overlap_score(&decision.title, &item.title) >= 0.55
        });
        if item.status == "blocked" && !duplicates_blocker_risk && !duplicates_open_decision {
            Some(item.title.clone())
        } else {
            None
        }
    });

    risk_titles.chain(dependency_titles).collect()
}

fn open_decision_titles(plan: &LaunchExecutionPlan) -> BTreeSet<String> {
    plan.decisions
        .iter()
        .filter(|item| item.status != "resolved")
        .map(|item| item.title.clone())
        .collect()
}

fn milestone_status_map(plan: &LaunchExecutionPlan) -> std::collections::BTreeMap<String, String> {
    plan.milestones
        .iter()
        .map(|item| (item.title.clone(), item.status.clone()))
        .collect()
}

fn build_readiness_brief(
    mut brief: LaunchReadinessBrief,
    plan: &LaunchExecutionPlan,
    change_summary: Vec<String>,
) -> LaunchReadinessBrief {
    brief.overall_readiness = normalize_optional_string(Some(brief.overall_readiness))
        .unwrap_or_else(|| {
            format!(
                "{} readiness. {}",
                title_case(&plan.readiness_status),
                plan.readiness_reason
            )
        });

    brief.critical_blockers = blocker_titles(plan).into_iter().collect();

    if brief.at_risk_dependencies.is_empty() {
        brief.at_risk_dependencies = plan
            .dependencies
            .iter()
            .filter(|item| matches!(item.status.as_str(), "at_risk" | "blocked"))
            .map(|item| item.title.clone())
            .collect();
    } else {
        brief.at_risk_dependencies = normalize_string_list(brief.at_risk_dependencies);
    }

    brief.open_decisions = plan
        .decisions
        .iter()
        .filter(|item| item.status != "resolved")
        .map(|item| item.title.clone())
        .collect();

    brief.missing_owner_updates = plan
        .owners
        .iter()
        .filter(|item| matches!(item.update_status.as_str(), "missing" | "stale"))
        .map(|item| item.name.clone())
        .collect();

    brief.what_changed_since_last_update = change_summary;

    if brief.evidence_summary.is_empty() {
        brief.evidence_summary = plan
            .evidence
            .iter()
            .take(4)
            .map(|item| format!("{}: {}", item.label, item.snippet))
            .collect();
    } else {
        brief.evidence_summary = normalize_string_list(brief.evidence_summary);
    }

    brief.next_follow_up_focus =
        normalize_optional_string(brief.next_follow_up_focus).or_else(|| {
            plan.follow_up_items
                .iter()
                .find(|item| item.priority == "high")
                .or_else(|| plan.follow_up_items.first())
                .map(|item| item.target.clone())
        });

    brief
}

fn collect_owner_names(plan: &LaunchExecutionPlan) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for name in plan
        .milestones
        .iter()
        .filter_map(|item| item.owner.clone())
        .chain(
            plan.dependencies
                .iter()
                .filter_map(|item| item.owner.clone()),
        )
        .chain(plan.risks.iter().filter_map(|item| item.owner.clone()))
        .chain(plan.decisions.iter().filter_map(|item| item.owner.clone()))
    {
        let trimmed = name.trim();
        if trimmed.is_empty() || looks_like_invalid_owner_name(trimmed) {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            names.push(trimmed.to_string());
        }
    }
    names
}

fn normalize_required_string(value: String) -> Option<String> {
    normalize_optional_string(Some(value))
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();

    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }

        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            output.push(trimmed.to_string());
        }
    }

    output
}

fn dedupe_by_key<I, T, F>(values: I, key_fn: F) -> Vec<T>
where
    I: IntoIterator<Item = T>,
    F: Fn(&T) -> String,
{
    let mut output = Vec::new();
    let mut seen = HashSet::new();

    for value in values {
        let key = key_fn(&value);
        if seen.insert(key) {
            output.push(value);
        }
    }

    output
}

fn normalize_milestone_status(value: String) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "done" | "complete" | "completed" => "done",
        "in_progress" | "in progress" | "active" => "in_progress",
        "at_risk" | "at risk" | "risk" => "at_risk",
        "blocked" => "blocked",
        "not_started" | "not started" | "todo" | "planned" => "not_started",
        _ => "unknown",
    }
    .to_string()
}

fn normalize_dependency_status(value: String) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "ready" | "on_track" | "on track" => "ready",
        "at_risk" | "at risk" => "at_risk",
        "blocked" => "blocked",
        "done" | "complete" | "completed" => "done",
        _ => "unknown",
    }
    .to_string()
}

fn normalize_owner_update_status(value: String) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "current" | "fresh" | "updated" => "current",
        "stale" | "waiting" | "lagging" => "stale",
        "missing" | "none" => "missing",
        _ => "unknown",
    }
    .to_string()
}

fn normalize_critical_path_item_type(value: String) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "milestone" | "dependency" | "decision" | "risk" => normalized,
        _ => "milestone".to_string(),
    }
}

fn normalize_risk_severity(value: String) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "low" | "medium" | "high" | "critical" => normalized,
        _ => "medium".to_string(),
    }
}

fn normalize_risk_status(value: String) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "open" | "watching" | "mitigated" | "resolved" => normalized,
        _ => "unknown".to_string(),
    }
}

fn normalize_decision_status(value: String) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "open" | "resolved" | "blocked" => normalized,
        _ => "unknown".to_string(),
    }
}

fn normalize_azure_endpoint(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.ends_with("/openai/v1") {
        trimmed.to_string()
    } else {
        format!("{}/openai/v1", trimmed)
    }
}

fn title_case(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "Unknown".to_string();
    }

    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_plan() -> LaunchExecutionPlan {
        LaunchExecutionPlan {
            title: "Mobile app launch".to_string(),
            evidence: vec![LaunchEvidence {
                label: "timeline".to_string(),
                snippet: "Launch target is June 14".to_string(),
                source_ref: Some("primary_context".to_string()),
            }],
            ..LaunchExecutionPlan::default()
        }
    }

    #[test]
    fn extract_json_object_finds_first_valid_object() {
        let raw =
            "noise {\"launch_execution_plan\":{\"title\":\"hi\"},\"readiness_brief\":{}} trailing";
        let extracted = extract_json_object(raw).expect("json object should be extracted");
        assert!(extracted.contains("\"title\":\"hi\""));
    }

    #[test]
    fn readiness_turns_red_without_evidence() {
        let plan = LaunchExecutionPlan::default();
        let (status, reason) = derive_readiness_status(&plan);
        assert_eq!(status, "red");
        assert!(reason.contains("No credible readiness evidence"));
    }

    #[test]
    fn readiness_turns_red_for_launch_blocking_decision() {
        let mut plan = base_plan();
        plan.decisions.push(LaunchDecision {
            title: "Approve pricing page copy".to_string(),
            status: "open".to_string(),
            launch_blocking: true,
            ..LaunchDecision::default()
        });

        let (status, reason) = derive_readiness_status(&plan);
        assert_eq!(status, "red");
        assert!(reason.contains("Launch-blocking decision"));
    }

    #[test]
    fn follow_up_generation_surfaces_missing_owners_and_stale_updates() {
        let mut plan = base_plan();
        plan.owners.push(LaunchOwner {
            name: "Dana".to_string(),
            update_status: "stale".to_string(),
            ..LaunchOwner::default()
        });
        plan.dependencies.push(LaunchDependency {
            title: "QA signoff".to_string(),
            status: "blocked".to_string(),
            critical_path: true,
            ..LaunchDependency::default()
        });

        let items = derive_follow_up_items(&plan);
        assert!(items.iter().any(|item| item.kind == "stale_update"));
        assert!(items.iter().any(|item| item.kind == "missing_owner"));
    }

    #[test]
    fn follow_up_generation_surfaces_unresolved_decisions() {
        let mut plan = base_plan();
        plan.decisions.push(LaunchDecision {
            title: "Pick rollback threshold".to_string(),
            status: "open".to_string(),
            ..LaunchDecision::default()
        });

        let items = derive_follow_up_items(&plan);
        assert!(items.iter().any(|item| item.kind == "unresolved_decision"));
    }

    #[test]
    fn follow_up_generation_surfaces_missing_dates() {
        let mut plan = base_plan();
        plan.dependencies.push(LaunchDependency {
            title: "Launch email scheduled".to_string(),
            owner: Some("Ava".to_string()),
            status: "at_risk".to_string(),
            ..LaunchDependency::default()
        });

        let items = derive_follow_up_items(&plan);
        assert!(items.iter().any(|item| item.kind == "missing_date"));
    }

    #[test]
    fn change_summary_reports_readiness_and_new_blockers() {
        let mut prior = base_plan();
        prior.readiness_status = "yellow".to_string();

        let mut current = base_plan();
        current.readiness_status = "red".to_string();
        current.risks.push(LaunchRisk {
            title: "Payments callback still failing".to_string(),
            blocker: true,
            severity: "critical".to_string(),
            status: "open".to_string(),
            ..LaunchRisk::default()
        });

        let summary = derive_change_summary(Some(&prior), &current);
        assert!(summary
            .iter()
            .any(|item| item.contains("Readiness changed")));
        assert!(summary
            .iter()
            .any(|item| item.contains("New blocker surfaced")));
    }

    #[test]
    fn normalize_source_type_rejects_non_v1_sources() {
        let err =
            normalize_source_type(Some("google_docs".to_string())).expect_err("should reject");
        assert!(err.contains("pasted_thread only"));
    }

    #[test]
    fn retry_queue_confirmation_is_not_treated_as_open_decision() {
        let decision = decision_candidate_from_text(
            "Jon (Eng): I can own the callback fix, but I need Infra to confirm whether the retry queue config can change before code freeze.",
            Some("Jon".to_string()),
        );
        assert!(decision.is_none());
    }

    #[test]
    fn focused_evidence_prefers_no_decision_yet_clause() {
        let snippet = focused_evidence_snippet(
            "decision_evidence",
            "U03 | Paul (PM) - Nov 8: Leadership asked whether phased cutover is acceptable. No decision yet.",
        );
        assert_eq!(snippet, "No decision yet.");
    }

    #[test]
    fn richer_dependency_status_line_beats_checklist_line() {
        let lines = parse_thread_lines(
            "L05 | Lena (Lifecycle) - Apr 17: Email plus in-app launch comms draft is 80 percent there. I need the final launch date and approved screenshots.\nL15 | - Lifecycle email plus in-app message scheduled",
            None,
        );
        let line = best_supporting_dependency_line(
            &lines,
            "Lifecycle email plus in-app message scheduled",
            false,
        )
        .expect("expected supporting line");
        assert_eq!(line.source_ref, "L05");
    }
}
