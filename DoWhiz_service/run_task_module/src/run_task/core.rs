use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::claude::run_claude_task;
use super::codex::{
    codex_command_timeout, run_codex_task, run_codex_task_with_fast_completion,
    run_codex_task_with_timeout,
};
use super::env::read_env_trimmed;
use super::errors::RunTaskError;
use super::investment_fail_soft::maybe_write_investment_operational_failure_artifact;
use super::reply_contract::{
    investment_monitor_request_for_workspace, investment_request_for_workspace,
};
use super::trace::RUN_TASK_TRACE_DIRNAME;
use super::types::{RunTaskOutput, RunTaskParams, RunTaskRequest};
use super::utils::split_reply_completion_budget;
use super::workspace::{prepare_workspace, remap_workspace_dir, write_placeholder_reply};

const MAX_INVESTMENT_MONITOR_PRIMARY_TIMEOUT_SECS: u64 = 30;
const MAX_INVESTMENT_MONITOR_FAST_COMPLETION_TIMEOUT_SECS: u64 = 20;
const MAX_INVESTMENT_RESEARCH_PRIMARY_TIMEOUT_SECS: u64 = 90;
const MAX_INVESTMENT_RESEARCH_FAST_COMPLETION_TIMEOUT_SECS: u64 = 30;
const MAX_INVESTMENT_CONTENT_FILTER_MONITOR_RETRY_TIMEOUT_SECS: u64 = 20;
const MAX_INVESTMENT_CONTENT_FILTER_RESEARCH_RETRY_TIMEOUT_SECS: u64 = 45;

pub fn run_task(params: &RunTaskParams) -> Result<RunTaskOutput, RunTaskError> {
    let workspace_dir = remap_workspace_dir(&params.workspace_dir)?;
    let runner = normalize_runner(&params.runner);
    let request = build_request(&workspace_dir, params, params.model_name.as_str());
    let investment_request = investment_request_for_workspace(&workspace_dir)?;

    let (reply_html_path, reply_attachments_dir) = prepare_workspace(&request)?;

    if params.codex_disabled {
        if !params.reply_to.is_empty() {
            write_placeholder_reply(&reply_html_path)?;
        }
        return Ok(RunTaskOutput {
            reply_html_path,
            reply_attachments_dir,
            codex_output: "codex disabled".to_string(),
            scheduled_tasks: Vec::new(),
            scheduled_tasks_error: None,
            scheduler_actions: Vec::new(),
            scheduler_actions_error: None,
            token_usage: None,
            recovery_note: None,
        });
    }

    let codex_budget_split = codex_fast_completion_budget_split(&runner, &request)?;
    let primary_result = match runner.as_str() {
        "claude" => run_claude_task(
            build_request(&workspace_dir, params, params.model_name.as_str()),
            &runner,
            reply_html_path.clone(),
            reply_attachments_dir.clone(),
            false,
        ),
        _ => match codex_budget_split {
            Some((primary_timeout, _)) => run_codex_task_with_timeout(
                build_request(&workspace_dir, params, params.model_name.as_str()),
                &runner,
                reply_html_path.clone(),
                reply_attachments_dir.clone(),
                Some(primary_timeout),
            ),
            None => run_codex_task(
                build_request(&workspace_dir, params, params.model_name.as_str()),
                &runner,
                reply_html_path.clone(),
                reply_attachments_dir.clone(),
            ),
        },
    };

    match primary_result {
        Ok(output) => Ok(output),
        Err(primary_err) => {
            let mut fallback_primary_err = primary_err;
            let mut allow_fast_completion_retry = codex_budget_split.is_some();
            if runner.eq_ignore_ascii_case("codex")
                && investment_request
                && !params.reply_to.is_empty()
                && is_content_filter_failure(&fallback_primary_err)
            {
                match run_investment_content_filter_retry(
                    params,
                    &workspace_dir,
                    &runner,
                    &fallback_primary_err,
                ) {
                    Ok(output) => return Ok(output),
                    Err(retry_err) => {
                        fallback_primary_err = retry_err;
                        allow_fast_completion_retry = false;
                    }
                }
            }
            if runner.eq_ignore_ascii_case("codex")
                && !investment_request
                && !params.reply_to.is_empty()
                && is_content_filter_failure(&fallback_primary_err)
                && has_tool_failures_file(&workspace_dir)
            {
                match run_tool_failure_content_filter_retry(
                    params,
                    &workspace_dir,
                    &runner,
                    &fallback_primary_err,
                ) {
                    Ok(output) => return Ok(output),
                    Err(retry_err) => {
                        // Retry also failed - send generic content filter reply
                        if let Some(output) = maybe_finalize_generic_content_filter_reply(
                            params,
                            &workspace_dir,
                            &retry_err,
                        )? {
                            return Ok(output);
                        }
                        fallback_primary_err = retry_err;
                    }
                }
            }
            if allow_fast_completion_retry {
                if let Some((_, fast_completion_timeout)) = codex_budget_split {
                    match maybe_run_codex_fast_completion_retry(
                        params,
                        &workspace_dir,
                        &runner,
                        &fallback_primary_err,
                        fast_completion_timeout,
                    ) {
                        Ok(Some(output)) => return Ok(output),
                        Ok(None) => {}
                        Err(retry_err) => fallback_primary_err = retry_err,
                    }
                }
            }
            match run_claude_fallback_after_codex_failure(params, fallback_primary_err) {
                Ok(output) => Ok(output),
                Err(fallback_err) => {
                    if let Some(output) = maybe_finalize_investment_operational_failure_reply(
                        params,
                        &workspace_dir,
                        &fallback_err,
                    )? {
                        Ok(output)
                    } else {
                        Err(fallback_err)
                    }
                }
            }
        }
    }
}

fn investment_content_filter_retry_timeout(workspace_dir: &Path) -> Result<Duration, RunTaskError> {
    let cap_secs = if investment_monitor_request_for_workspace(workspace_dir)? {
        MAX_INVESTMENT_CONTENT_FILTER_MONITOR_RETRY_TIMEOUT_SECS
    } else {
        MAX_INVESTMENT_CONTENT_FILTER_RESEARCH_RETRY_TIMEOUT_SECS
    };

    Ok(codex_command_timeout().min(Duration::from_secs(cap_secs)))
}

fn is_content_filter_failure(err: &RunTaskError) -> bool {
    let output = match err {
        RunTaskError::CodexFailed { output, .. }
        | RunTaskError::ClaudeFailed { output, .. }
        | RunTaskError::CommandTimeout { output, .. }
        | RunTaskError::OutputMissing { output, .. }
        | RunTaskError::OutputContractViolation { output, .. } => output,
        RunTaskError::FallbackFailed { primary, fallback } => {
            return output_looks_like_content_filter_failure(primary)
                || output_looks_like_content_filter_failure(fallback);
        }
        _ => return false,
    };

    output_looks_like_content_filter_failure(output)
}

fn maybe_finalize_generic_content_filter_reply(
    params: &RunTaskParams,
    workspace_dir: &Path,
    cause: &RunTaskError,
) -> Result<Option<RunTaskOutput>, RunTaskError> {
    if params.reply_to.is_empty() {
        return Ok(None);
    }

    let request = build_request(workspace_dir, params, params.model_name.as_str());
    let (reply_path, reply_attachments_dir) = prepare_workspace(&request)?;
    let lower_channel = params.channel.trim().to_ascii_lowercase();
    let reply_body = match lower_channel.as_str() {
        "slack" | "discord" | "telegram" | "sms" | "whatsapp" | "bluebubbles" | "lark"
        | "wechat" | "wechat_mp" | "notion" => {
            "I couldn't show the requested result because the Azure/OpenAI content filter blocked this run.\nPlease revise the prompt and send a new request."
                .to_string()
        }
        _ => r#"<html><body><p>I couldn't show the requested result because the Azure/OpenAI content filter blocked this run.</p><p>Please revise the prompt and send a new request.</p></body></html>"#.to_string(),
    };
    fs::write(&reply_path, reply_body)?;

    Ok(Some(RunTaskOutput {
        reply_html_path: reply_path,
        reply_attachments_dir,
        codex_output: cause.to_string(),
        scheduled_tasks: Vec::new(),
        scheduled_tasks_error: None,
        scheduler_actions: Vec::new(),
        scheduler_actions_error: None,
        token_usage: None,
        recovery_note: Some(
            "Returned an explicit Azure/OpenAI content-filter explanation to the user instead of retrying hidden output."
                .to_string(),
        ),
    }))
}

fn output_looks_like_content_filter_failure(output: &str) -> bool {
    let normalized = output.to_ascii_lowercase();
    normalized.contains("content_filter")
        || normalized.contains("reason: content_filter")
        || normalized.contains("stream disconnected before completion")
        || normalized.contains("incomplete response returned")
        || normalized.contains("i'm sorry, but i cannot assist with that request")
        || normalized.contains("i cannot assist with that request")
}

fn has_tool_failures_file(workspace_dir: &Path) -> bool {
    workspace_dir.join(".tool_failures.jsonl").exists()
}

fn run_tool_failure_content_filter_retry(
    params: &RunTaskParams,
    workspace_dir: &Path,
    runner: &str,
    primary_err: &RunTaskError,
) -> Result<RunTaskOutput, RunTaskError> {
    let retry_timeout = Duration::from_secs(60);
    let request = build_request(workspace_dir, params, params.model_name.as_str());
    let (reply_html_path, reply_attachments_dir) = prepare_workspace(&request)?;
    let mut output = run_codex_task_with_fast_completion(
        build_request(workspace_dir, params, params.model_name.as_str()),
        runner,
        reply_html_path,
        reply_attachments_dir,
        Some(retry_timeout),
    )?;
    let retry_note = format!(
        "Recovered via tool-failure-aware Codex retry after primary failure ({})",
        primary_error_summary(primary_err)
    );
    output.recovery_note = Some(match output.recovery_note.take() {
        Some(existing) => format!("{}\n{}", existing, retry_note),
        None => retry_note,
    });
    Ok(output)
}

fn run_investment_content_filter_retry(
    params: &RunTaskParams,
    workspace_dir: &Path,
    runner: &str,
    primary_err: &RunTaskError,
) -> Result<RunTaskOutput, RunTaskError> {
    let retry_timeout = investment_content_filter_retry_timeout(workspace_dir)?;
    write_investment_content_filter_retry_context(workspace_dir, primary_err, retry_timeout)?;
    let request = build_request(workspace_dir, params, params.model_name.as_str());
    let (reply_html_path, reply_attachments_dir) = prepare_workspace(&request)?;
    let mut output = run_codex_task_with_fast_completion(
        build_request(workspace_dir, params, params.model_name.as_str()),
        runner,
        reply_html_path,
        reply_attachments_dir,
        Some(retry_timeout),
    )?;
    let retry_note = format!(
        "Recovered via one content-filter-safe Codex retry after primary failure ({})",
        primary_error_summary(primary_err)
    );
    output.recovery_note = Some(match output.recovery_note.take() {
        Some(existing) => format!("{}\n{}", existing, retry_note),
        None => retry_note,
    });
    Ok(output)
}

fn write_investment_content_filter_retry_context(
    workspace_dir: &Path,
    primary_err: &RunTaskError,
    retry_timeout: Duration,
) -> Result<(), RunTaskError> {
    let context = format!(
        "# Codex fast-completion context\n\n\
This workspace is running exactly one bounded retry because the earlier pass was blocked by provider content filtering.\n\n\
Remaining retry budget: approximately {budget_secs} seconds.\n\n\
Primary failure class: {primary_summary}\n\n\
Required behavior now:\n\
- Normalize the user's request into a neutral public-company analysis task before doing any other work.\n\
- Do not repeat roleplay framing such as `Wall Street trader`, `deep research`, `investment advice`, or open-ended buy/sell solicitation language in your own task framing.\n\
- Continue the real analysis path. Do not collapse the request into canned monitor labels or a pseudo-answer.\n\
- Reuse existing workspace evidence first and add only the minimum targeted verification needed to finish.\n\
- If the original request was a deep-research memo, still aim for a structured substantive investment reply.\n\
- If the original request was a monitor or delta check, you may keep the reply concise, but it still must reflect real analysis rather than a placeholder status.\n\
- Produce exactly one final HTML reply in `reply_email_draft.html`.\n\
- If you cannot support a conclusion, say exactly what remains unverified and why. Do not emit canned shells like `Unable to Verify`, `No Material Update`, `Actionable Update`, or `No recommendation` just to end the run.\n\
- Do not restart a broad filing scrape, long quote sweep, or repeated market-price loop.\n\
- Do not mention internal policy machinery unless the user explicitly asks why the system could not answer.\n",
        budget_secs = retry_timeout.as_secs(),
        primary_summary = primary_error_summary(primary_err),
    );
    fs::write(
        workspace_dir.join("codex_fast_completion_context.md"),
        context,
    )?;
    Ok(())
}

fn maybe_finalize_investment_operational_failure_reply(
    params: &RunTaskParams,
    workspace_dir: &Path,
    cause: &RunTaskError,
) -> Result<Option<RunTaskOutput>, RunTaskError> {
    if params.reply_to.is_empty() {
        return Ok(None);
    }

    let request = build_request(workspace_dir, params, params.model_name.as_str());
    let (reply_html_path, reply_attachments_dir) = prepare_workspace(&request)?;
    let Some(recovery_note) = maybe_write_investment_operational_failure_artifact(
        workspace_dir,
        &reply_html_path,
        cause,
    )?
    else {
        return Ok(None);
    };

    Ok(Some(RunTaskOutput {
        reply_html_path,
        reply_attachments_dir,
        codex_output: cause.to_string(),
        scheduled_tasks: Vec::new(),
        scheduled_tasks_error: None,
        scheduler_actions: Vec::new(),
        scheduler_actions_error: None,
        token_usage: None,
        recovery_note: Some(recovery_note),
    }))
}

pub fn run_claude_fallback_after_codex_failure(
    params: &RunTaskParams,
    primary_err: RunTaskError,
) -> Result<RunTaskOutput, RunTaskError> {
    let runner = normalize_runner(&params.runner);
    if !should_fallback_to_claude(&runner, &primary_err) {
        return Err(primary_err);
    }

    let workspace_dir = remap_workspace_dir(&params.workspace_dir)?;
    let request = build_request(&workspace_dir, params, params.model_name.as_str());
    let (reply_html_path, reply_attachments_dir) = prepare_workspace(&request)?;
    let fallback_model = resolve_claude_fallback_model(params.model_name.as_str());
    archive_primary_codex_trace(&workspace_dir)?;
    let archived_primary_reply = archive_primary_reply_artifact(&workspace_dir, &reply_html_path)?;
    reset_reply_artifacts(&reply_html_path, &reply_attachments_dir)?;
    let fallback_result = run_claude_task(
        build_request(&workspace_dir, params, fallback_model.as_str()),
        "claude",
        reply_html_path,
        reply_attachments_dir,
        true,
    );

    match fallback_result {
        Ok(mut output) => {
            let note = build_claude_fallback_note(&primary_err, fallback_model.as_str());
            write_fallback_note(
                &workspace_dir,
                "success",
                fallback_model.as_str(),
                &primary_err,
                None,
                archived_primary_reply.as_deref(),
            )?;
            output.recovery_note = Some(match output.recovery_note.take() {
                Some(existing) => format!("{}\n{}", existing, note),
                None => note,
            });
            Ok(output)
        }
        Err(fallback_err) => {
            write_fallback_note(
                &workspace_dir,
                "failed",
                fallback_model.as_str(),
                &primary_err,
                Some(&fallback_err),
                archived_primary_reply.as_deref(),
            )?;
            Err(RunTaskError::FallbackFailed {
                primary: primary_error_with_archived_reply(
                    &primary_err,
                    archived_primary_reply.as_deref(),
                ),
                fallback: fallback_err.to_string(),
            })
        }
    }
}

fn normalize_runner(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "codex".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn is_codex_timeout_eligible_for_fallback(command: &str) -> bool {
    matches!(
        command,
        "codex" | "docker" | "docker run" | "az container create" | "az container show"
    )
}

fn build_request<'a>(
    workspace_dir: &'a Path,
    params: &'a RunTaskParams,
    model_name: &'a str,
) -> RunTaskRequest<'a> {
    RunTaskRequest {
        workspace_dir,
        input_email_dir: &params.input_email_dir,
        input_attachments_dir: &params.input_attachments_dir,
        memory_dir: &params.memory_dir,
        reference_dir: &params.reference_dir,
        model_name,
        reply_to: &params.reply_to,
        channel: &params.channel,
        google_access_token: params.google_access_token.as_deref(),
        notion_access_token: params.notion_access_token.as_deref(),
        has_unified_account: params.has_unified_account,
        user_identities: &params.user_identities,
        thread_epoch: params.thread_epoch,
        thread_state_path: params.thread_state_path.as_deref(),
    }
}

fn codex_fast_completion_budget_split(
    runner: &str,
    request: &RunTaskRequest<'_>,
) -> Result<Option<(Duration, Duration)>, RunTaskError> {
    if !runner.eq_ignore_ascii_case("codex") || request.reply_to.is_empty() {
        return Ok(None);
    }
    if !investment_request_for_workspace(request.workspace_dir)? {
        return Ok(None);
    }

    let total_budget = codex_command_timeout();
    if investment_monitor_request_for_workspace(request.workspace_dir)? {
        let desired_primary = Duration::from_secs(MAX_INVESTMENT_MONITOR_PRIMARY_TIMEOUT_SECS);
        let desired_reserve =
            Duration::from_secs(MAX_INVESTMENT_MONITOR_FAST_COMPLETION_TIMEOUT_SECS);
        if total_budget >= desired_primary + desired_reserve {
            return Ok(Some((desired_primary, desired_reserve)));
        }
    }

    let desired_primary = Duration::from_secs(MAX_INVESTMENT_RESEARCH_PRIMARY_TIMEOUT_SECS);
    let desired_reserve = Duration::from_secs(MAX_INVESTMENT_RESEARCH_FAST_COMPLETION_TIMEOUT_SECS);
    if total_budget >= desired_primary + desired_reserve {
        return Ok(Some((desired_primary, desired_reserve)));
    }

    let Some((primary_timeout, reserve_timeout)) = split_reply_completion_budget(total_budget)
    else {
        return Ok(None);
    };

    if investment_monitor_request_for_workspace(request.workspace_dir)? {
        return Ok(Some((primary_timeout, reserve_timeout)));
    }

    Ok(Some((primary_timeout, reserve_timeout)))
}

fn should_retry_codex_fast_completion(primary_err: &RunTaskError) -> bool {
    match primary_err {
        RunTaskError::CommandTimeout { command, .. } => {
            matches!(
                *command,
                "codex" | "docker" | "docker run" | "az container create" | "az container show"
            )
        }
        RunTaskError::OutputMissing { .. } | RunTaskError::OutputContractViolation { .. } => true,
        _ => false,
    }
}

fn write_codex_fast_completion_context(
    workspace_dir: &Path,
    primary_err: &RunTaskError,
    fast_completion_timeout: Duration,
) -> Result<(), RunTaskError> {
    let reply_path = workspace_dir.join("reply_email_draft.html");
    let reply_status = if reply_path.exists() {
        format!("Existing draft to inspect first: {}", reply_path.display())
    } else {
        "No existing reply draft was preserved from the earlier pass.".to_string()
    };
    let trace_hint = if workspace_dir.join(".run_task_trace_codex_primary").exists() {
        ".run_task_trace_codex_primary/"
    } else if workspace_dir.join(RUN_TASK_TRACE_DIRNAME).exists() {
        ".run_task_trace/"
    } else {
        "(no prior trace directory preserved)"
    };
    let context = format!(
        "# Codex fast-completion context\n\n\
This workspace is running a second Codex pass because the earlier pass did not finish with a deliverable inside its research budget.\n\n\
Remaining drafting budget: approximately {budget_secs} seconds.\n\n\
Primary failure class: {primary_summary}\n\n\
Artifact status:\n- {reply_status}\n- Prior trace to reuse if needed: {trace_hint}\n\n\
Required behavior now:\n- Use existing workspace evidence first. Do not restart the same search loop that already consumed the research budget.\n\
- Create or update `reply_email_draft.html` before any new broad research.\n\
- If this is an investment monitor request, keep the reply concise and decision-first, but still base it on real analysis.\n\
- If a single missing fact still blocks the verdict, do at most one targeted follow-up check after the draft exists.\n\
- Reuse evidence already gathered in this workspace.\n\
- Do not restart broad page-by-page annual-report extraction.\n\
- If evidence is incomplete, say so explicitly and finalize the best calibrated artifact now.\n\
- For real-ticker investment work, finish a complete final artifact instead of a provisional shell. Fill every required section, include concrete upgrade / downgrade / invalidation triggers, and remove `still being finalized`, `TBD`, or similar placeholders before you stop.\n\
- Do not fall back to canned labels like `Unable to Verify`, `No Material Update`, or `No recommendation` just to end the run.\n\
- Do not refuse solely because the request concerns stock analysis or a synthetic investment-monitor scenario. If certainty is limited, answer with calibrated uncertainty instead of refusal.\n",
        budget_secs = fast_completion_timeout.as_secs(),
        primary_summary = primary_error_summary(primary_err),
        reply_status = reply_status,
        trace_hint = trace_hint,
    );
    fs::write(
        workspace_dir.join("codex_fast_completion_context.md"),
        context,
    )?;
    Ok(())
}

fn maybe_run_codex_fast_completion_retry(
    params: &RunTaskParams,
    workspace_dir: &Path,
    runner: &str,
    primary_err: &RunTaskError,
    fast_completion_timeout: Duration,
) -> Result<Option<RunTaskOutput>, RunTaskError> {
    if !should_retry_codex_fast_completion(primary_err) {
        return Ok(None);
    }

    write_codex_fast_completion_context(workspace_dir, primary_err, fast_completion_timeout)?;
    let request = build_request(workspace_dir, params, params.model_name.as_str());
    let (reply_html_path, reply_attachments_dir) = prepare_workspace(&request)?;
    let mut output = run_codex_task_with_fast_completion(
        build_request(workspace_dir, params, params.model_name.as_str()),
        runner,
        reply_html_path,
        reply_attachments_dir,
        Some(fast_completion_timeout),
    )?;

    let retry_note = format!(
        "Recovered via second Codex fast-completion pass after primary Codex failure. Primary error: {}",
        primary_error_summary(primary_err)
    );
    output.recovery_note = Some(match output.recovery_note.take() {
        Some(existing) => format!("{}\n{}", existing, retry_note),
        None => retry_note,
    });
    Ok(Some(output))
}

fn should_fallback_to_claude(primary_runner: &str, err: &RunTaskError) -> bool {
    if !primary_runner.eq_ignore_ascii_case("codex") {
        return false;
    }

    match err {
        RunTaskError::CodexNotFound
        | RunTaskError::CodexFailed { .. }
        | RunTaskError::DockerNotFound
        | RunTaskError::DockerFailed { .. }
        | RunTaskError::AzureCliNotFound
        | RunTaskError::OutputMissing { .. }
        | RunTaskError::OutputContractViolation { .. } => true,
        RunTaskError::CommandTimeout { command, .. } => {
            is_codex_timeout_eligible_for_fallback(command)
        }
        _ => false,
    }
}

fn resolve_claude_fallback_model(primary_model_name: &str) -> String {
    if let Some(model) = read_env_trimmed("RUN_TASK_CODEX_FALLBACK_CLAUDE_MODEL") {
        return model;
    }

    let trimmed = primary_model_name.trim();
    if trimmed.to_ascii_lowercase().contains("claude") {
        trimmed.to_string()
    } else {
        read_env_trimmed("ANTHROPIC_DEFAULT_SONNET_MODEL")
            .unwrap_or_else(|| "claude-sonnet-4-5".to_string())
    }
}

fn archive_primary_codex_trace(workspace_dir: &Path) -> Result<(), RunTaskError> {
    let trace_dir = workspace_dir.join(RUN_TASK_TRACE_DIRNAME);
    if !trace_dir.exists() {
        return Ok(());
    }

    let archive_dir = workspace_dir.join(".run_task_trace_codex_primary");
    remove_path_if_exists(&archive_dir)?;
    fs::rename(trace_dir, archive_dir)?;
    Ok(())
}

fn archive_primary_reply_artifact(
    workspace_dir: &Path,
    reply_path: &Path,
) -> Result<Option<PathBuf>, RunTaskError> {
    if !reply_path.exists() {
        return Ok(None);
    }

    let archive_dir = workspace_dir.join(".run_task_trace_codex_primary");
    if !archive_dir.exists() {
        return Ok(None);
    }

    let preserved_dir = archive_dir.join("preserved_artifacts");
    fs::create_dir_all(&preserved_dir)?;
    let file_name = reply_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("reply_email_draft.html");
    let preserved_path = preserved_dir.join(file_name);
    fs::copy(reply_path, &preserved_path)?;
    Ok(Some(preserved_path))
}

fn reset_reply_artifacts(
    reply_path: &Path,
    reply_attachments_dir: &Path,
) -> Result<(), RunTaskError> {
    remove_path_if_exists(reply_path)?;
    remove_path_if_exists(reply_attachments_dir)?;
    fs::create_dir_all(reply_attachments_dir)?;
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<(), RunTaskError> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn build_claude_fallback_note(primary_err: &RunTaskError, fallback_model: &str) -> String {
    let model_note = if fallback_model.trim().is_empty() {
        "using the configured default Claude model".to_string()
    } else {
        format!("using Claude model {}", fallback_model.trim())
    };
    format!(
        "Recovered via Claude fallback after primary Codex failure ({}) {}",
        primary_error_summary(primary_err),
        model_note
    )
}

fn primary_error_with_archived_reply(
    primary_err: &RunTaskError,
    archived_reply: Option<&Path>,
) -> String {
    match archived_reply {
        Some(path) => format!(
            "{}\nPreserved primary Codex draft at {}",
            primary_err,
            path.display()
        ),
        None => primary_err.to_string(),
    }
}

fn primary_error_summary(err: &RunTaskError) -> &'static str {
    match err {
        RunTaskError::CodexNotFound => "Codex CLI not found",
        RunTaskError::CodexFailed { .. } => "Codex failed",
        RunTaskError::DockerNotFound => "Docker not found for Codex execution",
        RunTaskError::DockerFailed { .. } => "Docker-wrapped Codex execution failed",
        RunTaskError::AzureCliNotFound => "Azure CLI unavailable for Codex execution",
        RunTaskError::CommandTimeout { command, .. } if *command == "codex" => "Codex timed out",
        RunTaskError::CommandTimeout { command, .. }
            if *command == "docker" || *command == "docker run" =>
        {
            "Docker-wrapped Codex timed out"
        }
        RunTaskError::CommandTimeout { command, .. }
            if *command == "az container create" || *command == "az container show" =>
        {
            "Azure ACI Codex timed out"
        }
        RunTaskError::OutputMissing { .. } => {
            "Codex finished without writing the expected reply artifact"
        }
        RunTaskError::OutputContractViolation { .. } => {
            "Codex wrote a reply artifact that failed the required output contract"
        }
        _ => "Codex execution failed",
    }
}

fn write_fallback_note(
    workspace_dir: &Path,
    status: &str,
    fallback_model: &str,
    primary_err: &RunTaskError,
    fallback_err: Option<&RunTaskError>,
    archived_primary_reply: Option<&Path>,
) -> Result<(), RunTaskError> {
    let recovery_dir = workspace_dir.join(RUN_TASK_TRACE_DIRNAME).join("recovery");
    fs::create_dir_all(&recovery_dir)?;
    let mut body = String::new();
    body.push_str("primary_runner=codex\n");
    body.push_str("fallback_runner=claude\n");
    body.push_str(&format!("status={}\n", status));
    if fallback_model.trim().is_empty() {
        body.push_str("fallback_model=(default)\n");
    } else {
        body.push_str(&format!("fallback_model={}\n", fallback_model.trim()));
    }
    body.push_str("archived_primary_trace=.run_task_trace_codex_primary\n\n");
    if let Some(path) = archived_primary_reply {
        body.push_str(&format!("archived_primary_reply={}\n\n", path.display()));
    }
    body.push_str("primary_error:\n");
    body.push_str(&primary_err.to_string());
    body.push('\n');
    if let Some(err) = fallback_err {
        body.push_str("\nfallback_error:\n");
        body.push_str(&err.to_string());
        body.push('\n');
    }
    fs::write(recovery_dir.join("codex_to_claude_fallback.txt"), body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::env::acquire_env_test_lock;
    use super::super::types::{RunTaskRequest, UserIdentities};
    use super::codex_fast_completion_budget_split;
    use super::is_codex_timeout_eligible_for_fallback;
    use std::env;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn codex_timeout_fallback_covers_remote_and_docker_commands() {
        assert!(is_codex_timeout_eligible_for_fallback("codex"));
        assert!(is_codex_timeout_eligible_for_fallback("docker run"));
        assert!(is_codex_timeout_eligible_for_fallback(
            "az container create"
        ));
        assert!(is_codex_timeout_eligible_for_fallback("az container show"));
        assert!(!is_codex_timeout_eligible_for_fallback("az container logs"));
    }

    #[test]
    fn codex_fast_completion_budget_split_caps_monitor_primary_window() {
        let _lock = acquire_env_test_lock();
        let temp = TempDir::new().expect("tempdir");
        let incoming = temp.path().join("incoming_email");
        fs::create_dir_all(&incoming).expect("incoming dir");
        fs::write(
            incoming.join("thread_request.md"),
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.\n",
        )
        .expect("thread request");

        let prior_timeout = env::var_os("RUN_TASK_CODEX_TIMEOUT_SECS");
        env::set_var("RUN_TASK_CODEX_TIMEOUT_SECS", "480");

        let replies = vec!["user@example.com".to_string()];
        let identities = UserIdentities::default();
        let request = RunTaskRequest {
            workspace_dir: temp.path(),
            input_email_dir: Path::new("incoming_email"),
            input_attachments_dir: Path::new("incoming_attachments"),
            memory_dir: Path::new("memory"),
            reference_dir: Path::new("references"),
            model_name: "gpt-5.4",
            reply_to: &replies,
            channel: "email",
            google_access_token: None,
            notion_access_token: None,
            has_unified_account: true,
            user_identities: &identities,
            thread_epoch: None,
            thread_state_path: None,
        };

        let split = codex_fast_completion_budget_split("codex", &request)
            .expect("split")
            .expect("budget split");
        assert_eq!(split.0.as_secs(), 30);
        assert_eq!(split.1.as_secs(), 20);

        match prior_timeout {
            Some(value) => env::set_var("RUN_TASK_CODEX_TIMEOUT_SECS", value),
            None => env::remove_var("RUN_TASK_CODEX_TIMEOUT_SECS"),
        }
    }

    #[test]
    fn codex_fast_completion_budget_split_uses_fixed_monitor_window_for_shorter_budget() {
        let _lock = acquire_env_test_lock();
        let temp = TempDir::new().expect("tempdir");
        let incoming = temp.path().join("incoming_email");
        fs::create_dir_all(&incoming).expect("incoming dir");
        fs::write(
            incoming.join("thread_request.md"),
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.\n",
        )
        .expect("thread request");

        let prior_timeout = env::var_os("RUN_TASK_CODEX_TIMEOUT_SECS");
        env::set_var("RUN_TASK_CODEX_TIMEOUT_SECS", "120");

        let replies = vec!["user@example.com".to_string()];
        let identities = UserIdentities::default();
        let request = RunTaskRequest {
            workspace_dir: temp.path(),
            input_email_dir: Path::new("incoming_email"),
            input_attachments_dir: Path::new("incoming_attachments"),
            memory_dir: Path::new("memory"),
            reference_dir: Path::new("references"),
            model_name: "gpt-5.4",
            reply_to: &replies,
            channel: "email",
            google_access_token: None,
            notion_access_token: None,
            has_unified_account: true,
            user_identities: &identities,
            thread_epoch: None,
            thread_state_path: None,
        };

        let split = codex_fast_completion_budget_split("codex", &request)
            .expect("split")
            .expect("budget split");
        assert_eq!(split.0.as_secs(), 30);
        assert_eq!(split.1.as_secs(), 20);

        match prior_timeout {
            Some(value) => env::set_var("RUN_TASK_CODEX_TIMEOUT_SECS", value),
            None => env::remove_var("RUN_TASK_CODEX_TIMEOUT_SECS"),
        }
    }
}
