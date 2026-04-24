use std::collections::{BTreeSet, HashSet};
use std::env;
use std::time::Duration;

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-5.4";
const LLM_TIMEOUT: Duration = Duration::from_secs(45);
const DEFAULT_SOURCE_TYPE: &str = "pasted_thread";

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

#[derive(Debug, Clone, Deserialize)]
struct LlmLaunchExecutionOutput {
    #[serde(default, alias = "plan")]
    launch_execution_plan: LaunchExecutionPlan,
    #[serde(default, alias = "brief")]
    readiness_brief: LaunchReadinessBrief,
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
    let source_type = normalize_source_type(request.source_type)?;
    let context_text = request.context_text.trim().to_string();
    if context_text.is_empty() {
        return Err(
            "context_text must contain a pasted launch thread or planning bundle".to_string(),
        );
    }

    let config = LaunchExecutionLlmConfig::from_env()?;
    let system_prompt = launch_execution_system_prompt();
    let user_prompt = launch_execution_user_prompt(
        &source_type,
        request.source_label.as_deref(),
        &context_text,
        request.update_text.as_deref(),
        request.prior_plan.as_ref(),
    )?;
    let raw = call_chat_completion(&config, &system_prompt, &user_prompt).await?;
    let parsed = parse_llm_output(&raw)?;

    let mut plan = normalize_launch_execution_plan(
        parsed.launch_execution_plan,
        &source_type,
        request.source_label,
        request.prior_plan.as_ref(),
    );
    plan.follow_up_items = derive_follow_up_items(&plan);
    let (readiness_status, readiness_reason) = derive_readiness_status(&plan);
    plan.readiness_status = readiness_status;
    plan.readiness_reason = readiness_reason;

    let tracker_rows = build_tracker_rows(&plan);
    let change_summary = derive_change_summary(request.prior_plan.as_ref(), &plan);
    let readiness_brief = build_readiness_brief(parsed.readiness_brief, &plan, change_summary);

    Ok(LaunchExecutionResponse {
        launch_execution_plan: plan,
        tracker_rows,
        readiness_brief,
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

fn launch_execution_system_prompt() -> String {
    r#"You are Oliver's launch execution analyst.

Your only job is to turn one launch-related thread or planning bundle into a narrow launch execution record.

Scope rules:
- Focus only on launch execution.
- Do not generalize into a broad TPM system, generic project management, or broad automation advice.
- Never invent owners, dates, milestones, or status.
- If information is missing, leave the field null/empty and surface it through missing owner updates, decisions, risks, or other grounded fields.
- Preserve uncertainty. If a date is only a window, use launch_window instead of target_date.
- Evidence matters: every readiness judgment must be grounded in snippets from the provided context.

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
"#
    .to_string()
}

fn launch_execution_user_prompt(
    source_type: &str,
    source_label: Option<&str>,
    context_text: &str,
    update_text: Option<&str>,
    prior_plan: Option<&LaunchExecutionPlan>,
) -> Result<String, String> {
    let prior_plan_json = serde_json::to_string_pretty(&prior_plan.cloned().unwrap_or_default())
        .map_err(|err| format!("failed to serialize prior launch plan: {}", err))?;

    let source_label = source_label
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Untitled launch thread");

    let latest_update = update_text
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("None");

    Ok(format!(
        "Current UTC timestamp: {timestamp}
Supported source type for this request: {source_type}
Source label: {source_label}

Previous launch execution plan (may be empty on first run):
{prior_plan_json}

Primary launch context:
{context_text}

Latest incremental update:
{latest_update}

Instructions:
1. Extract the narrowest believable launch execution record from the provided context.
2. If a prior plan is provided, refresh it using the latest context and preserve stable titles when the same item still exists.
3. Treat missing owners, missing dates, stale updates, unresolved blockers, and unresolved decisions as first-class output signals.
4. Keep evidence visible and specific.",
        timestamp = Utc::now().to_rfc3339(),
        source_type = source_type,
        source_label = source_label,
        prior_plan_json = prior_plan_json,
        context_text = context_text,
        latest_update = latest_update
    ))
}

async fn call_chat_completion(
    config: &LaunchExecutionLlmConfig,
    system_prompt: &str,
    user_prompt: &str,
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
        max_completion_tokens: 2600,
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
            item.owner = normalize_optional_string(item.owner);
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
            item.owner = normalize_optional_string(item.owner);
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
            item.owner = normalize_optional_string(item.owner);
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

    if output.len() > 8 {
        output.truncate(8);
    }
    output
}

fn derive_follow_up_items(plan: &LaunchExecutionPlan) -> Vec<LaunchFollowUpItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

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

    for item in &plan.milestones {
        if item.status != "done" && item.owner.is_none() {
            push_follow_up(
                &mut items,
                &mut seen,
                build_missing_owner_follow_up(
                    "milestone",
                    &item.title,
                    item.critical_path,
                    item.notes.as_deref(),
                ),
            );
        }
        if item.status != "done" && item.target_date.is_none() {
            push_follow_up(
                &mut items,
                &mut seen,
                build_missing_date_follow_up(
                    "milestone",
                    &item.title,
                    item.owner.as_deref(),
                    item.critical_path,
                ),
            );
        }
    }

    for item in &plan.dependencies {
        if item.status != "done" && item.owner.is_none() {
            push_follow_up(
                &mut items,
                &mut seen,
                build_missing_owner_follow_up(
                    "dependency",
                    &item.title,
                    item.critical_path,
                    item.notes.as_deref(),
                ),
            );
        }
        if item.status != "done" && item.target_date.is_none() {
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
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "unresolved_blocker".to_string(),
                    target: item.title.clone(),
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
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "unresolved_blocker".to_string(),
                    target: item.title.clone(),
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
        if item.status != "resolved" {
            push_follow_up(
                &mut items,
                &mut seen,
                LaunchFollowUpItem {
                    kind: "unresolved_decision".to_string(),
                    target: item.title.clone(),
                    owner: item.owner.clone(),
                    priority: if item.launch_blocking { "high" } else { "medium" }.to_string(),
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
        if item.status != "resolved" && item.due_date.is_none() {
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
        if item.status != "resolved" && item.owner.is_none() {
            push_follow_up(
                &mut items,
                &mut seen,
                build_missing_owner_follow_up(
                    "decision",
                    &item.title,
                    item.launch_blocking,
                    item.notes.as_deref(),
                ),
            );
        }
    }

    items
}

fn build_missing_owner_follow_up(
    category: &str,
    title: &str,
    high_priority: bool,
    notes: Option<&str>,
) -> LaunchFollowUpItem {
    LaunchFollowUpItem {
        kind: "missing_owner".to_string(),
        target: format!("{}: {}", category, title),
        owner: None,
        priority: if high_priority { "high" } else { "medium" }.to_string(),
        reason: notes.map(str::to_string).unwrap_or_else(|| {
            format!(
                "{} is present in the launch plan but has no clear owner.",
                title
            )
        }),
        suggested_message: format!(
            "Who owns \"{}\" for launch, and who should Oliver follow up with next?",
            title
        ),
    }
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
        target: format!("{}: {}", category, title),
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

fn derive_readiness_status(plan: &LaunchExecutionPlan) -> (String, String) {
    if plan.evidence.is_empty() {
        return (
            "red".to_string(),
            "No credible readiness evidence was extracted from the source context.".to_string(),
        );
    }

    if let Some(risk) = plan.risks.iter().find(|item| {
        item.blocker
            && matches!(item.severity.as_str(), "high" | "critical")
            && !matches!(item.status.as_str(), "resolved" | "mitigated")
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
            "missing_date"
                | "missing_owner"
                | "stale_update"
                | "unresolved_blocker"
                | "unresolved_decision"
        )
    }) || plan
        .dependencies
        .iter()
        .any(|item| matches!(item.status.as_str(), "at_risk" | "blocked"))
        || plan
            .risks
            .iter()
            .any(|item| matches!(item.status.as_str(), "open" | "watching"))
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
        if item.status == "blocked" {
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

    if brief.critical_blockers.is_empty() {
        brief.critical_blockers = blocker_titles(plan).into_iter().collect();
    } else {
        brief.critical_blockers = normalize_string_list(brief.critical_blockers);
    }

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

    if brief.open_decisions.is_empty() {
        brief.open_decisions = plan
            .decisions
            .iter()
            .filter(|item| item.status != "resolved")
            .map(|item| item.title.clone())
            .collect();
    } else {
        brief.open_decisions = normalize_string_list(brief.open_decisions);
    }

    if brief.missing_owner_updates.is_empty() {
        brief.missing_owner_updates = plan
            .owners
            .iter()
            .filter(|item| matches!(item.update_status.as_str(), "missing" | "stale"))
            .map(|item| item.name.clone())
            .collect();
    } else {
        brief.missing_owner_updates = normalize_string_list(brief.missing_owner_updates);
    }

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
        if trimmed.is_empty() {
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
    fn follow_up_generation_surfaces_missing_owners_dates_and_stale_updates() {
        let mut plan = base_plan();
        plan.owners.push(LaunchOwner {
            name: "Dana".to_string(),
            update_status: "stale".to_string(),
            ..LaunchOwner::default()
        });
        plan.milestones.push(LaunchMilestone {
            title: "Finish QA signoff".to_string(),
            critical_path: true,
            status: "in_progress".to_string(),
            ..LaunchMilestone::default()
        });
        plan.decisions.push(LaunchDecision {
            title: "Pick rollback threshold".to_string(),
            status: "open".to_string(),
            ..LaunchDecision::default()
        });

        let items = derive_follow_up_items(&plan);
        assert!(items.iter().any(|item| item.kind == "stale_update"));
        assert!(items.iter().any(|item| item.kind == "missing_owner"));
        assert!(items.iter().any(|item| item.kind == "missing_date"));
        assert!(items.iter().any(|item| item.kind == "unresolved_decision"));
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
}
