use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::constants::{CLAUDE_FOUNDRY_RESOURCE_DEFAULT, DEFAULT_CLAUDE_MODEL};

/// Check if cross-channel routing was requested and return the correct expected reply path.
/// If reply_routing.json specifies a different target channel, compute the expected file for that target.
fn resolve_expected_reply_path(workspace_dir: &Path, default_path: PathBuf) -> PathBuf {
    let routing_file = workspace_dir.join("reply_routing.json");
    if !routing_file.exists() {
        return default_path;
    }

    // Try to read and parse reply_routing.json
    let routing_content = match fs::read_to_string(&routing_file) {
        Ok(content) => content,
        Err(_) => return default_path,
    };

    // Parse the JSON to get target channel
    let routing: serde_json::Value = match serde_json::from_str(&routing_content) {
        Ok(v) => v,
        Err(_) => return default_path,
    };

    let target_channel = match routing.get("channel").and_then(|c| c.as_str()) {
        Some(ch) => ch.to_lowercase(),
        None => return default_path,
    };

    // Determine expected reply path based on TARGET channel
    match target_channel.as_str() {
        "email" | "googledocs" | "googlesheets" | "googleslides" => {
            workspace_dir.join("reply_email_draft.html")
        }
        "slack" | "discord" | "telegram" | "sms" | "whatsapp" | "bluebubbles" => {
            workspace_dir.join("reply_message.txt")
        }
        "notion" => {
            // Notion agent posts directly via API and creates .notion_api_replied marker
            workspace_dir.join(".notion_api_replied")
        }
        _ => default_path,
    }
}
use super::env::{load_env_sources, read_env_trimmed, remove_restricted_agent_env};
use super::errors::RunTaskError;
use super::github_auth::{ensure_github_cli_auth, resolve_github_auth};
use super::prompt::{build_prompt_with_fast_completion, load_memory_context};
use super::reply_contract::{ensure_expected_reply_artifact, reply_artifact_ready_for_workspace};
use super::scheduled::{extract_scheduled_tasks, extract_scheduler_actions};
use super::trace::RunTaskTraceRecorder;
use super::types::{RunTaskOutput, RunTaskRequest};
use super::utils::{
    run_command_with_timeout_and_cancel, run_task_timeout, tail_string, ThreadSupersedeMonitor,
};

const CLAUDE_ALLOWED_TOOLS: &str = "Read,Glob,Grep,Bash,Write,Edit,WebSearch,WebFetch,TodoWrite";
const DEFAULT_CLAUDE_FALLBACK_TIMEOUT_SECS: u64 = 900;
pub(super) fn run_claude_task(
    request: RunTaskRequest<'_>,
    runner: &str,
    reply_html_path: std::path::PathBuf,
    reply_attachments_dir: std::path::PathBuf,
    is_codex_fallback: bool,
) -> Result<RunTaskOutput, RunTaskError> {
    load_env_sources(request.workspace_dir)?;
    let github_auth = resolve_github_auth(None)?;
    let cancel_monitor = request
        .thread_epoch
        .zip(request.thread_state_path)
        .map(|(epoch, path)| ThreadSupersedeMonitor::new(path, epoch));

    let api_key =
        env::var("AZURE_OPENAI_API_KEY_BACKUP").map_err(|_| RunTaskError::MissingEnv {
            key: "AZURE_OPENAI_API_KEY_BACKUP",
        })?;
    if api_key.trim().is_empty() {
        return Err(RunTaskError::MissingEnv {
            key: "AZURE_OPENAI_API_KEY_BACKUP",
        });
    }

    let model_name = if request.model_name.trim().is_empty() {
        env::var("CLAUDE_MODEL").unwrap_or_else(|_| DEFAULT_CLAUDE_MODEL.to_string())
    } else {
        request.model_name.to_string()
    };

    let memory_context = load_memory_context(request.workspace_dir, request.memory_dir)?;
    let prompt = build_prompt_with_fast_completion(
        request.input_email_dir,
        request.input_attachments_dir,
        request.memory_dir,
        request.reference_dir,
        request.workspace_dir,
        runner,
        &memory_context,
        !request.reply_to.is_empty(),
        request.channel,
        request.has_unified_account,
        request.user_identities,
        is_codex_fallback,
    );

    ensure_github_cli_auth(&github_auth)?;
    let mut env_overrides = prepare_claude_env(&api_key, &model_name)?;
    env_overrides.extend(github_auth.env_overrides.clone());
    if let Some(askpass_path) = github_auth.askpass_path.as_ref() {
        env_overrides.push((
            "GIT_ASKPASS".to_string(),
            askpass_path.to_string_lossy().into_owned(),
        ));
        env_overrides.push(("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()));
    }
    let timeout = claude_task_timeout(is_codex_fallback);
    let mut trace = RunTaskTraceRecorder::new(
        request.workspace_dir,
        runner,
        "claude_local",
        &model_name,
        &prompt,
        timeout,
        serde_json::json!({
            "protocol": "claude_stream_json",
            "max_turns": claude_max_turns(),
            "reply_expected": !request.reply_to.is_empty(),
        }),
        &env_overrides,
    )?;
    let _ = trace.set_stage("executing_claude_local");
    let output = match run_claude_command(
        request.workspace_dir,
        &prompt,
        &model_name,
        &env_overrides,
        cancel_monitor.as_ref(),
        timeout,
    ) {
        Ok(output) => output,
        Err(err) => {
            let expected_reply_path =
                resolve_expected_reply_path(request.workspace_dir, reply_html_path.clone());
            if let Some(recovery_note) = maybe_recover_from_ready_reply_artifact(
                !request.reply_to.is_empty(),
                request.workspace_dir,
                &expected_reply_path,
                &err,
            ) {
                let _ = trace.record_text("logs/recovery_note.txt", &recovery_note);
                let _ = trace.finish(None, true, None, None);
                return Ok(RunTaskOutput {
                    reply_html_path: expected_reply_path,
                    reply_attachments_dir,
                    codex_output: recovery_note.clone(),
                    scheduled_tasks: Vec::new(),
                    scheduled_tasks_error: None,
                    scheduler_actions: Vec::new(),
                    scheduler_actions_error: None,
                    token_usage: None,
                    recovery_note: Some(recovery_note),
                });
            }
            let _ = trace.finish(None, false, Some(&err.to_string()), None);
            return Err(err);
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined_output = String::new();
    combined_output.push_str(&stdout);
    combined_output.push_str(&stderr);
    let _ = trace.record_outputs(&stdout, &stderr, &combined_output);
    let output_tail = tail_string(&combined_output, 2000);
    let expected_reply_path =
        resolve_expected_reply_path(request.workspace_dir, reply_html_path.clone());

    if !output.status.success() {
        let err = RunTaskError::ClaudeFailed {
            status: output.status.code(),
            output: annotate_claude_failure_output(&output_tail),
        };
        let _ = trace.finish(output.status.code(), false, Some(&err.to_string()), None);
        return Err(err);
    }

    let (assistant_text, _logs) = extract_claude_text(&stdout);
    if assistant_text.trim().is_empty() {
        let err = RunTaskError::ClaudeFailed {
            status: output.status.code(),
            output: annotate_claude_failure_output(&output_tail),
        };
        if let Some(recovery_note) = maybe_recover_from_ready_reply_artifact(
            !request.reply_to.is_empty(),
            request.workspace_dir,
            &expected_reply_path,
            &err,
        ) {
            let _ = trace.record_text("logs/recovery_note.txt", &recovery_note);
            let _ = trace.finish(output.status.code(), true, None, None);
            return Ok(RunTaskOutput {
                reply_html_path: expected_reply_path,
                reply_attachments_dir,
                codex_output: recovery_note.clone(),
                scheduled_tasks: Vec::new(),
                scheduled_tasks_error: None,
                scheduler_actions: Vec::new(),
                scheduler_actions_error: None,
                token_usage: None,
                recovery_note: Some(recovery_note),
            });
        }
        let _ = trace.finish(output.status.code(), false, Some(&err.to_string()), None);
        return Err(err);
    }
    let _ = trace.record_text("logs/assistant_output.txt", &assistant_text);
    let (scheduled_tasks, scheduled_tasks_error) = extract_scheduled_tasks(&assistant_text);
    let (scheduler_actions, scheduler_actions_error) = extract_scheduler_actions(&assistant_text);
    let assistant_tail = tail_string(&assistant_text, 2000);

    // Only check for reply file if a reply was expected
    // Use cross-channel routing to determine actual expected path
    if !request.reply_to.is_empty() {
        let _ = trace.set_stage("validating_reply_artifact");
        let err = match ensure_expected_reply_artifact(
            request.workspace_dir,
            &expected_reply_path,
            &assistant_tail,
        ) {
            Ok(()) => {
                let _ = trace.finish(output.status.code(), true, None, None);

                return Ok(RunTaskOutput {
                    reply_html_path: expected_reply_path,
                    reply_attachments_dir,
                    codex_output: assistant_tail,
                    scheduled_tasks,
                    scheduled_tasks_error,
                    scheduler_actions,
                    scheduler_actions_error,
                    token_usage: None, // TODO: Extract from Claude API response
                    recovery_note: None,
                });
            }
            Err(err) => err,
        };
        let _ = trace.finish(output.status.code(), false, Some(&err.to_string()), None);
        return Err(err);
    }
    let _ = trace.finish(output.status.code(), true, None, None);

    Ok(RunTaskOutput {
        reply_html_path: expected_reply_path,
        reply_attachments_dir,
        codex_output: assistant_tail,
        scheduled_tasks,
        scheduled_tasks_error,
        scheduler_actions,
        scheduler_actions_error,
        token_usage: None, // TODO: Extract from Claude API response
        recovery_note: None,
    })
}

fn claude_task_timeout(is_codex_fallback: bool) -> std::time::Duration {
    let default_timeout = run_task_timeout();
    if !is_codex_fallback {
        return default_timeout;
    }

    read_env_trimmed("RUN_TASK_CODEX_FALLBACK_TIMEOUT_SECS")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .map(std::time::Duration::from_secs)
        .unwrap_or_else(|| std::time::Duration::from_secs(DEFAULT_CLAUDE_FALLBACK_TIMEOUT_SECS))
        .min(default_timeout)
}

fn prepare_claude_env(
    api_key: &str,
    model_name: &str,
) -> Result<Vec<(String, String)>, RunTaskError> {
    let foundry_resource = env::var("ANTHROPIC_FOUNDRY_RESOURCE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| CLAUDE_FOUNDRY_RESOURCE_DEFAULT.to_string());
    let default_opus = env::var("ANTHROPIC_DEFAULT_OPUS_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLAUDE_MODEL.to_string());
    let default_sonnet = env::var("ANTHROPIC_DEFAULT_SONNET_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "claude-sonnet-4-5".to_string());
    let default_haiku = env::var("ANTHROPIC_DEFAULT_HAIKU_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "claude-haiku-4-5".to_string());

    ensure_claude_settings(
        model_name,
        api_key,
        &foundry_resource,
        &default_opus,
        &default_sonnet,
        &default_haiku,
    )?;

    // Get current PATH and prepend our custom bin directory for tools like google-docs
    let current_path = env::var("PATH").unwrap_or_default();
    // Look for DOWHIZ_BIN_DIR env var, or use default location relative to crate
    let dowhiz_bin_dir = env::var("DOWHIZ_BIN_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            // Default: assume bin/ is sibling to scheduler_module
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            let parent = Path::new(manifest_dir).parent().unwrap_or(Path::new("."));
            parent.join("bin").to_string_lossy().into_owned()
        });
    let extended_path = format!("{}:{}", dowhiz_bin_dir, current_path);

    // Set CLAUDE_HOME to use DoWhiz-specific config directory
    let claude_home = dowhiz_claude_home()?;

    Ok(vec![
        (
            "AZURE_OPENAI_API_KEY_BACKUP".to_string(),
            api_key.to_string(),
        ),
        // Use DoWhiz-specific Claude home to avoid affecting user's ~/.claude config
        (
            "CLAUDE_HOME".to_string(),
            claude_home.to_string_lossy().into_owned(),
        ),
        ("CLAUDE_CODE_USE_FOUNDRY".to_string(), "1".to_string()),
        ("ANTHROPIC_FOUNDRY_RESOURCE".to_string(), foundry_resource),
        ("ANTHROPIC_FOUNDRY_API_KEY".to_string(), api_key.to_string()),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), default_opus),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), default_sonnet),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), default_haiku),
        ("PATH".to_string(), extended_path),
    ])
}

/// Returns the path to the DoWhiz-specific Claude home directory.
/// This isolates DoWhiz's Claude config from the user's personal ~/.claude config.
fn dowhiz_claude_home() -> Result<std::path::PathBuf, RunTaskError> {
    let home = env::var("HOME").map_err(|_| RunTaskError::MissingEnv { key: "HOME" })?;
    // Use ~/.dowhiz/claude instead of ~/.claude to avoid overwriting user's config
    let claude_home = std::path::PathBuf::from(home)
        .join(".dowhiz")
        .join("claude");
    Ok(claude_home)
}

fn ensure_claude_settings(
    model_name: &str,
    api_key: &str,
    foundry_resource: &str,
    default_opus: &str,
    default_sonnet: &str,
    default_haiku: &str,
) -> Result<(), RunTaskError> {
    // Use DoWhiz-specific Claude home to avoid overwriting user's ~/.claude/settings.json
    let settings_dir = dowhiz_claude_home()?;
    fs::create_dir_all(&settings_dir)?;
    let settings_path = settings_dir.join("settings.json");
    let payload = serde_json::json!({
        "env": {
            "CLAUDE_CODE_USE_FOUNDRY": "1",
            "ANTHROPIC_FOUNDRY_RESOURCE": foundry_resource,
            "ANTHROPIC_FOUNDRY_API_KEY": api_key,
            "ANTHROPIC_DEFAULT_OPUS_MODEL": default_opus,
            "ANTHROPIC_DEFAULT_SONNET_MODEL": default_sonnet,
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": default_haiku,
        },
        "model": model_name,
    });
    let rendered = serde_json::to_string_pretty(&payload)
        .map_err(|err| RunTaskError::Io(io::Error::other(err)))?;
    fs::write(settings_path, format!("{}\n", rendered))?;
    Ok(())
}

fn run_claude_command(
    workspace_dir: &Path,
    prompt: &str,
    model_name: &str,
    env_overrides: &[(String, String)],
    cancel_monitor: Option<&ThreadSupersedeMonitor>,
    timeout: std::time::Duration,
) -> Result<std::process::Output, RunTaskError> {
    match run_command_with_timeout_and_cancel(
        build_claude_command(workspace_dir, prompt, model_name, env_overrides),
        timeout,
        "claude",
        cancel_monitor,
    ) {
        Ok(output) => return Ok(output),
        Err(RunTaskError::Io(err)) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }

    ensure_claude_cli_installed(env_overrides)?;
    match run_command_with_timeout_and_cancel(
        build_claude_command(workspace_dir, prompt, model_name, env_overrides),
        timeout,
        "claude",
        cancel_monitor,
    ) {
        Ok(output) => Ok(output),
        Err(RunTaskError::Io(err)) if err.kind() == io::ErrorKind::NotFound => {
            Err(RunTaskError::ClaudeNotFound)
        }
        Err(err) => Err(err),
    }
}

fn maybe_recover_from_ready_reply_artifact(
    reply_expected: bool,
    workspace_dir: &Path,
    expected_reply_path: &Path,
    err: &RunTaskError,
) -> Option<String> {
    if !reply_expected || !reply_artifact_ready_for_workspace(workspace_dir, expected_reply_path) {
        return None;
    }

    let reason = match err {
        RunTaskError::CommandTimeout {
            command: "claude",
            timeout_secs,
            ..
        } => format!(
            "Recovered ready reply artifact after Claude timed out after {}s",
            timeout_secs
        ),
        RunTaskError::ClaudeFailed {
            status: Some(0), ..
        } => {
            "Recovered ready reply artifact after Claude exited successfully without assistant text"
                .to_string()
        }
        RunTaskError::ClaudeFailed {
            status: Some(code), ..
        } => format!(
            "Recovered ready reply artifact after Claude exited with status {code} after writing output"
        ),
        RunTaskError::OutputMissing { .. } => {
            "Recovered ready reply artifact after Claude reported a late output validation failure"
                .to_string()
        }
        _ => "Recovered ready reply artifact after a late Claude failure".to_string(),
    };

    Some(reason)
}

fn annotate_claude_failure_output(output: &str) -> String {
    if is_claude_auth_failure(output) {
        format!(
            "Claude authentication failed while attempting DoWhiz fallback. No reply artifact was delivered. Clear conflicting local Claude auth state and rely on the DoWhiz Foundry settings for this run.\n{}",
            output
        )
    } else {
        output.to_string()
    }
}

fn is_claude_auth_failure(output: &str) -> bool {
    let normalized = output.to_ascii_lowercase();
    normalized.contains("invalid api key")
        || normalized.contains("please run /login")
        || normalized.contains("authentication failed")
}

fn build_claude_command(
    workspace_dir: &Path,
    prompt: &str,
    model_name: &str,
    env_overrides: &[(String, String)],
) -> Command {
    let max_turns = claude_max_turns();
    let mut cmd = Command::new("claude");
    remove_restricted_agent_env(&mut cmd);
    if let Ok(settings_path) = claude_settings_path() {
        cmd.arg("--settings").arg(settings_path);
    }
    cmd.arg("-p")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .arg("--model")
        .arg(model_name)
        .arg("--allowedTools")
        .arg(CLAUDE_ALLOWED_TOOLS)
        .arg("--max-turns")
        .arg(max_turns.to_string())
        .arg("--dangerously-skip-permissions")
        .arg(prompt)
        .current_dir(workspace_dir);
    cmd.env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .env_remove("ANTHROPIC_BASE_URL")
        .env_remove("ANTHROPIC_API_BASE")
        .env_remove("CLAUDE_API_KEY");
    apply_env_pairs(&mut cmd, env_overrides);
    cmd
}

fn claude_settings_path() -> Result<PathBuf, RunTaskError> {
    Ok(dowhiz_claude_home()?.join("settings.json"))
}

fn claude_max_turns() -> u32 {
    env::var("CLAUDE_MAX_TURNS")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10)
}

fn ensure_claude_cli_installed(env_overrides: &[(String, String)]) -> Result<(), RunTaskError> {
    let mut cmd = Command::new("npm");
    cmd.args(["i", "-g", "@anthropic-ai/claude-code"]);
    apply_env_pairs(&mut cmd, env_overrides);
    let output = match cmd.output() {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(RunTaskError::ClaudeInstallFailed {
                output: "npm not found on PATH".to_string(),
            })
        }
        Err(err) => return Err(RunTaskError::Io(err)),
    };
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Err(RunTaskError::ClaudeInstallFailed {
            output: tail_string(&combined, 2000),
        });
    }
    Ok(())
}

fn apply_env_pairs(cmd: &mut Command, overrides: &[(String, String)]) {
    for (key, value) in overrides {
        cmd.env(key, value);
    }
}

fn extract_claude_text(raw: &str) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut logs = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let event: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => {
                logs.push(trimmed.to_string());
                continue;
            }
        };
        let event_type = event
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        // Handle both old and new Claude stream-json formats
        if matches!(
            event_type,
            "text_delta"
                | "message_delta"
                | "content_block_delta"
                | "message_stop"
                | "result"
                | "assistant"
                | "text"
                | "message"
        ) {
            if let Some(fragment) = extract_claude_fragment(&event) {
                text.push_str(&fragment);
            }
        }
    }
    (text, logs)
}

fn extract_claude_fragment(event: &serde_json::Value) -> Option<String> {
    // Direct text field
    if let Some(text) = event.get("text").and_then(|value| value.as_str()) {
        return Some(text.to_string());
    }
    // Delta format: {"delta": {"text": "..."}}
    if let Some(text) = event
        .get("delta")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
    {
        return Some(text.to_string());
    }
    // New Claude format: {"message": {"content": [{"type": "text", "text": "..."}]}}
    if let Some(content) = event
        .get("message")
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_array())
    {
        let mut combined = String::new();
        for item in content {
            if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    combined.push_str(text);
                }
            }
        }
        if !combined.is_empty() {
            return Some(combined);
        }
    }
    // Old message format: {"message": {"text": "..."}}
    if let Some(text) = event
        .get("message")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
    {
        return Some(text.to_string());
    }
    if let Some(text) = event.get("final_text").and_then(|value| value.as_str()) {
        return Some(text.to_string());
    }
    if let Some(text) = event.get("result").and_then(|value| value.as_str()) {
        return Some(text.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::env::acquire_env_test_lock;
    use super::{build_claude_command, claude_task_timeout, CLAUDE_ALLOWED_TOOLS};
    use std::env;
    use std::path::Path;
    use std::time::Duration;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = env::var(key).ok();
            env::set_var(key, value);
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = env::var(key).ok();
            env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        acquire_env_test_lock()
    }

    #[test]
    fn claude_fallback_timeout_defaults_to_900s_cap() {
        let _lock = env_lock();
        let _guards = [
            EnvVarGuard::set("RUN_TASK_TIMEOUT_SECS", "1200"),
            EnvVarGuard::unset("RUN_TASK_CODEX_FALLBACK_TIMEOUT_SECS"),
            EnvVarGuard::unset("TASK_TIMEOUT_SECS"),
        ];

        assert_eq!(claude_task_timeout(true), Duration::from_secs(900));
    }

    #[test]
    fn claude_fallback_timeout_respects_explicit_cap() {
        let _lock = env_lock();
        let _guards = [
            EnvVarGuard::set("RUN_TASK_TIMEOUT_SECS", "1200"),
            EnvVarGuard::set("RUN_TASK_CODEX_FALLBACK_TIMEOUT_SECS", "300"),
            EnvVarGuard::unset("TASK_TIMEOUT_SECS"),
        ];

        assert_eq!(claude_task_timeout(true), Duration::from_secs(300));
    }

    #[test]
    fn build_claude_command_includes_web_and_todo_tools() {
        let cmd = build_claude_command(Path::new("."), "hello", "claude-sonnet-4-5", &[]);
        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let allowed_tools_idx = args
            .iter()
            .position(|arg| arg == "--allowedTools")
            .expect("allowedTools flag should be present");

        assert_eq!(args[allowed_tools_idx + 1], CLAUDE_ALLOWED_TOOLS);
    }
}
