use std::fs;
use std::path::Path;

use super::claude::run_claude_task;
use super::codex::run_codex_task;
use super::env::read_env_trimmed;
use super::errors::RunTaskError;
use super::trace::RUN_TASK_TRACE_DIRNAME;
use super::types::{RunTaskOutput, RunTaskParams, RunTaskRequest};
use super::workspace::{prepare_workspace, remap_workspace_dir, write_placeholder_reply};

pub fn run_task(params: &RunTaskParams) -> Result<RunTaskOutput, RunTaskError> {
    let workspace_dir = remap_workspace_dir(&params.workspace_dir)?;
    let runner = normalize_runner(&params.runner);
    let request = build_request(&workspace_dir, params, params.model_name.as_str());
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

    let primary_result = match runner.as_str() {
        "claude" => run_claude_task(
            build_request(&workspace_dir, params, params.model_name.as_str()),
            &runner,
            reply_html_path.clone(),
            reply_attachments_dir.clone(),
            false,
        ),
        _ => run_codex_task(
            build_request(&workspace_dir, params, params.model_name.as_str()),
            &runner,
            reply_html_path.clone(),
            reply_attachments_dir.clone(),
        ),
    };

    match primary_result {
        Ok(output) => Ok(output),
        Err(primary_err) => run_claude_fallback_after_codex_failure(params, primary_err),
    }
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
            )?;
            Err(RunTaskError::FallbackFailed {
                primary: primary_err.to_string(),
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
    use super::is_codex_timeout_eligible_for_fallback;

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
}
