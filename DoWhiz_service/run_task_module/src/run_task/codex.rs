use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;

use super::browserbase::{
    collect_browserbase_env_overrides, BrowserbaseSessionCleanupGuard,
    BROWSERBASE_ACTIVE_SESSION_PATH_ENV_KEY, BROWSERBASE_STATE_DIR_ENV_KEY,
    BROWSER_HANDOFF_BASE_URL_ENV_KEY, BROWSER_HANDOFF_SIGNING_SECRET_ENV_KEY,
};
use super::constants::{
    CODEX_CONFIG_BASE_URL_PLACEHOLDER, CODEX_CONFIG_BLOCK_TEMPLATE, CODEX_CONFIG_MARKER,
    CODEX_MODEL_NAME, CODEX_SANDBOX_MODE, DOCKER_CODEX_HOME_DIR, DOCKER_WORKSPACE_DIR,
};
use super::docker::{docker_cli_available, ensure_docker_image_available};
use super::env::{
    env_enabled, normalize_env_prefix, read_env_list, read_env_trimmed, remove_restricted_agent_env,
};
use super::errors::RunTaskError;
use super::github_auth::{ensure_github_cli_auth, resolve_github_auth};
use super::prompt::{build_prompt, load_memory_context};
use super::scheduled::{extract_scheduled_tasks, extract_scheduler_actions};
use super::timing::{TaskTimingBuilder, TIMING_COLLECTOR};
use super::trace::RunTaskTraceRecorder;
use super::types::RunTaskParams;
use super::types::{RunTaskOutput, RunTaskRequest, TokenUsage};
use super::utils::{
    run_command_with_timeout, run_command_with_timeout_and_cancel, run_task_timeout, tail_string,
    ThreadSupersedeMonitor,
};
use super::workspace::{canonicalize_dir, workspace_path_in_container};

const PAYMENT_ENV_KEYS: &[&str] = &[
    "GOATX402_API_URL",
    "GOATX402_MERCHANT_ID",
    "GOATX402_API_KEY",
    "GOATX402_API_SECRET",
    "GOATX402_WALLET_ADDRESS",
    "GOATX402_AGENT_ID",
    "GOATX402_CHAIN_ID",
    "GOATX402_RPC_URL",
    "GOATX402_EXPLORER_URL",
    "GOATX402_USDC_ADDRESS",
    "GOATX402_USDT_ADDRESS",
    "GOAT_WALLET_ADDRESS",
    "GOAT_AGENT_ID",
    "GOAT_CHAIN_ID",
    "GOAT_RPC_URL",
    "GOAT_EXPLORER_URL",
    "GOAT_USDC_ADDRESS",
    "GOAT_USDT_ADDRESS",
    "X402_API_URL",
    "X402_MERCHANT_ID",
    "X402_API_KEY",
    "X402_API_SECRET",
];
const HUMAN_APPROVAL_GATE_ENV_KEYS: &[&str] = &[
    "POSTMARK_SERVER_TOKEN",
    "HUMAN_APPROVAL_FROM",
    "HUMAN_APPROVAL_REPLY_TO",
    "POSTMARK_API_BASE_URL",
    "GOOGLE_PASSWORD",
    BROWSERBASE_STATE_DIR_ENV_KEY,
    BROWSERBASE_ACTIVE_SESSION_PATH_ENV_KEY,
    BROWSER_HANDOFF_BASE_URL_ENV_KEY,
    BROWSER_HANDOFF_SIGNING_SECRET_ENV_KEY,
];
const HUMAN_APPROVAL_GATE_REQUIRE_MCP_ENV_KEY: &str = "HUMAN_APPROVAL_GATE_REQUIRE_MCP";
const HUMAN_APPROVAL_GATE_MCP_SERVER_NAME: &str = "human-approval-gate";
const HUMAN_APPROVAL_GATE_MCP_TOOL_TIMEOUT_SECONDS: u32 = 31 * 60;
const LARK_ENV_KEYS: &[&str] = &["LARK_APP_ID", "LARK_APP_SECRET"];
const HUMAN_APPROVAL_FROM_ENV_KEY: &str = "HUMAN_APPROVAL_FROM";
const HUMAN_APPROVAL_REPLY_TO_ENV_KEY: &str = "HUMAN_APPROVAL_REPLY_TO";
const EMPLOYEE_CONFIG_PATH_ENV_KEY: &str = "EMPLOYEE_CONFIG_PATH";
const EMPLOYEE_ID_ENV_KEY: &str = "EMPLOYEE_ID";
const DEPLOY_TARGET_ENV_KEY: &str = "DEPLOY_TARGET";
const STAGING_DEPLOY_TARGET: &str = "staging";
const GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV: &str = "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE";
const GOOGLE_WORKSPACE_CLI_CREDENTIAL_COMPONENT_KEYS: &[&str] = &[
    "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID",
    "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET",
    "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN",
    "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE",
];
const GOOGLE_WORKSPACE_CLI_CREDENTIALS_REL_PATH: &str =
    ".secrets/google_workspace_cli_credentials.json";
const DISCORD_CONTEXT_REL_PATH: &str = ".discord_context.json";
const BRIGHT_DATA_API_KEY_ENV_KEY: &str = "BRIGHT_DATA_API_KEY";
const BRIGHTDATA_API_KEY_ENV_KEY: &str = "BRIGHTDATA_API_KEY";
const BRIGHT_DATA_OPTIONAL_ENV_KEYS: &[&str] = &[
    "BRIGHT_DATA_XIAOHONGSHU_COLLECTOR",
    "BRIGHT_DATA_XIAOHONGSHU_TRIGGER_URL",
];

const REMOTE_OUTPUT_FILENAME: &str = ".codex_remote_output.log";
const REMOTE_EXIT_CODE_FILENAME: &str = ".codex_remote_exit_code";
const HAG_MCP_CONFIG_START_MARKER: &str = "# BEGIN DOWHIZ HUMAN APPROVAL GATE MCP";
const HAG_MCP_CONFIG_END_MARKER: &str = "# END DOWHIZ HUMAN APPROVAL GATE MCP";
const EPHEMERAL_SHARE_PREFIX: &str = "task-";
static ACI_CONTAINER_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize)]
struct HumanApprovalEmployeeConfigFile {
    #[serde(default)]
    employees: Vec<HumanApprovalEmployeeConfigEntry>,
}

#[derive(Debug, Deserialize)]
struct HumanApprovalEmployeeConfigEntry {
    id: String,
    #[serde(default)]
    addresses: Vec<String>,
}

/// Global registry of active ACI containers created by this process.
/// Used for cleanup on shutdown to prevent orphaned containers.
static ACTIVE_ACI_CONTAINERS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn register_aci_container(name: &str) {
    if let Ok(mut containers) = ACTIVE_ACI_CONTAINERS.lock() {
        containers.insert(name.to_string());
    }
}

fn deregister_aci_container(name: &str) {
    if let Ok(mut containers) = ACTIVE_ACI_CONTAINERS.lock() {
        containers.remove(name);
    }
}

/// Clean up all active ACI containers created by this process.
/// Called on shutdown to prevent orphaned containers.
/// Returns the number of containers that were cleaned up.
pub fn cleanup_all_aci_containers() -> usize {
    let containers: Vec<String> = match ACTIVE_ACI_CONTAINERS.lock() {
        Ok(mut guard) => guard.drain().collect(),
        Err(poisoned) => poisoned.into_inner().drain().collect(),
    };

    if containers.is_empty() {
        return 0;
    }

    eprintln!(
        "[cleanup] cleaning up {} active ACI container(s) on shutdown",
        containers.len()
    );

    let config = match load_azure_aci_config() {
        Ok(config) => config,
        Err(err) => {
            eprintln!(
                "[cleanup] failed to load ACI config, cannot clean up containers: {:?}",
                err
            );
            return 0;
        }
    };

    let mut cleaned = 0;
    for container_name in &containers {
        eprintln!("[cleanup] deleting ACI container: {}", container_name);
        match delete_aci_container_with_retry(&config, container_name) {
            Ok(()) => {
                eprintln!("[cleanup] successfully deleted: {}", container_name);
                cleaned += 1;
            }
            Err(err) => {
                eprintln!("[cleanup] failed to delete {}: {:?}", container_name, err);
            }
        }
    }

    eprintln!(
        "[cleanup] finished cleaning up {}/{} ACI container(s)",
        cleaned,
        containers.len()
    );
    cleaned
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionBackend {
    Local,
    AzureAci,
}

#[derive(Debug, Clone)]
struct AzureAciConfig {
    resource_group: String,
    image: String,
    location: Option<String>,
    registry_server: Option<String>,
    registry_username: Option<String>,
    registry_password: Option<String>,
    cpu: String,
    memory_gb: String,
    storage_account: String,
    storage_key: String,
    file_share: String,
    host_share_root: PathBuf,
    container_share_root: PathBuf,
}

#[derive(Debug, Clone)]
struct AzureAciExecutionArtifacts {
    container_state: String,
    container_logs: String,
    container_show_json: Option<String>,
    remote_artifact_completion: bool,
}

#[derive(Debug, Clone)]
struct AciPollState {
    container_state: String,
    remote_artifact_completion: bool,
}

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
        "slack" | "discord" | "telegram" | "sms" | "whatsapp" | "bluebubbles" | "lark"
        | "wechat" | "wechat_mp" => workspace_dir.join("reply_message.txt"),
        "notion" => {
            // Notion agent posts directly via API and creates .notion_api_replied marker
            workspace_dir.join(".notion_api_replied")
        }
        _ => default_path,
    }
}

fn reply_artifact_ready(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if path.file_name().and_then(|value| value.to_str()) == Some(".notion_api_replied") {
        return true;
    }
    match fs::read_to_string(path) {
        Ok(contents) => !contents.trim().is_empty(),
        Err(_) => fs::metadata(path)
            .map(|meta| meta.len() > 0)
            .unwrap_or(false),
    }
}

fn maybe_recover_from_ready_reply_artifact(
    reply_expected: bool,
    expected_reply_path: &Path,
    exit_status: Option<i32>,
    failure_output: &str,
) -> Option<String> {
    if !reply_expected || !reply_artifact_ready(expected_reply_path) {
        return None;
    }

    let lowered = failure_output.to_ascii_lowercase();
    let reason = if lowered.contains("response.failed event received") {
        "Recovered ready reply artifact after Codex stream disconnect during finalization"
            .to_string()
    } else if lowered.contains("cannot assist with that request") {
        "Recovered ready reply artifact after a late Codex refusal".to_string()
    } else if lowered.contains("task_complete reported status=") {
        "Recovered ready reply artifact after Codex reported a late task_complete failure"
            .to_string()
    } else if let Some(code) = exit_status.filter(|code| *code != 0) {
        format!(
            "Recovered ready reply artifact after Codex exited with status {code} after writing output"
        )
    } else {
        "Recovered ready reply artifact after Codex reported a late failure".to_string()
    };

    Some(reason)
}

fn record_codex_success(
    trace: &mut RunTaskTraceRecorder,
    exit_status: Option<i32>,
    output_tail: &str,
    recovery_note: Option<&str>,
    token_usage: Option<&TokenUsage>,
) {
    let _ = trace.record_text("logs/assistant_output_tail.txt", output_tail);
    if let Some(note) = recovery_note {
        let _ = trace.record_text("logs/recovery_note.txt", note);
    }
    let _ = trace.finish(exit_status, true, None, token_usage);
}

fn validate_warm_pool_codex_result(
    reply_expected: bool,
    expected_reply_path: &Path,
    exit_status: i32,
    codex_output: &str,
) -> Result<Option<String>, RunTaskError> {
    let output_tail = tail_string(codex_output, 4000);
    if exit_status != 0 {
        let err = RunTaskError::CodexFailed {
            status: Some(exit_status),
            output: output_tail.clone(),
        };
        if let Some(recovery_note) = maybe_recover_from_ready_reply_artifact(
            reply_expected,
            expected_reply_path,
            Some(exit_status),
            &err.to_string(),
        ) {
            return Ok(Some(recovery_note));
        }
        return Err(err);
    }

    if let Some(runtime_failure) = detect_codex_runtime_failure(codex_output) {
        let status = runtime_failure
            .status_code
            .or(Some(exit_status).filter(|code| *code != 0));
        let mut failure_output = runtime_failure.message;
        if !output_tail.is_empty() {
            failure_output.push('\n');
            failure_output.push_str(&output_tail);
        }
        let err = RunTaskError::CodexFailed {
            status,
            output: failure_output,
        };
        if let Some(recovery_note) = maybe_recover_from_ready_reply_artifact(
            reply_expected,
            expected_reply_path,
            status,
            &err.to_string(),
        ) {
            return Ok(Some(recovery_note));
        }
        return Err(err);
    }

    if reply_expected && !reply_artifact_ready(expected_reply_path) {
        return Err(RunTaskError::OutputMissing {
            path: expected_reply_path.to_path_buf(),
            output: output_tail,
        });
    }

    Ok(None)
}

pub(super) fn run_codex_task(
    request: RunTaskRequest<'_>,
    runner: &str,
    reply_html_path: PathBuf,
    reply_attachments_dir: PathBuf,
) -> Result<RunTaskOutput, RunTaskError> {
    super::env::load_env_sources(request.workspace_dir)?;
    let _browserbase_cleanup = BrowserbaseSessionCleanupGuard::new(request.workspace_dir);
    let cancel_monitor = request
        .thread_epoch
        .zip(request.thread_state_path)
        .map(|(epoch, path)| ThreadSupersedeMonitor::new(path, epoch));
    let backend = resolve_execution_backend();
    match backend {
        ExecutionBackend::AzureAci => {
            eprintln!(
                "[run_task] execution_backend=azure_aci deploy_target={}",
                env::var("DEPLOY_TARGET").unwrap_or_else(|_| "unknown".to_string())
            );
        }
        ExecutionBackend::Local => {
            eprintln!("[run_task] execution_backend=local");
        }
    }
    if backend == ExecutionBackend::AzureAci {
        return run_codex_task_azure_aci(
            request,
            runner,
            reply_html_path,
            reply_attachments_dir,
            cancel_monitor.as_ref(),
        );
    }
    ensure_local_execution_allowed()?;
    let docker_image = read_env_trimmed("RUN_TASK_DOCKER_IMAGE");
    let docker_requested = env_enabled("RUN_TASK_USE_DOCKER");
    let docker_available = docker_requested && docker_cli_available();
    let docker_required = env_enabled("RUN_TASK_DOCKER_REQUIRED");
    let use_docker = docker_requested && docker_available;
    if docker_requested && !docker_available {
        if docker_required {
            return Err(RunTaskError::DockerNotFound);
        }
        eprintln!(
            "[run_task] Docker CLI not found; falling back to host execution. Set RUN_TASK_DOCKER_REQUIRED=1 to fail."
        );
    }
    let docker_image = if use_docker {
        docker_image.ok_or(RunTaskError::MissingEnv {
            key: "RUN_TASK_DOCKER_IMAGE",
        })?
    } else {
        String::new()
    };
    let host_workspace_dir = if use_docker {
        Some(canonicalize_dir(request.workspace_dir)?)
    } else {
        None
    };
    let askpass_dir = if use_docker {
        host_workspace_dir
            .as_ref()
            .map(|dir| dir.join(DOCKER_CODEX_HOME_DIR))
    } else {
        None
    };
    let github_auth = resolve_github_auth(askpass_dir.as_deref())?;

    let api_key =
        env::var("AZURE_OPENAI_API_KEY_BACKUP").map_err(|_| RunTaskError::MissingEnv {
            key: "AZURE_OPENAI_API_KEY_BACKUP",
        })?;
    if api_key.trim().is_empty() {
        return Err(RunTaskError::MissingEnv {
            key: "AZURE_OPENAI_API_KEY_BACKUP",
        });
    }
    let azure_endpoint = azure_endpoint_from_env()?;
    // Use model from request/database, fallback to env var, then constant
    let model_name = if request.model_name.trim().is_empty() {
        env::var("CODEX_MODEL").unwrap_or_else(|_| CODEX_MODEL_NAME.to_string())
    } else {
        request.model_name.to_string()
    };
    let sandbox_mode = codex_sandbox_mode();
    // Bypass sandbox for GoogleDocs tasks to allow network access for Google APIs
    let channel_lower = request.channel.to_lowercase();
    let is_google_docs = channel_lower == "google_docs" || channel_lower == "googledocs";
    // Also bypass sandbox if workspace has .google_access_token (indicates Google Docs artifacts)
    let has_google_token = request.workspace_dir.join(".google_access_token").exists();
    let bypass_sandbox = codex_bypass_sandbox() || use_docker || is_google_docs || has_google_token;
    let sandbox_mode = effective_codex_sandbox_mode(&sandbox_mode, bypass_sandbox);
    let add_dirs = codex_add_dirs(request.workspace_dir, use_docker)?;
    if use_docker {
        let codex_home = host_workspace_dir
            .as_ref()
            .map(|dir| dir.join(DOCKER_CODEX_HOME_DIR))
            .unwrap_or_else(|| request.workspace_dir.join(DOCKER_CODEX_HOME_DIR));
        ensure_codex_config_at(
            &codex_home,
            Path::new(DOCKER_WORKSPACE_DIR),
            &azure_endpoint,
        )?;
    } else {
        ensure_codex_config(request.workspace_dir, &azure_endpoint)?;
    }
    ensure_github_cli_auth(&github_auth)?;
    let payment_env_overrides = collect_payment_env_overrides();
    let bright_data_env_overrides = collect_bright_data_env_overrides();
    let google_workspace_cli_env_overrides = collect_google_workspace_cli_env_overrides(
        host_workspace_dir
            .as_deref()
            .unwrap_or(request.workspace_dir),
    )?;
    ensure_discord_context_file(
        host_workspace_dir
            .as_deref()
            .unwrap_or(request.workspace_dir),
    )?;
    let browserbase_workspace_dir = if use_docker {
        PathBuf::from(DOCKER_WORKSPACE_DIR)
    } else {
        canonicalize_dir(request.workspace_dir)?
    };
    let browserbase_env_overrides = collect_browserbase_env_overrides(&browserbase_workspace_dir);
    let human_approval_gate_env_overrides = collect_human_approval_gate_env_overrides();
    let lark_env_overrides = collect_lark_env_overrides();

    let memory_context = load_memory_context(request.workspace_dir, request.memory_dir)?;
    let prompt = build_prompt(
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
    );

    let mut trace_env_overrides = vec![
        (
            "AZURE_OPENAI_API_KEY_BACKUP".to_string(),
            api_key.to_string(),
        ),
        (
            "AZURE_OPENAI_ENDPOINT_BACKUP".to_string(),
            azure_endpoint.to_string(),
        ),
    ];
    if let Some(token) = request.google_access_token {
        trace_env_overrides.push(("GOOGLE_ACCESS_TOKEN".to_string(), token.to_string()));
        trace_env_overrides.push(("GOOGLE_WORKSPACE_CLI_TOKEN".to_string(), token.to_string()));
    }
    for (key, value) in &payment_env_overrides {
        trace_env_overrides.push((key.clone(), value.clone()));
    }
    for (key, value) in &bright_data_env_overrides {
        trace_env_overrides.push((key.clone(), value.clone()));
    }
    for (key, value) in &google_workspace_cli_env_overrides {
        trace_env_overrides.push((key.clone(), value.clone()));
    }
    for (key, value) in &browserbase_env_overrides {
        trace_env_overrides.push((key.clone(), value.clone()));
    }
    for (key, value) in &human_approval_gate_env_overrides {
        trace_env_overrides.push((key.clone(), value.clone()));
    }
    for (key, value) in &lark_env_overrides {
        trace_env_overrides.push((key.clone(), value.clone()));
    }
    for (key, value) in &github_auth.env_overrides {
        trace_env_overrides.push((key.clone(), value.clone()));
    }

    let timeout = codex_command_timeout();
    let mut trace = RunTaskTraceRecorder::new(
        request.workspace_dir,
        runner,
        if use_docker {
            "codex_docker"
        } else {
            "codex_local"
        },
        &model_name,
        &prompt,
        timeout,
        serde_json::json!({
            "reply_expected": !request.reply_to.is_empty(),
            "sandbox_mode": sandbox_mode.clone(),
            "bypass_sandbox": bypass_sandbox,
            "use_docker": use_docker,
            "docker_image": if use_docker { serde_json::Value::String(docker_image.clone()) } else { serde_json::Value::Null },
            "add_dirs": add_dirs.clone(),
        }),
        &trace_env_overrides,
    )?;
    let output = if use_docker {
        if let Err(err) = ensure_docker_image_available(&docker_image) {
            let _ = trace.finish(None, false, Some(&err.to_string()), None);
            return Err(err);
        }
        let host_workspace_dir = host_workspace_dir
            .as_ref()
            .ok_or(RunTaskError::MissingEnv {
                key: "RUN_TASK_DOCKER_IMAGE",
            })?;
        let askpass_container_path = github_auth
            .askpass_path
            .as_ref()
            .and_then(|path| workspace_path_in_container(path, host_workspace_dir));

        if github_auth.askpass_path.is_some() && askpass_container_path.is_none() {
            return Err(RunTaskError::InvalidPath {
                label: "git_askpass_path",
                path: github_auth
                    .askpass_path
                    .clone()
                    .unwrap_or_else(|| host_workspace_dir.join("missing")),
                reason: "askpass path is not within workspace_dir",
            });
        }

        let mut cmd = Command::new("docker");
        cmd.arg("run")
            .arg("--rm")
            .arg("--workdir")
            .arg(DOCKER_WORKSPACE_DIR)
            .arg("-v")
            .arg(format!(
                "{}:{}",
                host_workspace_dir.display(),
                DOCKER_WORKSPACE_DIR
            ))
            .arg("-e")
            .arg(format!("HOME={}", DOCKER_WORKSPACE_DIR))
            .arg("-e")
            .arg(format!(
                "CODEX_HOME={}/{}",
                DOCKER_WORKSPACE_DIR, DOCKER_CODEX_HOME_DIR
            ))
            .arg("-e")
            .arg(format!("AZURE_OPENAI_API_KEY_BACKUP={}", api_key))
            .arg("-e")
            .arg(format!("AZURE_OPENAI_ENDPOINT_BACKUP={}", azure_endpoint));
        // Write Google access token to file for sandbox environments without network access
        // (Codex sandbox may not pass environment variables to tools it spawns)
        if let Some(ref token) = request.google_access_token {
            cmd.arg("-e").arg(format!("GOOGLE_ACCESS_TOKEN={}", token));
            // Also set GOOGLE_WORKSPACE_CLI_TOKEN for gws CLI (third-party @googleworkspace/cli)
            // This takes highest priority and avoids OAuth refresh_token issues (invalid_rapt)
            cmd.arg("-e")
                .arg(format!("GOOGLE_WORKSPACE_CLI_TOKEN={}", token));
            // Also write to file as backup since Codex sandbox may strip env vars
            let token_file = host_workspace_dir.join(".google_access_token");
            if let Err(e) = std::fs::write(&token_file, token) {
                eprintln!(
                    "[run_task] Warning: Failed to write Google access token file: {}",
                    e
                );
            }
        }
        // Write Notion access token for channel-agnostic Notion operations
        if let Some(ref token) = request.notion_access_token {
            cmd.arg("-e").arg(format!("NOTION_API_TOKEN={}", token));
            // Also write to .notion_env file for CLI tools
            let notion_env_file = host_workspace_dir.join(".notion_env");
            if let Err(e) =
                std::fs::write(&notion_env_file, format!("NOTION_API_TOKEN={}\n", token))
            {
                eprintln!("[run_task] Warning: Failed to write Notion env file: {}", e);
            }
        }
        for (key, value) in &payment_env_overrides {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
        for (key, value) in &bright_data_env_overrides {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
        for (key, value) in &google_workspace_cli_env_overrides {
            if key == GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV {
                let host_path = PathBuf::from(value);
                if let Some(container_path) =
                    workspace_path_in_container(&host_path, host_workspace_dir)
                {
                    cmd.arg("-e")
                        .arg(format!("{}={}", key, container_path.display()));
                } else {
                    eprintln!(
                        "[run_task] warning: {} is outside workspace mount; skipping container override",
                        GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV
                    );
                }
            } else {
                cmd.arg("-e").arg(format!("{}={}", key, value));
            }
        }
        for (key, value) in &browserbase_env_overrides {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
        for (key, value) in &human_approval_gate_env_overrides {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
        for (key, value) in &lark_env_overrides {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
        cmd.arg("-e")
            .arg(format!("{}=1", HUMAN_APPROVAL_GATE_REQUIRE_MCP_ENV_KEY));
        for (key, value) in &github_auth.env_overrides {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
        if let Some(container_path) = askpass_container_path {
            cmd.arg("-e")
                .arg(format!("GIT_ASKPASS={}", container_path.display()))
                .arg("-e")
                .arg("GIT_TERMINAL_PROMPT=0");
        }
        if let Some(network) = read_env_trimmed("RUN_TASK_DOCKER_NETWORK") {
            cmd.arg("--network").arg(network);
        }
        for dns in read_env_list("RUN_TASK_DOCKER_DNS") {
            cmd.arg("--dns").arg(dns);
        }
        for search_domain in read_env_list("RUN_TASK_DOCKER_DNS_SEARCH") {
            cmd.arg("--dns-search").arg(search_domain);
        }
        cmd.arg("--entrypoint")
            .arg("codex")
            .arg(&docker_image)
            .arg("exec")
            .arg("--json");
        if bypass_sandbox {
            cmd.arg("--yolo");
        }
        for add_dir in &add_dirs {
            cmd.arg("--add-dir").arg(add_dir);
        }
        cmd.arg("--skip-git-repo-check")
            .arg("-m")
            .arg(&model_name)
            .arg("-c")
            .arg("web_search=\"live\"")
            .arg("-c")
            .arg("ask_for_approval=\"never\"")
            .arg("-c")
            .arg(format!("sandbox=\"{}\"", sandbox_mode))
            .arg("-c")
            .arg("model_provider=\"azure\"")
            .arg("-c")
            .arg("model_providers.azure.env_key=\"AZURE_OPENAI_API_KEY_BACKUP\"")
            .arg("--cd")
            .arg(DOCKER_WORKSPACE_DIR)
            .arg(prompt);

        match run_command_with_timeout_and_cancel(
            cmd,
            timeout,
            "docker run",
            cancel_monitor.as_ref(),
        ) {
            Ok(output) => output,
            Err(RunTaskError::Io(err)) if err.kind() == io::ErrorKind::NotFound => {
                let failure = RunTaskError::DockerNotFound;
                let _ = trace.finish(None, false, Some(&failure.to_string()), None);
                return Err(failure);
            }
            Err(err) => {
                let _ = trace.finish(None, false, Some(&err.to_string()), None);
                return Err(err);
            }
        }
    } else {
        let mut cmd = Command::new("codex");
        cmd.arg("exec").arg("--json");
        remove_restricted_agent_env(&mut cmd);
        if bypass_sandbox {
            cmd.arg("--yolo");
        }
        for add_dir in &add_dirs {
            cmd.arg("--add-dir").arg(add_dir);
        }
        cmd.arg("--skip-git-repo-check")
            .arg("-m")
            .arg(&model_name)
            .arg("-c")
            .arg("web_search=\"live\"")
            .arg("-c")
            .arg("ask_for_approval=\"never\"")
            .arg("-c")
            .arg(format!("sandbox=\"{}\"", sandbox_mode))
            .arg("-c")
            .arg("model_provider=\"azure\"")
            .arg("-c")
            .arg("model_providers.azure.env_key=\"AZURE_OPENAI_API_KEY_BACKUP\"")
            .arg("--cd")
            .arg(request.workspace_dir)
            .arg(prompt)
            .env("AZURE_OPENAI_API_KEY_BACKUP", api_key)
            .env("AZURE_OPENAI_ENDPOINT_BACKUP", &azure_endpoint)
            .env_remove("OPENAI_API_KEY") // Prevent Codex from using OpenAI instead of Azure
            .current_dir(request.workspace_dir);
        // Extend PATH with DoWhiz bin directory for tools like google-docs
        let current_path = env::var("PATH").unwrap_or_default();
        let dowhiz_bin_dir = env::var("DOWHIZ_BIN_DIR")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| {
                let manifest_dir = env!("CARGO_MANIFEST_DIR");
                let parent = Path::new(manifest_dir).parent().unwrap_or(Path::new("."));
                parent.join("bin").to_string_lossy().into_owned()
            });
        let extended_path = format!("{}:{}", dowhiz_bin_dir, current_path);
        cmd.env("PATH", extended_path);
        // Write Google access token to file for sandbox environments without network access
        // (Codex sandbox may not pass environment variables to tools it spawns)
        if let Some(ref token) = request.google_access_token {
            cmd.env("GOOGLE_ACCESS_TOKEN", token);
            // Also set GOOGLE_WORKSPACE_CLI_TOKEN for gws CLI (third-party @googleworkspace/cli)
            // This takes highest priority and avoids OAuth refresh_token issues (invalid_rapt)
            cmd.env("GOOGLE_WORKSPACE_CLI_TOKEN", token);
            // Also write to file as backup since Codex sandbox may strip env vars
            let token_file = request.workspace_dir.join(".google_access_token");
            if let Err(e) = fs::write(&token_file, token) {
                eprintln!(
                    "[run_task] Warning: Failed to write Google access token file: {}",
                    e
                );
            }
        }
        // Write Notion access token for channel-agnostic Notion operations
        if let Some(ref token) = request.notion_access_token {
            cmd.env("NOTION_API_TOKEN", token);
            // Also write to .notion_env file for CLI tools
            let notion_env_file = request.workspace_dir.join(".notion_env");
            if let Err(e) = fs::write(&notion_env_file, format!("NOTION_API_TOKEN={}\n", token)) {
                eprintln!("[run_task] Warning: Failed to write Notion env file: {}", e);
            }
        }
        for (key, value) in &payment_env_overrides {
            cmd.env(key, value);
        }
        for (key, value) in &bright_data_env_overrides {
            cmd.env(key, value);
        }
        for (key, value) in &google_workspace_cli_env_overrides {
            cmd.env(key, value);
        }
        for (key, value) in &browserbase_env_overrides {
            cmd.env(key, value);
        }
        for (key, value) in &human_approval_gate_env_overrides {
            cmd.env(key, value);
        }
        for (key, value) in &lark_env_overrides {
            cmd.env(key, value);
        }
        cmd.env(HUMAN_APPROVAL_GATE_REQUIRE_MCP_ENV_KEY, "1");
        for (key, value) in github_auth.env_overrides {
            cmd.env(key, value);
        }
        if let Some(askpass_path) = github_auth.askpass_path {
            cmd.env("GIT_ASKPASS", askpass_path);
            cmd.env("GIT_TERMINAL_PROMPT", "0");
        }

        match run_command_with_timeout_and_cancel(cmd, timeout, "codex", cancel_monitor.as_ref()) {
            Ok(output) => output,
            Err(RunTaskError::Io(err)) if err.kind() == io::ErrorKind::NotFound => {
                let failure = RunTaskError::CodexNotFound;
                let _ = trace.finish(None, false, Some(&failure.to_string()), None);
                return Err(failure);
            }
            Err(err) => {
                let _ = trace.finish(None, false, Some(&err.to_string()), None);
                return Err(err);
            }
        }
    };

    let stdout_output = String::from_utf8_lossy(&output.stdout);
    let stderr_output = String::from_utf8_lossy(&output.stderr);
    let mut combined_output = String::new();
    combined_output.push_str(&stdout_output);
    combined_output.push_str(&stderr_output);
    let _ = trace.record_outputs(&stdout_output, &stderr_output, &combined_output);

    let (scheduled_tasks, scheduled_tasks_error, scheduler_actions, scheduler_actions_error) =
        parse_scheduling_from_outputs(
            &stdout_output,
            &stderr_output,
            &combined_output,
            request.workspace_dir,
        );
    let token_usage = extract_token_usage(&combined_output);
    let output_tail = tail_string(&combined_output, 2000);
    let expected_reply_path =
        resolve_expected_reply_path(request.workspace_dir, reply_html_path.clone());

    if !output.status.success() {
        let err = if use_docker {
            RunTaskError::DockerFailed {
                status: output.status.code(),
                output: output_tail.clone(),
            }
        } else {
            RunTaskError::CodexFailed {
                status: output.status.code(),
                output: output_tail.clone(),
            }
        };
        if let Some(recovery_note) = maybe_recover_from_ready_reply_artifact(
            !request.reply_to.is_empty(),
            &expected_reply_path,
            output.status.code(),
            &err.to_string(),
        ) {
            record_codex_success(
                &mut trace,
                output.status.code(),
                &output_tail,
                Some(&recovery_note),
                token_usage.as_ref(),
            );
            return Ok(RunTaskOutput {
                reply_html_path: expected_reply_path,
                reply_attachments_dir,
                codex_output: output_tail,
                scheduled_tasks,
                scheduled_tasks_error,
                scheduler_actions,
                scheduler_actions_error,
                token_usage,
                recovery_note: Some(recovery_note),
            });
        }
        let _ = trace.finish(
            output.status.code(),
            false,
            Some(&err.to_string()),
            token_usage.as_ref(),
        );
        return Err(err);
    }

    // Codex can return process exit code 0 while reporting turn/task failure in JSON events.
    // Surface those runtime failures before checking for expected output files.
    if let Some(runtime_failure) = detect_codex_runtime_failure(&combined_output) {
        let status = runtime_failure
            .status_code
            .or_else(|| output.status.code().filter(|code| *code != 0));
        let mut failure_output = runtime_failure.message;
        if !output_tail.is_empty() {
            failure_output.push('\n');
            failure_output.push_str(&output_tail);
        }
        let err = if use_docker {
            RunTaskError::DockerFailed {
                status,
                output: failure_output,
            }
        } else {
            RunTaskError::CodexFailed {
                status,
                output: failure_output,
            }
        };
        if let Some(recovery_note) = maybe_recover_from_ready_reply_artifact(
            !request.reply_to.is_empty(),
            &expected_reply_path,
            status,
            &err.to_string(),
        ) {
            record_codex_success(
                &mut trace,
                status,
                &output_tail,
                Some(&recovery_note),
                token_usage.as_ref(),
            );
            return Ok(RunTaskOutput {
                reply_html_path: expected_reply_path,
                reply_attachments_dir,
                codex_output: output_tail,
                scheduled_tasks,
                scheduled_tasks_error,
                scheduler_actions,
                scheduler_actions_error,
                token_usage,
                recovery_note: Some(recovery_note),
            });
        }
        let _ = trace.finish(status, false, Some(&err.to_string()), token_usage.as_ref());
        return Err(err);
    }

    // Only check for reply file if a reply was expected
    // Use cross-channel routing to determine actual expected path
    if !request.reply_to.is_empty() && !reply_artifact_ready(&expected_reply_path) {
        let err = RunTaskError::OutputMissing {
            path: expected_reply_path,
            output: output_tail.clone(),
        };
        let _ = trace.finish(
            output.status.code(),
            false,
            Some(&err.to_string()),
            token_usage.as_ref(),
        );
        return Err(err);
    }
    record_codex_success(
        &mut trace,
        output.status.code(),
        &output_tail,
        None,
        token_usage.as_ref(),
    );

    Ok(RunTaskOutput {
        reply_html_path: expected_reply_path,
        reply_attachments_dir,
        codex_output: output_tail,
        scheduled_tasks,
        scheduled_tasks_error,
        scheduler_actions,
        scheduler_actions_error,
        token_usage,
        recovery_note: None,
    })
}

fn resolve_execution_backend() -> ExecutionBackend {
    match read_env_trimmed("RUN_TASK_EXECUTION_BACKEND")
        .unwrap_or_else(|| "auto".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "azure_aci" => ExecutionBackend::AzureAci,
        "local" => ExecutionBackend::Local,
        _ => {
            let target = normalized_deploy_target();
            if target == "staging" || target == "production" {
                ExecutionBackend::AzureAci
            } else {
                ExecutionBackend::Local
            }
        }
    }
}

fn codex_command_timeout() -> Duration {
    let overall_timeout = run_task_timeout();
    let configured_timeout = read_env_trimmed("RUN_TASK_CODEX_TIMEOUT_SECS")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .map(Duration::from_secs);

    configured_timeout
        .map(|timeout| timeout.min(overall_timeout))
        .unwrap_or(overall_timeout)
}

fn normalized_deploy_target() -> String {
    env::var("DEPLOY_TARGET")
        .unwrap_or_else(|_| "local".to_string())
        .trim()
        .to_ascii_lowercase()
}

fn required_env(key: &'static str) -> Result<String, RunTaskError> {
    read_env_trimmed(key).ok_or(RunTaskError::MissingEnv { key })
}

fn ensure_local_execution_allowed() -> Result<(), RunTaskError> {
    let target = normalized_deploy_target();
    if target == "staging" || target == "production" {
        return Err(RunTaskError::LocalExecutionForbidden {
            deploy_target: target,
        });
    }
    Ok(())
}

fn run_codex_task_azure_aci(
    request: RunTaskRequest<'_>,
    runner: &str,
    reply_html_path: PathBuf,
    reply_attachments_dir: PathBuf,
    cancel_monitor: Option<&ThreadSupersedeMonitor>,
) -> Result<RunTaskOutput, RunTaskError> {
    let _browserbase_cleanup = BrowserbaseSessionCleanupGuard::new(request.workspace_dir);
    let config = load_azure_aci_config()?;
    let mut timing = TaskTimingBuilder::new("pending");
    timing.start_stage();

    let host_workspace_dir = canonicalize_dir(request.workspace_dir)?;
    let host_share_root = canonicalize_dir(&config.host_share_root)?;
    let container_workspace_dir = map_workspace_to_container(
        &host_workspace_dir,
        &host_share_root,
        &config.container_share_root,
    )?;

    let askpass_dir = host_workspace_dir.join(DOCKER_CODEX_HOME_DIR);
    let github_auth = resolve_github_auth(Some(&askpass_dir))?;

    let api_key =
        env::var("AZURE_OPENAI_API_KEY_BACKUP").map_err(|_| RunTaskError::MissingEnv {
            key: "AZURE_OPENAI_API_KEY_BACKUP",
        })?;
    if api_key.trim().is_empty() {
        return Err(RunTaskError::MissingEnv {
            key: "AZURE_OPENAI_API_KEY_BACKUP",
        });
    }

    let azure_endpoint = azure_endpoint_from_env()?;
    let model_name = if request.model_name.trim().is_empty() {
        env::var("CODEX_MODEL").unwrap_or_else(|_| CODEX_MODEL_NAME.to_string())
    } else {
        request.model_name.to_string()
    };
    let sandbox_mode = codex_sandbox_mode();
    let channel_lower = request.channel.to_ascii_lowercase();
    let is_google_docs = channel_lower == "google_docs" || channel_lower == "googledocs";
    let has_google_token = request.workspace_dir.join(".google_access_token").exists();
    let bypass_sandbox = codex_bypass_sandbox() || is_google_docs || has_google_token;
    let sandbox_mode = effective_codex_sandbox_mode(&sandbox_mode, bypass_sandbox);

    let add_dirs = codex_add_dirs_remote(&host_workspace_dir, &container_workspace_dir)?;
    let codex_home = host_workspace_dir.join(DOCKER_CODEX_HOME_DIR);
    ensure_codex_config_at(&codex_home, &container_workspace_dir, &azure_endpoint)?;
    let payment_env_overrides = collect_payment_env_overrides();
    let bright_data_env_overrides = collect_bright_data_env_overrides();
    let google_workspace_cli_env_overrides =
        collect_google_workspace_cli_env_overrides(&host_workspace_dir)?;
    ensure_discord_context_file(&host_workspace_dir)?;
    let browserbase_env_overrides = collect_browserbase_env_overrides(&container_workspace_dir);
    let human_approval_gate_env_overrides = collect_human_approval_gate_env_overrides();
    let lark_env_overrides = collect_lark_env_overrides();

    let memory_context = load_memory_context(request.workspace_dir, request.memory_dir)?;
    let prompt = build_prompt(
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
    );

    // Remote executor reads prompt from workspace file to avoid oversized command lines.
    let prompt_path = host_workspace_dir.join(".codex_remote_prompt.txt");
    fs::write(&prompt_path, &prompt)?;

    let remote_output_path = host_workspace_dir.join(REMOTE_OUTPUT_FILENAME);
    let remote_exit_code_path = host_workspace_dir.join(REMOTE_EXIT_CODE_FILENAME);
    let _ = fs::remove_file(&remote_output_path);
    let _ = fs::remove_file(&remote_exit_code_path);

    if let Some(token) = request.google_access_token {
        // Keep token as workspace file so remote container tools can access it.
        fs::write(host_workspace_dir.join(".google_access_token"), token)?;
    }

    // Write Notion access token for channel-agnostic Notion operations
    if let Some(token) = request.notion_access_token {
        fs::write(
            host_workspace_dir.join(".notion_env"),
            format!("NOTION_API_TOKEN={}\n", token),
        )?;
    }

    let askpass_container_path = github_auth.askpass_path.as_ref().and_then(|path| {
        map_path_to_container(path, &host_workspace_dir, &container_workspace_dir)
    });
    if github_auth.askpass_path.is_some() && askpass_container_path.is_none() {
        return Err(RunTaskError::InvalidPath {
            label: "git_askpass_path",
            path: github_auth
                .askpass_path
                .clone()
                .unwrap_or_else(|| host_workspace_dir.join("missing")),
            reason: "askpass path is not within workspace_dir",
        });
    }

    let mut env_overrides = vec![
        (
            "AZURE_OPENAI_API_KEY_BACKUP".to_string(),
            api_key.to_string(),
        ),
        (
            "AZURE_OPENAI_ENDPOINT_BACKUP".to_string(),
            azure_endpoint.to_string(),
        ),
        (
            "HOME".to_string(),
            container_workspace_dir.to_string_lossy().into_owned(),
        ),
        (
            "CODEX_HOME".to_string(),
            format!(
                "{}/{}",
                container_workspace_dir.to_string_lossy(),
                DOCKER_CODEX_HOME_DIR
            ),
        ),
        ("DEPLOY_TARGET".to_string(), "azure_aci_runner".to_string()),
    ];
    for (key, value) in payment_env_overrides {
        env_overrides.push((key, value));
    }
    for (key, value) in bright_data_env_overrides {
        env_overrides.push((key, value));
    }
    for (key, value) in google_workspace_cli_env_overrides {
        if key == GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV {
            let host_path = PathBuf::from(&value);
            if let Some(container_path) =
                map_path_to_container(&host_path, &host_workspace_dir, &container_workspace_dir)
            {
                env_overrides.push((key, container_path.to_string_lossy().into_owned()));
            } else {
                eprintln!(
                    "[run_task] warning: {} is outside Azure Files workspace mount; skipping container override",
                    GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV
                );
            }
        } else {
            env_overrides.push((key, value));
        }
    }
    for (key, value) in browserbase_env_overrides {
        env_overrides.push((key, value));
    }
    for (key, value) in human_approval_gate_env_overrides {
        env_overrides.push((key, value));
    }
    for (key, value) in lark_env_overrides {
        env_overrides.push((key, value));
    }
    env_overrides.push((
        HUMAN_APPROVAL_GATE_REQUIRE_MCP_ENV_KEY.to_string(),
        "1".to_string(),
    ));
    for (key, value) in github_auth.env_overrides {
        env_overrides.push((key, value));
    }
    if let Some(ref token) = request.google_access_token {
        env_overrides.push(("GOOGLE_ACCESS_TOKEN".to_string(), token.to_string()));
        // Also set GOOGLE_WORKSPACE_CLI_TOKEN for gws CLI (third-party @googleworkspace/cli)
        // This takes highest priority and avoids OAuth refresh_token issues (invalid_rapt)
        env_overrides.push(("GOOGLE_WORKSPACE_CLI_TOKEN".to_string(), token.to_string()));
    }
    if let Some(container_path) = askpass_container_path {
        env_overrides.push((
            "GIT_ASKPASS".to_string(),
            container_path.to_string_lossy().into_owned(),
        ));
        env_overrides.push(("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()));
    }
    let mut env_overrides = dedupe_env_overrides_last_wins(&env_overrides);

    let container_name = build_aci_container_name();
    timing.set_task_id(&container_name);
    timing.end_setup();
    let timeout = codex_command_timeout();
    let mut trace = RunTaskTraceRecorder::new(
        request.workspace_dir,
        runner,
        "codex_azure_aci",
        &model_name,
        &prompt,
        timeout,
        serde_json::json!({
            "reply_expected": !request.reply_to.is_empty(),
            "sandbox_mode": sandbox_mode.clone(),
            "bypass_sandbox": bypass_sandbox,
            "container_name": container_name.clone(),
            "resource_group": config.resource_group.clone(),
            "image": config.image.clone(),
            "cpu": config.cpu.clone(),
            "memory_gb": config.memory_gb.clone(),
            "file_share": config.file_share.clone(),
            "host_workspace_dir": host_workspace_dir.to_string_lossy().into_owned(),
            "container_workspace_dir": container_workspace_dir.to_string_lossy().into_owned(),
            "add_dirs": add_dirs.clone(),
        }),
        &env_overrides,
    )?;
    let _ = trace.record_text("aci/prompt_path.txt", &prompt_path.to_string_lossy());
    let env_override_keys: Vec<&str> = env_overrides.iter().map(|(key, _)| key.as_str()).collect();
    let _ = trace.record_json("aci/env_override_keys.json", &env_override_keys);
    register_aci_container(&container_name);

    let ephemeral_guard = if use_ephemeral_share() {
        eprintln!(
            "[run_task] azure_aci ephemeral_share=true task_id={}",
            container_name
        );
        timing.start_stage();
        let guard = match EphemeralShareGuard::new(&config, &container_name, &host_workspace_dir) {
            Ok(guard) => Some(guard),
            Err(e) => {
                eprintln!("[run_task] failed to create ephemeral share: {:?}", e);
                None
            }
        };
        timing.end_ephemeral_create();
        guard
    } else {
        None
    };

    let (effective_share, effective_container_workspace) = match &ephemeral_guard {
        Some(guard) => (
            guard.share_name().to_string(),
            config.container_share_root.clone(),
        ),
        None => (config.file_share.clone(), container_workspace_dir.clone()),
    };

    let effective_add_dirs = if ephemeral_guard.is_some() {
        codex_add_dirs_remote(&host_workspace_dir, &effective_container_workspace)?
    } else {
        add_dirs.clone()
    };

    // Override HOME and CODEX_HOME when using ephemeral shares (files uploaded to share root)
    if ephemeral_guard.is_some() {
        env_overrides.push((
            "HOME".to_string(),
            effective_container_workspace.to_string_lossy().into_owned(),
        ));
        env_overrides.push((
            "CODEX_HOME".to_string(),
            format!(
                "{}/{}",
                effective_container_workspace.to_string_lossy(),
                DOCKER_CODEX_HOME_DIR
            ),
        ));
    }

    eprintln!(
        "[run_task] azure_aci create container={} resource_group={} image={}",
        container_name, config.resource_group, config.image
    );
    let execution = run_azure_aci_execution(
        &config,
        &container_name,
        &effective_container_workspace,
        &effective_add_dirs,
        &remote_exit_code_path,
        &model_name,
        &sandbox_mode,
        bypass_sandbox,
        &env_overrides,
        timeout,
        cancel_monitor,
        &effective_share,
        &mut timing,
    );
    eprintln!(
        "[run_task] azure_aci delete-request container={} resource_group={}",
        container_name, config.resource_group
    );
    if let Err(cleanup_err) =
        delete_aci_container_with_timeout(&config, &container_name, Duration::from_secs(20))
    {
        if !is_aci_not_found_error(&cleanup_err) {
            eprintln!(
                "[run_task] azure_aci delete-request failed container={} resource_group={} error={}",
                container_name, config.resource_group, cleanup_err
            );
        }
    }
    if execution.is_err() {
        eprintln!(
            "[run_task] azure_aci execution failed for container={} (cleanup requested)",
            container_name
        );
    }
    deregister_aci_container(&container_name);

    if let Some(ref guard) = ephemeral_guard {
        timing.start_stage();
        if let Err(e) = guard.download_back() {
            eprintln!(
                "[run_task] failed to download from ephemeral share: {:?}",
                e
            );
        }
        timing.end_result_download();
    }

    let output_content = fs::read_to_string(&remote_output_path).unwrap_or_default();
    let exit_status = read_remote_exit_code(&remote_exit_code_path);
    if remote_output_path.exists() {
        let _ = trace.copy_file(&remote_output_path, "aci/remote_output.log");
    }
    if remote_exit_code_path.exists() {
        let _ = trace.copy_file(&remote_exit_code_path, "aci/remote_exit_code.txt");
    }
    let execution = match execution {
        Ok(execution) => execution,
        Err(err) => {
            let _ = trace.record_text("logs/combined.log", &output_content);
            let _ = trace.finish(exit_status, false, Some(&err.to_string()), None);
            return Err(err);
        }
    };
    eprintln!(
        "[run_task] azure_aci finished container={} state={} remote_artifact_completion={}",
        container_name, execution.container_state, execution.remote_artifact_completion
    );
    if let Some(show_json) = execution.container_show_json.as_deref() {
        let _ = trace.record_text("aci/container_show.json", show_json);
    }
    let _ = trace.record_text("aci/container_logs.txt", &execution.container_logs);

    let mut combined_output = String::new();
    combined_output.push_str(&output_content);
    if !execution.container_logs.trim().is_empty() {
        if !combined_output.trim().is_empty() {
            combined_output.push('\n');
        }
        combined_output.push_str(&execution.container_logs);
    }
    let _ = trace.record_outputs(&output_content, &execution.container_logs, &combined_output);

    let (scheduled_tasks, scheduled_tasks_error, scheduler_actions, scheduler_actions_error) =
        parse_scheduling_from_outputs(
            &output_content,
            &execution.container_logs,
            &combined_output,
            request.workspace_dir,
        );
    let token_usage = extract_token_usage(&combined_output);
    let output_tail = tail_string(&combined_output, 4000);
    let expected_reply_path =
        resolve_expected_reply_path(request.workspace_dir, reply_html_path.clone());

    if !azure_aci_execution_succeeded(&execution, exit_status) {
        let err = RunTaskError::CodexFailed {
            status: exit_status,
            output: format!(
                "azure_aci_state={} remote_artifact_completion={}{}\n{}",
                execution.container_state,
                execution.remote_artifact_completion,
                match exit_status {
                    Some(code) => format!(" exit_code={code}"),
                    None => String::new(),
                },
                output_tail
            ),
        };
        if let Some(recovery_note) = maybe_recover_from_ready_reply_artifact(
            !request.reply_to.is_empty(),
            &expected_reply_path,
            exit_status,
            &err.to_string(),
        ) {
            record_codex_success(
                &mut trace,
                exit_status,
                &output_tail,
                Some(&recovery_note),
                token_usage.as_ref(),
            );
            TIMING_COLLECTOR.record(timing.finish());
            return Ok(RunTaskOutput {
                reply_html_path: expected_reply_path,
                reply_attachments_dir,
                codex_output: output_tail,
                scheduled_tasks,
                scheduled_tasks_error,
                scheduler_actions,
                scheduler_actions_error,
                token_usage,
                recovery_note: Some(recovery_note),
            });
        }
        let _ = trace.finish(
            exit_status,
            false,
            Some(&err.to_string()),
            token_usage.as_ref(),
        );
        return Err(err);
    }

    // Use cross-channel routing to determine actual expected path
    if !request.reply_to.is_empty() && !reply_artifact_ready(&expected_reply_path) {
        let err = RunTaskError::OutputMissing {
            path: expected_reply_path,
            output: output_tail,
        };
        let _ = trace.finish(
            exit_status,
            false,
            Some(&err.to_string()),
            token_usage.as_ref(),
        );
        return Err(err);
    }
    record_codex_success(
        &mut trace,
        exit_status,
        &output_tail,
        None,
        token_usage.as_ref(),
    );

    TIMING_COLLECTOR.record(timing.finish());

    Ok(RunTaskOutput {
        reply_html_path: expected_reply_path,
        reply_attachments_dir,
        codex_output: output_tail,
        scheduled_tasks,
        scheduled_tasks_error,
        scheduler_actions,
        scheduler_actions_error,
        token_usage,
        recovery_note: None,
    })
}

fn load_azure_aci_config() -> Result<AzureAciConfig, RunTaskError> {
    let resource_group = required_env("RUN_TASK_AZURE_ACI_RESOURCE_GROUP")?;
    let image = read_env_trimmed("RUN_TASK_AZURE_ACI_IMAGE")
        .or_else(|| read_env_trimmed("RUN_TASK_DOCKER_IMAGE"))
        .ok_or(RunTaskError::MissingEnv {
            key: "RUN_TASK_AZURE_ACI_IMAGE",
        })?;
    let location = read_env_trimmed("RUN_TASK_AZURE_ACI_LOCATION");
    let mut registry_server = read_env_trimmed("RUN_TASK_AZURE_ACI_REGISTRY_SERVER");
    if registry_server.is_none() {
        registry_server = image
            .split('/')
            .next()
            .filter(|candidate| candidate.contains('.'))
            .map(|value| value.to_string());
    }
    let registry_username = read_env_trimmed("RUN_TASK_AZURE_ACI_REGISTRY_USERNAME");
    let registry_password = read_env_trimmed("RUN_TASK_AZURE_ACI_REGISTRY_PASSWORD");
    if registry_username.is_some() && registry_password.is_none() {
        return Err(RunTaskError::MissingEnv {
            key: "RUN_TASK_AZURE_ACI_REGISTRY_PASSWORD",
        });
    }
    if registry_password.is_some() && registry_username.is_none() {
        return Err(RunTaskError::MissingEnv {
            key: "RUN_TASK_AZURE_ACI_REGISTRY_USERNAME",
        });
    }
    if registry_username.is_some() && registry_server.is_none() {
        return Err(RunTaskError::MissingEnv {
            key: "RUN_TASK_AZURE_ACI_REGISTRY_SERVER",
        });
    }
    let cpu = read_env_trimmed("RUN_TASK_AZURE_ACI_CPU").unwrap_or_else(|| "2.0".to_string());
    let memory_gb =
        read_env_trimmed("RUN_TASK_AZURE_ACI_MEMORY_GB").unwrap_or_else(|| "4.0".to_string());
    let file_share = read_env_trimmed("RUN_TASK_AZURE_ACI_FILE_SHARE")
        .unwrap_or_else(|| "dowhiz-run-task".to_string());

    let host_share_root = PathBuf::from(required_env("RUN_TASK_AZURE_ACI_HOST_SHARE_ROOT")?);
    let container_share_root = PathBuf::from(
        read_env_trimmed("RUN_TASK_AZURE_ACI_CONTAINER_SHARE_ROOT")
            .unwrap_or_else(|| "/mnt/dowhiz-share".to_string()),
    );

    let storage_account = read_env_trimmed("RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT")
        .or_else(|| read_env_trimmed("AZURE_STORAGE_ACCOUNT"))
        .or_else(|| {
            read_env_trimmed("AZURE_STORAGE_CONNECTION_STRING")
                .and_then(|cs| parse_connection_string_component(&cs, "AccountName"))
        })
        .ok_or(RunTaskError::MissingEnv {
            key: "RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT",
        })?;

    let storage_key = read_env_trimmed("RUN_TASK_AZURE_ACI_STORAGE_KEY")
        .or_else(|| {
            read_env_trimmed("RUN_TASK_AZURE_ACI_STORAGE_CONNECTION_STRING")
                .and_then(|cs| parse_connection_string_component(&cs, "AccountKey"))
        })
        .or_else(|| {
            read_env_trimmed("AZURE_STORAGE_CONNECTION_STRING")
                .and_then(|cs| parse_connection_string_component(&cs, "AccountKey"))
        })
        .ok_or(RunTaskError::MissingEnv {
            key: "RUN_TASK_AZURE_ACI_STORAGE_KEY",
        })?;

    Ok(AzureAciConfig {
        resource_group,
        image,
        location,
        registry_server,
        registry_username,
        registry_password,
        cpu,
        memory_gb,
        storage_account,
        storage_key,
        file_share,
        host_share_root,
        container_share_root,
    })
}

fn parse_connection_string_component(connection_string: &str, key: &str) -> Option<String> {
    for part in connection_string.split(';') {
        let mut iter = part.splitn(2, '=');
        let part_key = iter.next()?.trim();
        let value = iter.next()?.trim();
        if part_key.eq_ignore_ascii_case(key) && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn map_workspace_to_container(
    workspace_dir: &Path,
    host_share_root: &Path,
    container_share_root: &Path,
) -> Result<PathBuf, RunTaskError> {
    let relative =
        workspace_dir
            .strip_prefix(host_share_root)
            .map_err(|_| RunTaskError::InvalidPath {
                label: "workspace_dir",
                path: workspace_dir.to_path_buf(),
                reason: "workspace is outside RUN_TASK_AZURE_ACI_HOST_SHARE_ROOT",
            })?;
    Ok(container_share_root.join(relative))
}

fn use_ephemeral_share() -> bool {
    env_enabled("RUN_TASK_AZURE_ACI_EPHEMERAL_SHARE")
}

fn create_ephemeral_share(config: &AzureAciConfig, share_name: &str) -> Result<(), RunTaskError> {
    let output = Command::new("az")
        .arg("storage")
        .arg("share")
        .arg("create")
        .arg("--name")
        .arg(share_name)
        .arg("--account-name")
        .arg(&config.storage_account)
        .arg("--account-key")
        .arg(&config.storage_key)
        .output()?;
    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: format!(
                "az storage share create --name {} failed:\n{}",
                share_name,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    Ok(())
}

fn delete_ephemeral_share(config: &AzureAciConfig, share_name: &str) -> Result<(), RunTaskError> {
    let output = Command::new("az")
        .arg("storage")
        .arg("share")
        .arg("delete")
        .arg("--name")
        .arg(share_name)
        .arg("--account-name")
        .arg(&config.storage_account)
        .arg("--account-key")
        .arg(&config.storage_key)
        .arg("--delete-snapshots")
        .arg("include")
        .output()?;
    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: format!(
                "az storage share delete --name {} failed:\n{}",
                share_name,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    Ok(())
}

/// Generate a SAS token for an Azure File share (valid for 1 hour)
fn generate_share_sas(config: &AzureAciConfig, share_name: &str) -> Result<String, RunTaskError> {
    let output = Command::new("az")
        .arg("storage")
        .arg("share")
        .arg("generate-sas")
        .arg("--name")
        .arg(share_name)
        .arg("--account-name")
        .arg(&config.storage_account)
        .arg("--account-key")
        .arg(&config.storage_key)
        .arg("--permissions")
        .arg("rwdl") // read, write, delete, list
        .arg("--expiry")
        .arg(
            (Utc::now() + ChronoDuration::hours(1))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        )
        .arg("--output")
        .arg("tsv")
        .output()?;
    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: format!(
                "az storage share generate-sas failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn upload_workspace_to_share(
    config: &AzureAciConfig,
    share_name: &str,
    workspace_dir: &Path,
) -> Result<(), RunTaskError> {
    // Generate SAS token for azcopy auth
    let sas = generate_share_sas(config, share_name)?;
    let dest_url = format!(
        "https://{}.file.core.windows.net/{}?{}",
        config.storage_account, share_name, sas
    );
    let output = Command::new("azcopy")
        .arg("copy")
        .arg(format!("{}/*", workspace_dir.display()))
        .arg(&dest_url)
        .arg("--recursive")
        .output()?;
    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: format!(
                "azcopy copy to share {} failed:\n{}",
                share_name,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    Ok(())
}

fn download_workspace_from_share(
    config: &AzureAciConfig,
    share_name: &str,
    workspace_dir: &Path,
) -> Result<(), RunTaskError> {
    // Generate SAS token for azcopy auth
    let sas = generate_share_sas(config, share_name)?;
    let source_url = format!(
        "https://{}.file.core.windows.net/{}/*?{}",
        config.storage_account, share_name, sas
    );
    let output = Command::new("azcopy")
        .arg("copy")
        .arg(&source_url)
        .arg(workspace_dir)
        .arg("--recursive")
        .output()?;
    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: format!(
                "azcopy copy from share {} failed:\n{}",
                share_name,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    Ok(())
}

struct EphemeralShareGuard<'a> {
    config: &'a AzureAciConfig,
    share_name: String,
    workspace_dir: PathBuf,
}

impl<'a> EphemeralShareGuard<'a> {
    fn new(
        config: &'a AzureAciConfig,
        task_id: &str,
        workspace_dir: &Path,
    ) -> Result<Self, RunTaskError> {
        let share_name = format!("{}{}", EPHEMERAL_SHARE_PREFIX, task_id);
        create_ephemeral_share(config, &share_name)?;
        upload_workspace_to_share(config, &share_name, workspace_dir)?;
        Ok(Self {
            config,
            share_name,
            workspace_dir: workspace_dir.to_path_buf(),
        })
    }

    fn share_name(&self) -> &str {
        &self.share_name
    }

    fn download_back(&self) -> Result<(), RunTaskError> {
        download_workspace_from_share(self.config, &self.share_name, &self.workspace_dir)
    }
}

impl Drop for EphemeralShareGuard<'_> {
    fn drop(&mut self) {
        if let Err(e) = delete_ephemeral_share(self.config, &self.share_name) {
            eprintln!(
                "[ephemeral_share] failed to delete share {}: {:?}",
                self.share_name, e
            );
        }
    }
}

fn map_path_to_container(
    host_path: &Path,
    host_workspace_dir: &Path,
    container_workspace_dir: &Path,
) -> Option<PathBuf> {
    let relative = host_path.strip_prefix(host_workspace_dir).ok()?;
    Some(container_workspace_dir.join(relative))
}

fn codex_add_dirs_remote(
    host_workspace_dir: &Path,
    container_workspace_dir: &Path,
) -> Result<Vec<String>, RunTaskError> {
    let host_gh_config_dir = host_workspace_dir.join(".config").join("gh");
    fs::create_dir_all(&host_gh_config_dir)?;
    let container_gh_config_dir = container_workspace_dir.join(".config").join("gh");
    Ok(vec![container_gh_config_dir.to_string_lossy().into_owned()])
}

fn build_aci_container_name() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = ACI_CONTAINER_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dwz-codex-{}-{}-{}", millis, std::process::id(), seq)
}

fn run_azure_aci_execution(
    config: &AzureAciConfig,
    container_name: &str,
    container_workspace_dir: &Path,
    add_dirs: &[String],
    remote_exit_code_path: &Path,
    model_name: &str,
    sandbox_mode: &str,
    bypass_sandbox: bool,
    env_overrides: &[(String, String)],
    timeout: Duration,
    cancel_monitor: Option<&ThreadSupersedeMonitor>,
    file_share: &str,
    timing: &mut TaskTimingBuilder,
) -> Result<AzureAciExecutionArtifacts, RunTaskError> {
    if let Some(reason) = cancel_monitor.and_then(ThreadSupersedeMonitor::supersede_reason) {
        return Err(RunTaskError::Canceled {
            reason,
            output: "superseded before Azure ACI execution started".to_string(),
        });
    }

    let workspace_sh = shell_quote(&container_workspace_dir.to_string_lossy());
    let output_file = shell_quote(
        &container_workspace_dir
            .join(REMOTE_OUTPUT_FILENAME)
            .to_string_lossy(),
    );
    let exit_file = shell_quote(
        &container_workspace_dir
            .join(REMOTE_EXIT_CODE_FILENAME)
            .to_string_lossy(),
    );
    let output_tmp = shell_quote(&format!("/tmp/{container_name}{REMOTE_OUTPUT_FILENAME}"));
    let exit_tmp = shell_quote(&format!("/tmp/{container_name}{REMOTE_EXIT_CODE_FILENAME}"));
    let model_name_sh = shell_quote(model_name);
    let sandbox_mode_sh = shell_quote(sandbox_mode);
    let web_search_cfg = shell_quote("web_search=\"live\"");
    let ask_for_approval_cfg = shell_quote("ask_for_approval=\"never\"");
    let sandbox_cfg = shell_quote(&format!("sandbox=\"{}\"", sandbox_mode));
    let model_provider_cfg = shell_quote("model_provider=\"azure\"");
    let azure_env_cfg =
        shell_quote("model_providers.azure.env_key=\"AZURE_OPENAI_API_KEY_BACKUP\"");
    let add_dir_lines = add_dirs
        .iter()
        .map(|dir| format!("codex_cmd+=(--add-dir {})", shell_quote(dir)))
        .collect::<Vec<_>>()
        .join("\n");
    let bypass_enabled = if bypass_sandbox { "1" } else { "0" };
    let execution_started = Instant::now();

    let script = format!(
        "set -euo pipefail\n\
export PATH=/app/bin:/usr/local/cargo/bin:$PATH\n\
export PLAYWRIGHT_BROWSERS_PATH=\"${{PLAYWRIGHT_BROWSERS_PATH:-/app/.cache/ms-playwright}}\"\n\
export XDG_CACHE_HOME=\"${{XDG_CACHE_HOME:-/tmp/.cache}}\"\n\
export NPM_CONFIG_CACHE=\"${{NPM_CONFIG_CACHE:-/tmp/.npm}}\"\n\
export npm_config_cache=\"$NPM_CONFIG_CACHE\"\n\
mkdir -p \"$PLAYWRIGHT_BROWSERS_PATH\" \"$XDG_CACHE_HOME\" \"$NPM_CONFIG_CACHE\" /tmp/.local/share\n\
if [ -z \"${{PLAYWRIGHT_MCP_EXECUTABLE_PATH:-}}\" ]; then\n\
  if [ -x /opt/google/chrome/chrome ]; then\n\
    export PLAYWRIGHT_MCP_EXECUTABLE_PATH=/opt/google/chrome/chrome\n\
  else\n\
    playwright_exec=\"$(find \"$PLAYWRIGHT_BROWSERS_PATH\" -type f \\( -path '*/chrome-linux/chrome' -o -path '*/chrome-linux64/chrome' \\) 2>/dev/null | head -n1 || true)\"\n\
    if [ -n \"$playwright_exec\" ]; then\n\
      export PLAYWRIGHT_MCP_EXECUTABLE_PATH=\"$playwright_exec\"\n\
    fi\n\
  fi\n\
fi\n\
rm -f {output} {exit} {output_tmp} {exit_tmp}\n\
if ! cd {workspace}; then\n\
  printf 'workspace path unavailable: %s\\n' {workspace} > {output_tmp}\n\
  printf '%s' '1' > {exit_tmp}\n\
  cp {output_tmp} {output} 2>/dev/null || true\n\
  cp {exit_tmp} {exit} 2>/dev/null || true\n\
  exit 1\n\
fi\n\
mkdir -p .config/gh .codex\n\
codex_help=\"$(codex exec --help 2>/dev/null || true)\"\n\
codex_cmd=(codex exec --json)\n\
if printf '%s' \"$codex_help\" | grep -q -- '--search'; then\n\
  codex_cmd+=(--search)\n\
else\n\
  codex_cmd+=(-c {web_search_cfg})\n\
fi\n\
if printf '%s' \"$codex_help\" | grep -q -- '--ask-for-approval'; then\n\
  codex_cmd+=(--ask-for-approval never)\n\
else\n\
  codex_cmd+=(-c {ask_for_approval_cfg})\n\
fi\n\
if printf '%s' \"$codex_help\" | grep -q -- '--sandbox'; then\n\
  codex_cmd+=(--sandbox {sandbox_mode})\n\
else\n\
  codex_cmd+=(-c {sandbox_cfg})\n\
fi\n\
if [ \"{bypass}\" = \"1\" ]; then\n\
  if printf '%s' \"$codex_help\" | grep -q -- '--dangerously-bypass-approvals-and-sandbox'; then\n\
    codex_cmd+=(--dangerously-bypass-approvals-and-sandbox)\n\
  elif printf '%s' \"$codex_help\" | grep -q -- '--yolo'; then\n\
    codex_cmd+=(--yolo)\n\
  fi\n\
fi\n\
{add_dirs}\n\
codex_cmd+=(--skip-git-repo-check -m {model_name} -c {model_provider_cfg} -c {azure_env_cfg} --cd {workspace} \"$(cat .codex_remote_prompt.txt)\")\n\
set +e\n\
\"${{codex_cmd[@]}}\" > {output_tmp} 2>&1\n\
status=$?\n\
printf '%s' \"$status\" > {exit_tmp}\n\
cp {output_tmp} {output} 2>/dev/null || true\n\
cp {exit_tmp} {exit} 2>/dev/null || true\n\
if [ ! -f {output} ]; then\n\
  echo '[run_task] warning: failed to persist codex output to workspace' >&2\n\
fi\n\
if [ ! -f {exit} ]; then\n\
  echo '[run_task] warning: failed to persist codex exit code to workspace' >&2\n\
fi\n\
exit \"$status\"\n",
        workspace = workspace_sh,
        output = output_file,
        exit = exit_file,
        output_tmp = output_tmp,
        exit_tmp = exit_tmp,
        web_search_cfg = web_search_cfg,
        ask_for_approval_cfg = ask_for_approval_cfg,
        sandbox_mode = sandbox_mode_sh,
        sandbox_cfg = sandbox_cfg,
        bypass = bypass_enabled,
        add_dirs = add_dir_lines,
        model_name = model_name_sh,
        model_provider_cfg = model_provider_cfg,
        azure_env_cfg = azure_env_cfg,
    );

    let create_command = format!("/bin/bash -lc {}", shell_quote(&script));
    timing.start_stage();
    match create_aci_container(
        config,
        container_name,
        &create_command,
        env_overrides,
        file_share,
    ) {
        Ok(()) => {}
        Err(err) if is_aci_quota_error(&err) => {
            eprintln!(
                "[run_task] azure_aci quota reached for container={}, attempting stale cleanup",
                container_name
            );
            match cleanup_stale_aci_containers(config) {
                Ok(cleaned) => {
                    eprintln!(
                        "[run_task] azure_aci stale cleanup deleted {} container(s)",
                        cleaned
                    );
                }
                Err(cleanup_err) => {
                    eprintln!(
                        "[run_task] azure_aci stale cleanup failed before retry: {}",
                        cleanup_err
                    );
                }
            }
            create_aci_container(
                config,
                container_name,
                &create_command,
                env_overrides,
                file_share,
            )?;
        }
        Err(err) => return Err(err),
    }
    timing.end_aci_cold_start();

    let elapsed_after_create = execution_started.elapsed();
    if elapsed_after_create >= timeout {
        return Err(RunTaskError::CommandTimeout {
            command: "az container create",
            timeout_secs: timeout.as_secs(),
            output: format!(
                "container create consumed run_task timeout budget before polling (elapsed={}s)",
                elapsed_after_create.as_secs()
            ),
        });
    }
    let poll_timeout = timeout.saturating_sub(elapsed_after_create);
    timing.start_stage();
    let poll_state = poll_aci_state(
        config,
        container_name,
        remote_exit_code_path,
        poll_timeout,
        cancel_monitor,
    )?;
    timing.end_codex_execution();
    let logs = fetch_aci_logs(config, container_name).unwrap_or_default();
    let container_show_json = fetch_aci_show_json(config, container_name).ok();
    Ok(AzureAciExecutionArtifacts {
        container_state: poll_state.container_state,
        container_logs: logs,
        container_show_json,
        remote_artifact_completion: poll_state.remote_artifact_completion,
    })
}

fn poll_aci_state(
    config: &AzureAciConfig,
    container_name: &str,
    remote_exit_code_path: &Path,
    timeout: Duration,
    cancel_monitor: Option<&ThreadSupersedeMonitor>,
) -> Result<AciPollState, RunTaskError> {
    let start = Instant::now();
    let mut last_state: Option<String> = None;
    loop {
        if remote_aci_result_ready(remote_exit_code_path) {
            return Ok(AciPollState {
                container_state: last_state
                    .clone()
                    .unwrap_or_else(|| "remote_artifact_completion".to_string()),
                remote_artifact_completion: true,
            });
        }

        if let Some(reason) = cancel_monitor.and_then(ThreadSupersedeMonitor::supersede_reason) {
            let cleanup_message = match delete_aci_container_with_retry(config, container_name) {
                Ok(()) => "remote container deleted after supersede".to_string(),
                Err(err) if is_aci_not_found_error(&err) => {
                    "remote container already absent after supersede".to_string()
                }
                Err(err) => format!("failed to delete remote container after supersede: {}", err),
            };
            return Err(RunTaskError::Canceled {
                reason,
                output: cleanup_message,
            });
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(RunTaskError::CommandTimeout {
                command: "az container show",
                timeout_secs: timeout.as_secs(),
                output: "container did not reach terminal state before timeout".to_string(),
            });
        }
        let remaining = timeout.saturating_sub(elapsed);
        let show_timeout = remaining.min(Duration::from_secs(60));

        let mut show_cmd = Command::new("az");
        show_cmd
            .arg("container")
            .arg("show")
            .arg("--name")
            .arg(container_name)
            .arg("--resource-group")
            .arg(&config.resource_group)
            .arg("--query")
            .arg("instanceView.state")
            .arg("--output")
            .arg("tsv")
            .arg("--only-show-errors");
        let output = run_command_with_timeout(show_cmd, show_timeout, "az container show")?;
        if !output.status.success() {
            let mut combined = String::new();
            combined.push_str(&String::from_utf8_lossy(&output.stdout));
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
            return Err(RunTaskError::CodexFailed {
                status: output.status.code(),
                output: tail_string(&combined, 4000),
            });
        }
        let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
        last_state = Some(state.clone());
        if remote_aci_result_ready(remote_exit_code_path) {
            return Ok(AciPollState {
                container_state: state,
                remote_artifact_completion: true,
            });
        }
        if state.eq_ignore_ascii_case("Succeeded")
            || state.eq_ignore_ascii_case("Failed")
            || state.eq_ignore_ascii_case("Terminated")
            || state.eq_ignore_ascii_case("Stopped")
        {
            return Ok(AciPollState {
                container_state: state,
                remote_artifact_completion: false,
            });
        }
        if start.elapsed() >= timeout {
            return Err(RunTaskError::CommandTimeout {
                command: "az container show",
                timeout_secs: timeout.as_secs(),
                output: format!("last_state={state}"),
            });
        }
        let sleep_for = Duration::from_secs(5).min(timeout.saturating_sub(start.elapsed()));
        if !sleep_for.is_zero() {
            thread::sleep(sleep_for);
        }
    }
}

fn remote_aci_result_ready(remote_exit_code_path: &Path) -> bool {
    remote_exit_code_path.is_file()
}

fn azure_aci_execution_succeeded(
    execution: &AzureAciExecutionArtifacts,
    exit_status: Option<i32>,
) -> bool {
    (execution.container_state.eq_ignore_ascii_case("Succeeded")
        || execution.remote_artifact_completion)
        && exit_status == Some(0)
}

fn fetch_aci_logs(config: &AzureAciConfig, container_name: &str) -> Result<String, RunTaskError> {
    let mut logs_cmd = Command::new("az");
    logs_cmd
        .arg("container")
        .arg("logs")
        .arg("--name")
        .arg(container_name)
        .arg("--resource-group")
        .arg(&config.resource_group)
        .arg("--only-show-errors")
        .arg("--output")
        .arg("tsv");
    let output = run_command_with_timeout(logs_cmd, Duration::from_secs(120), "az container logs")?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn fetch_aci_show_json(
    config: &AzureAciConfig,
    container_name: &str,
) -> Result<String, RunTaskError> {
    let mut show_cmd = Command::new("az");
    show_cmd
        .arg("container")
        .arg("show")
        .arg("--name")
        .arg(container_name)
        .arg("--resource-group")
        .arg(&config.resource_group)
        .arg("--only-show-errors")
        .arg("--output")
        .arg("json");
    let output = run_command_with_timeout(show_cmd, Duration::from_secs(120), "az container show")?;
    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn create_aci_container(
    config: &AzureAciConfig,
    container_name: &str,
    create_command: &str,
    env_overrides: &[(String, String)],
    file_share: &str,
) -> Result<(), RunTaskError> {
    let mut create_cmd =
        build_aci_create_command(config, container_name, create_command, file_share);
    let env_overrides = dedupe_env_overrides_last_wins(env_overrides);
    if !env_overrides.is_empty() {
        create_cmd.arg("--environment-variables");
        for (key, value) in &env_overrides {
            create_cmd.arg(format!("{key}={value}"));
        }
    }

    let create_output =
        match run_command_with_timeout(create_cmd, Duration::from_secs(300), "az container create")
        {
            Ok(output) => output,
            Err(RunTaskError::Io(err)) if err.kind() == io::ErrorKind::NotFound => {
                return Err(RunTaskError::AzureCliNotFound)
            }
            Err(err) => return Err(err),
        };
    if !create_output.status.success() {
        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&create_output.stdout));
        combined.push_str(&String::from_utf8_lossy(&create_output.stderr));
        return Err(RunTaskError::CodexFailed {
            status: create_output.status.code(),
            output: tail_string(&combined, 4000),
        });
    }
    Ok(())
}

fn build_aci_create_command(
    config: &AzureAciConfig,
    container_name: &str,
    create_command: &str,
    file_share: &str,
) -> Command {
    let mut create_cmd = Command::new("az");
    create_cmd
        .arg("container")
        .arg("create")
        .arg("--name")
        .arg(container_name)
        .arg("--resource-group")
        .arg(&config.resource_group)
        .arg("--image")
        .arg(&config.image)
        .arg("--os-type")
        .arg("Linux")
        .arg("--restart-policy")
        .arg("Never")
        .arg("--cpu")
        .arg(&config.cpu)
        .arg("--memory")
        .arg(&config.memory_gb)
        .arg("--azure-file-volume-account-name")
        .arg(&config.storage_account)
        .arg("--azure-file-volume-account-key")
        .arg(&config.storage_key)
        .arg("--azure-file-volume-share-name")
        .arg(file_share)
        .arg("--azure-file-volume-mount-path")
        .arg(&config.container_share_root)
        .arg("--command-line")
        .arg(create_command)
        .arg("--only-show-errors")
        .arg("--output")
        .arg("json");

    if let Some(location) = &config.location {
        create_cmd.arg("--location").arg(location);
    }
    if let (Some(server), Some(username), Some(password)) = (
        &config.registry_server,
        &config.registry_username,
        &config.registry_password,
    ) {
        create_cmd
            .arg("--registry-login-server")
            .arg(server)
            .arg("--registry-username")
            .arg(username)
            .arg("--registry-password")
            .arg(password);
    }
    create_cmd
}

fn dedupe_env_overrides_last_wins(env_overrides: &[(String, String)]) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(env_overrides.len());
    for (key, value) in env_overrides.iter().rev() {
        if seen.insert(key.clone()) {
            deduped.push((key.clone(), value.clone()));
        }
    }
    deduped.reverse();
    deduped
}

fn cleanup_stale_aci_containers(config: &AzureAciConfig) -> Result<usize, RunTaskError> {
    let mut list_cmd = Command::new("az");
    list_cmd
        .arg("container")
        .arg("list")
        .arg("--resource-group")
        .arg(&config.resource_group)
        .arg("--query")
        .arg("[?starts_with(name, 'dwz-codex-') && (instanceView.state == null || instanceView.state == 'Succeeded' || instanceView.state == 'Failed' || instanceView.state == 'Terminated' || instanceView.state == 'Stopped')].name")
        .arg("--output")
        .arg("tsv")
        .arg("--only-show-errors");
    let output = run_command_with_timeout(list_cmd, Duration::from_secs(120), "az container list")?;
    if !output.status.success() {
        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&output.stdout));
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: tail_string(&combined, 4000),
        });
    }
    let names = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect::<Vec<_>>();

    let mut cleaned = 0usize;
    for name in names {
        match delete_aci_container_with_retry(config, &name) {
            Ok(()) => cleaned += 1,
            Err(err) => {
                eprintln!(
                    "[run_task] azure_aci stale cleanup delete failed container={} error={}",
                    name, err
                );
            }
        }
    }
    Ok(cleaned)
}

fn delete_aci_container_with_timeout(
    config: &AzureAciConfig,
    container_name: &str,
    command_timeout: Duration,
) -> Result<(), RunTaskError> {
    let mut delete_cmd = Command::new("az");
    delete_cmd
        .arg("container")
        .arg("delete")
        .arg("--name")
        .arg(container_name)
        .arg("--resource-group")
        .arg(&config.resource_group)
        .arg("--yes")
        .arg("--only-show-errors");
    let output = run_command_with_timeout(delete_cmd, command_timeout, "az container delete")?;
    if !output.status.success() {
        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&output.stdout));
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: tail_string(&combined, 4000),
        });
    }
    Ok(())
}

fn delete_aci_container(config: &AzureAciConfig, container_name: &str) -> Result<(), RunTaskError> {
    delete_aci_container_with_timeout(config, container_name, Duration::from_secs(120))
}

fn delete_aci_container_with_retry(
    config: &AzureAciConfig,
    container_name: &str,
) -> Result<(), RunTaskError> {
    const DELETE_ATTEMPTS: usize = 3;
    const DELETE_WAIT_TIMEOUT: Duration = Duration::from_secs(180);

    let mut last_error: Option<RunTaskError> = None;
    for attempt in 1..=DELETE_ATTEMPTS {
        match delete_aci_container(config, container_name) {
            Ok(()) => {
                match wait_for_aci_container_deleted(config, container_name, DELETE_WAIT_TIMEOUT) {
                    Ok(()) => return Ok(()),
                    Err(err) => {
                        last_error = Some(err);
                    }
                }
            }
            Err(err) if is_aci_not_found_error(&err) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
            }
        }

        if attempt < DELETE_ATTEMPTS {
            thread::sleep(Duration::from_secs((attempt as u64) * 5));
        }
    }

    Err(last_error.expect("delete_aci_container_with_retry exhausted without error"))
}

fn wait_for_aci_container_deleted(
    config: &AzureAciConfig,
    container_name: &str,
    timeout: Duration,
) -> Result<(), RunTaskError> {
    let started = Instant::now();
    loop {
        let mut show_cmd = Command::new("az");
        show_cmd
            .arg("container")
            .arg("show")
            .arg("--name")
            .arg(container_name)
            .arg("--resource-group")
            .arg(&config.resource_group)
            .arg("--only-show-errors")
            .arg("--output")
            .arg("json");
        let output =
            run_command_with_timeout(show_cmd, Duration::from_secs(60), "az container show")?;
        if output.status.success() {
            if started.elapsed() >= timeout {
                return Err(RunTaskError::CommandTimeout {
                    command: "az container delete",
                    timeout_secs: timeout.as_secs(),
                    output: format!("container {} still exists", container_name),
                });
            }
            thread::sleep(Duration::from_secs(5));
            continue;
        }

        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&output.stdout));
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
        if is_aci_not_found_output(&combined) {
            return Ok(());
        }
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: tail_string(&combined, 4000),
        });
    }
}

fn is_aci_quota_error(err: &RunTaskError) -> bool {
    let message = err.to_string().to_ascii_lowercase();
    message.contains("containergroupquotareached")
        || (message.contains("container group quota")
            && message.contains("microsoft.containerinstance/containergroups"))
        || message.contains("resource quota of container groups")
}

fn is_aci_not_found_error(err: &RunTaskError) -> bool {
    match err {
        RunTaskError::CodexFailed { output, .. } => is_aci_not_found_output(output),
        _ => false,
    }
}

fn is_aci_not_found_output(output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    lowered.contains("resourcenotfound")
        || lowered.contains("could not be found")
        || lowered.contains("was not found")
}

fn read_remote_exit_code(path: &Path) -> Option<i32> {
    fs::read_to_string(path).ok()?.trim().parse::<i32>().ok()
}

fn shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\"'\"'");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

fn ensure_codex_config(workspace_dir: &Path, azure_endpoint: &str) -> Result<(), RunTaskError> {
    let home = env::var("HOME").map_err(|_| RunTaskError::MissingEnv { key: "HOME" })?;
    let config_dir = PathBuf::from(home).join(".codex");
    ensure_codex_config_at(&config_dir, workspace_dir, azure_endpoint)
}

fn ensure_codex_config_at(
    config_dir: &Path,
    trust_workspace_dir: &Path,
    azure_endpoint: &str,
) -> Result<(), RunTaskError> {
    let config_path = config_dir.join("config.toml");
    let config_dir = config_path.parent().ok_or(RunTaskError::InvalidPath {
        label: "codex_config_dir",
        path: config_path.clone(),
        reason: "could not resolve config directory",
    })?;
    fs::create_dir_all(config_dir)?;

    let block = build_codex_config_block(azure_endpoint);
    let hag_mcp_block = build_human_approval_gate_mcp_block();

    let existing = if config_path.exists() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    let updated = update_config_block(&existing, &block);
    let updated = update_managed_config_block(
        &updated,
        HAG_MCP_CONFIG_START_MARKER,
        HAG_MCP_CONFIG_END_MARKER,
        &hag_mcp_block,
    );
    let updated = ensure_project_trust(&updated, trust_workspace_dir);
    fs::write(config_path, updated)?;
    Ok(())
}

fn ensure_project_trust(existing: &str, workspace_dir: &Path) -> String {
    let workspace_str = workspace_dir.to_string_lossy();
    let escaped = toml_escape(&workspace_str);
    let header = format!("[projects.\"{escaped}\"]");
    if existing.contains(&header) {
        return existing.to_string();
    }
    let mut updated = existing.trim_end().to_string();
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(&header);
    updated.push('\n');
    updated.push_str("trust_level = \"trusted\"\n");
    updated
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn codex_sandbox_mode() -> String {
    read_env_trimmed("CODEX_SANDBOX_MODE")
        .or_else(|| read_env_trimmed("RUN_TASK_CODEX_SANDBOX_MODE"))
        .unwrap_or_else(|| CODEX_SANDBOX_MODE.to_string())
}

fn effective_codex_sandbox_mode(sandbox_mode: &str, bypass_sandbox: bool) -> String {
    if bypass_sandbox {
        "danger-full-access".to_string()
    } else {
        sandbox_mode.to_string()
    }
}

fn codex_bypass_sandbox() -> bool {
    env_enabled("CODEX_BYPASS_SANDBOX")
}

fn employee_id_default_env_prefix(employee_id: &str) -> Option<&'static str> {
    let normalized = employee_id.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "little_bear" => Some("OLIVER"),
        "mini_mouse" => Some("MAGGIE"),
        "sticky_octopus" => Some("DEVIN"),
        "boiled_egg" => Some("PROTO"),
        _ => None,
    }
}

fn resolve_payment_env_prefix() -> Option<String> {
    read_env_trimmed("EMPLOYEE_PAYMENT_ENV_PREFIX")
        .or_else(|| read_env_trimmed("PAYMENT_ENV_PREFIX"))
        .or_else(|| read_env_trimmed("EMPLOYEE_GITHUB_ENV_PREFIX"))
        .or_else(|| read_env_trimmed("GITHUB_ENV_PREFIX"))
        .or_else(|| {
            read_env_trimmed("EMPLOYEE_ID").and_then(|id| {
                employee_id_default_env_prefix(&id)
                    .map(|value| value.to_string())
                    .or_else(|| Some(normalize_env_prefix(&id)))
            })
        })
}

fn collect_payment_env_overrides() -> Vec<(String, String)> {
    let prefix = resolve_payment_env_prefix();
    PAYMENT_ENV_KEYS
        .iter()
        .filter_map(|key| {
            read_env_trimmed(key)
                .or_else(|| {
                    prefix
                        .as_ref()
                        .and_then(|prefix| read_env_trimmed(&format!("{}_{}", prefix, key)))
                })
                .map(|value| ((*key).to_string(), value))
        })
        .collect()
}

fn collect_human_approval_gate_env_overrides() -> Vec<(String, String)> {
    let mut overrides: Vec<(String, String)> = HUMAN_APPROVAL_GATE_ENV_KEYS
        .iter()
        .filter_map(|key| read_env_trimmed(key).map(|value| ((*key).to_string(), value)))
        .collect();

    let has_human_approval_from = overrides
        .iter()
        .any(|(key, _)| key == HUMAN_APPROVAL_FROM_ENV_KEY);
    let has_human_approval_reply_to = overrides
        .iter()
        .any(|(key, _)| key == HUMAN_APPROVAL_REPLY_TO_ENV_KEY);

    if let Some(mailbox_email) = resolve_human_approval_mailbox_email_from_employee_config() {
        if !has_human_approval_from {
            overrides.push((
                HUMAN_APPROVAL_FROM_ENV_KEY.to_string(),
                mailbox_email.clone(),
            ));
        }
        if !has_human_approval_reply_to {
            overrides.push((HUMAN_APPROVAL_REPLY_TO_ENV_KEY.to_string(), mailbox_email));
        }
    }

    overrides
}

fn collect_lark_env_overrides() -> Vec<(String, String)> {
    LARK_ENV_KEYS
        .iter()
        .filter_map(|key| read_env_trimmed(key).map(|value| ((*key).to_string(), value)))
        .collect()
}

fn collect_bright_data_env_overrides() -> Vec<(String, String)> {
    let mut overrides = Vec::new();
    if let Some(api_key) = read_env_trimmed(BRIGHT_DATA_API_KEY_ENV_KEY)
        .or_else(|| read_env_trimmed(BRIGHTDATA_API_KEY_ENV_KEY))
    {
        overrides.push((BRIGHT_DATA_API_KEY_ENV_KEY.to_string(), api_key.clone()));
        overrides.push((BRIGHTDATA_API_KEY_ENV_KEY.to_string(), api_key));
    }

    for key in BRIGHT_DATA_OPTIONAL_ENV_KEYS {
        if let Some(value) = read_env_trimmed(key) {
            overrides.push(((*key).to_string(), value));
        }
    }

    overrides
}

fn resolve_human_approval_mailbox_email_from_employee_config() -> Option<String> {
    let employee_id = read_env_trimmed(EMPLOYEE_ID_ENV_KEY)?;
    for config_path in resolve_employee_config_paths() {
        if let Some(email) = load_employee_mailbox_email_from_config(&config_path, &employee_id) {
            return Some(email);
        }
    }
    None
}

fn resolve_employee_config_paths() -> Vec<PathBuf> {
    if let Some(config_path_raw) = read_env_trimmed(EMPLOYEE_CONFIG_PATH_ENV_KEY) {
        return vec![resolve_employee_config_path(&config_path_raw)];
    }

    let root = do_whiz_service_root_dir();
    let deploy_target = read_env_trimmed(DEPLOY_TARGET_ENV_KEY)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut candidates = if deploy_target == STAGING_DEPLOY_TARGET {
        vec![
            root.join("employee.staging.toml"),
            root.join("employee.toml"),
        ]
    } else {
        vec![
            root.join("employee.toml"),
            root.join("employee.staging.toml"),
        ]
    };

    candidates.retain(|path| path.exists());
    candidates
}

fn resolve_employee_config_path(raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        path
    } else {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let cwd_candidate = cwd.join(&path);
        if cwd_candidate.exists() {
            return cwd_candidate;
        }

        let service_root_candidate = do_whiz_service_root_dir().join(path);
        if service_root_candidate.exists() {
            return service_root_candidate;
        }

        cwd_candidate
    }
}

fn do_whiz_service_root_dir() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd
        .file_name()
        .map(|name| name == "DoWhiz_service")
        .unwrap_or(false)
    {
        cwd
    } else {
        cwd.join("DoWhiz_service")
    }
}

fn load_employee_mailbox_email_from_config(
    config_path: &Path,
    employee_id: &str,
) -> Option<String> {
    let content = fs::read_to_string(config_path).ok()?;
    let parsed: HumanApprovalEmployeeConfigFile = toml::from_str(&content).ok()?;
    let entry = parsed
        .employees
        .iter()
        .find(|entry| entry.id.trim().eq_ignore_ascii_case(employee_id))?;
    entry
        .addresses
        .iter()
        .map(|address| address.trim())
        .find(|address| !address.is_empty())
        .map(|address| address.to_string())
}

fn collect_google_workspace_cli_env_overrides(
    workspace_dir: &Path,
) -> Result<Vec<(String, String)>, RunTaskError> {
    let mut overrides = Vec::new();
    if let Some(path) = ensure_google_workspace_cli_credentials_file(workspace_dir)? {
        overrides.push((
            GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV.to_string(),
            path.to_string_lossy().into_owned(),
        ));
    }

    // Service Account + Domain-Wide Delegation support
    // These env vars allow google-docs CLI to use Service Account authentication
    // instead of OAuth refresh tokens (tokens never expire with Service Account)
    if let Some(sa_json) = read_env_trimmed("GOOGLE_SERVICE_ACCOUNT_JSON") {
        overrides.push(("GOOGLE_SERVICE_ACCOUNT_JSON".to_string(), sa_json));
    }
    if let Some(sa_subject) = read_env_trimmed("GOOGLE_SERVICE_ACCOUNT_SUBJECT") {
        overrides.push(("GOOGLE_SERVICE_ACCOUNT_SUBJECT".to_string(), sa_subject));
    }

    Ok(overrides)
}

#[derive(Debug)]
struct GoogleWorkspaceCliCredentialParts {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    credential_type: String,
}

fn ensure_google_workspace_cli_credentials_file(
    workspace_dir: &Path,
) -> Result<Option<PathBuf>, RunTaskError> {
    let mut unresolved_outside_workspace: Option<PathBuf> = None;
    if let Some(raw_path) = read_env_trimmed(GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV) {
        let resolved = resolve_google_workspace_cli_credentials_file_path(workspace_dir, &raw_path);
        if path_is_within_dir(&resolved, workspace_dir) {
            env::set_var(
                GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV,
                resolved.to_string_lossy().into_owned(),
            );
            return Ok(Some(resolved));
        }

        if resolved.exists() {
            let materialized =
                materialize_google_workspace_cli_credentials_file(workspace_dir, &resolved)?;
            env::set_var(
                GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV,
                materialized.to_string_lossy().into_owned(),
            );
            return Ok(Some(materialized));
        }

        eprintln!(
            "[run_task] warning: {} points outside workspace and source file does not exist: {}",
            GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV,
            resolved.display()
        );
        unresolved_outside_workspace = Some(resolved);
    }

    let Some(parts) = load_google_workspace_cli_credential_parts() else {
        if let Some(path) = unresolved_outside_workspace {
            env::set_var(
                GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV,
                path.to_string_lossy().into_owned(),
            );
            return Ok(Some(path));
        }
        return Ok(None);
    };

    let credentials_path = workspace_dir.join(GOOGLE_WORKSPACE_CLI_CREDENTIALS_REL_PATH);
    if let Some(parent) = credentials_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let payload = serde_json::json!({
        "client_id": parts.client_id,
        "client_secret": parts.client_secret,
        "refresh_token": parts.refresh_token,
        "type": parts.credential_type,
    });
    let rendered = serde_json::to_string_pretty(&payload)
        .map_err(|err| RunTaskError::Io(io::Error::other(err.to_string())))?;
    fs::write(&credentials_path, format!("{rendered}\n"))?;
    env::set_var(
        GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV,
        credentials_path.to_string_lossy().into_owned(),
    );

    Ok(Some(credentials_path))
}

fn materialize_google_workspace_cli_credentials_file(
    workspace_dir: &Path,
    source_path: &Path,
) -> Result<PathBuf, RunTaskError> {
    let credentials_path = workspace_dir.join(GOOGLE_WORKSPACE_CLI_CREDENTIALS_REL_PATH);
    if let Some(parent) = credentials_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source_path, &credentials_path).map_err(RunTaskError::Io)?;
    Ok(credentials_path)
}

fn path_is_within_dir(path: &Path, dir: &Path) -> bool {
    path.strip_prefix(dir)
        .map(|relative| {
            !relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        })
        .unwrap_or(false)
}

fn resolve_google_workspace_cli_credentials_file_path(
    workspace_dir: &Path,
    raw_path: &str,
) -> PathBuf {
    let candidate = PathBuf::from(raw_path);
    if candidate.is_absolute() {
        candidate
    } else {
        workspace_dir.join(candidate)
    }
}

fn load_google_workspace_cli_credential_parts() -> Option<GoogleWorkspaceCliCredentialParts> {
    let client_id = read_env_trimmed("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID");
    let client_secret = read_env_trimmed("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET");
    let refresh_token = read_env_trimmed("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN");
    let credential_type = read_env_trimmed("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE")
        .unwrap_or_else(|| "authorized_user".to_string());

    let has_any = GOOGLE_WORKSPACE_CLI_CREDENTIAL_COMPONENT_KEYS
        .iter()
        .any(|key| read_env_trimmed(key).is_some());
    if !has_any {
        return None;
    }

    let (Some(client_id), Some(client_secret), Some(refresh_token)) =
        (client_id, client_secret, refresh_token)
    else {
        eprintln!(
            "[run_task] warning: incomplete Google Workspace CLI credential components; expected {}, {}, {}",
            GOOGLE_WORKSPACE_CLI_CREDENTIAL_COMPONENT_KEYS[0],
            GOOGLE_WORKSPACE_CLI_CREDENTIAL_COMPONENT_KEYS[1],
            GOOGLE_WORKSPACE_CLI_CREDENTIAL_COMPONENT_KEYS[2],
        );
        return None;
    };

    Some(GoogleWorkspaceCliCredentialParts {
        client_id,
        client_secret,
        refresh_token,
        credential_type,
    })
}

/// Write `.discord_context.json` to workspace with the bot token.
/// This allows `discord_cli` to authenticate without passing the token via env var.
fn ensure_discord_context_file(workspace_dir: &Path) -> Result<(), RunTaskError> {
    let Some(token) = read_env_trimmed("DISCORD_BOT_TOKEN") else {
        // No bot token configured - skip
        return Ok(());
    };

    let context_path = workspace_dir.join(DISCORD_CONTEXT_REL_PATH);
    let payload = serde_json::json!({
        "bot_token": token,
    });
    let rendered = serde_json::to_string_pretty(&payload)
        .map_err(|err| RunTaskError::Io(io::Error::other(err.to_string())))?;
    fs::write(&context_path, format!("{rendered}\n"))?;
    eprintln!(
        "[run_task] wrote discord context file: {}",
        context_path.display()
    );
    Ok(())
}

fn codex_add_dirs(workspace_dir: &Path, use_docker: bool) -> Result<Vec<String>, RunTaskError> {
    let mut add_dirs = Vec::new();
    if use_docker {
        let gh_config_dir = workspace_dir.join(".config").join("gh");
        fs::create_dir_all(&gh_config_dir)?;
        add_dirs.push(format!("{}/.config/gh", DOCKER_WORKSPACE_DIR));
    } else {
        let home = env::var("HOME").map_err(|_| RunTaskError::MissingEnv { key: "HOME" })?;
        let gh_config_dir = PathBuf::from(home).join(".config").join("gh");
        fs::create_dir_all(&gh_config_dir)?;
        add_dirs.push(gh_config_dir.to_string_lossy().into_owned());
    }
    Ok(add_dirs)
}

fn azure_endpoint_from_env() -> Result<String, RunTaskError> {
    let endpoint =
        read_env_trimmed("AZURE_OPENAI_ENDPOINT_BACKUP").ok_or(RunTaskError::MissingEnv {
            key: "AZURE_OPENAI_ENDPOINT_BACKUP",
        })?;
    Ok(normalize_azure_endpoint(&endpoint))
}

fn build_codex_config_block(azure_endpoint: &str) -> String {
    CODEX_CONFIG_BLOCK_TEMPLATE.replace(CODEX_CONFIG_BASE_URL_PLACEHOLDER, azure_endpoint)
}

fn build_human_approval_gate_mcp_block() -> String {
    let env_vars = HUMAN_APPROVAL_GATE_ENV_KEYS
        .iter()
        .map(|key| format!(r#""{key}""#))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"{HAG_MCP_CONFIG_START_MARKER}
[mcp_servers.{HUMAN_APPROVAL_GATE_MCP_SERVER_NAME}]
command = "human_approval_gate_mcp"
env_vars = [{env_vars}]
tool_timeout_sec = {HUMAN_APPROVAL_GATE_MCP_TOOL_TIMEOUT_SECONDS}

{HAG_MCP_CONFIG_END_MARKER}"#
    )
}

fn normalize_azure_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if trimmed.ends_with("/openai/v1") {
        trimmed.to_string()
    } else {
        format!("{}/openai/v1", trimmed.trim_end_matches('/'))
    }
}

fn update_config_block(existing: &str, block: &str) -> String {
    if let Some(marker_index) = existing.find(CODEX_CONFIG_MARKER) {
        if let Some(block_end_index) = existing[marker_index..].find("wire_api = \"responses\"") {
            let end_index = marker_index + block_end_index + "wire_api = \"responses\"".len();
            let end_line_index = existing[end_index..]
                .find('\n')
                .map(|idx| end_index + idx + 1)
                .unwrap_or_else(|| existing.len());
            let mut updated = String::new();
            updated.push_str(existing[..marker_index].trim_end());
            if !updated.is_empty() {
                updated.push_str("\n\n");
            }
            updated.push_str(block.trim_end());
            updated.push('\n');
            updated.push_str(existing[end_line_index..].trim_start());
            return updated;
        }
    }

    let mut updated = existing.trim_end().to_string();
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(block.trim_end());
    updated.push('\n');
    updated
}

fn update_managed_config_block(
    existing: &str,
    start_marker: &str,
    end_marker: &str,
    block: &str,
) -> String {
    if let Some(start_index) = existing.find(start_marker) {
        if let Some(end_relative_index) = existing[start_index..].find(end_marker) {
            let end_marker_index = start_index + end_relative_index + end_marker.len();
            let trailing_newline_index = existing[end_marker_index..]
                .find('\n')
                .map(|idx| end_marker_index + idx + 1)
                .unwrap_or(existing.len());
            let mut updated = String::new();
            updated.push_str(existing[..start_index].trim_end());
            if !updated.is_empty() {
                updated.push_str("\n\n");
            }
            updated.push_str(block.trim_end());
            updated.push('\n');
            updated.push_str(existing[trailing_newline_index..].trim_start());
            return updated;
        }
    }

    let mut updated = existing.trim_end().to_string();
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(block.trim_end());
    updated.push('\n');
    updated
}

/// Parse token usage from Codex JSON output (JSONL format)
/// Looks for: {"type":"turn.completed","usage":{"input_tokens":N,"output_tokens":M}}
fn extract_token_usage(output: &str) -> Option<TokenUsage> {
    #[derive(serde::Deserialize)]
    struct TurnCompleted {
        #[serde(rename = "type")]
        event_type: String,
        usage: Option<TokenUsage>,
    }

    for line in output.lines() {
        if line.contains("\"turn.completed\"") {
            if let Ok(event) = serde_json::from_str::<TurnCompleted>(line) {
                if event.event_type == "turn.completed" {
                    return event.usage;
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexRuntimeFailure {
    status_code: Option<i32>,
    message: String,
}

fn detect_codex_runtime_failure(output: &str) -> Option<CodexRuntimeFailure> {
    enum TerminalState {
        Success,
        Failure(CodexRuntimeFailure),
    }

    let mut terminal_state: Option<TerminalState> = None;

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()) != Some("event_msg") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };

        match payload.get("type").and_then(|v| v.as_str()) {
            Some("task_complete") => {
                let status = payload.get("status").and_then(|v| v.as_str());
                let exit_code = payload
                    .get("exit_code")
                    .and_then(|v| v.as_i64())
                    .and_then(|v| i32::try_from(v).ok());
                let status_failed = matches!(status, Some("failed" | "error" | "aborted"));
                let exit_failed = matches!(exit_code, Some(code) if code != 0);

                if status_failed || exit_failed {
                    let status_text = status.unwrap_or("unknown");
                    let mut message = format!("Codex task_complete reported status={status_text}");
                    if let Some(code) = exit_code {
                        message.push_str(&format!(" exit_code={code}"));
                    }
                    if let Some(last_agent_message) =
                        payload.get("last_agent_message").and_then(|v| v.as_str())
                    {
                        let trimmed = last_agent_message.trim();
                        if !trimmed.is_empty() {
                            message.push_str(&format!(
                                ". last_agent_message: {}",
                                tail_string(trimmed, 400)
                            ));
                        }
                    }
                    terminal_state = Some(TerminalState::Failure(CodexRuntimeFailure {
                        status_code: exit_code,
                        message,
                    }));
                } else if matches!(status, Some("success")) || matches!(exit_code, Some(0)) {
                    terminal_state = Some(TerminalState::Success);
                }
            }
            Some("turn_aborted") => {
                let reason = payload
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                terminal_state = Some(TerminalState::Failure(CodexRuntimeFailure {
                    status_code: None,
                    message: format!("Codex turn aborted (reason: {reason})"),
                }));
            }
            _ => {}
        }
    }

    match terminal_state {
        Some(TerminalState::Failure(failure)) => Some(failure),
        _ => None,
    }
}

fn parse_scheduling_from_outputs(
    stdout_output: &str,
    stderr_output: &str,
    combined_output: &str,
    workspace_dir: &Path,
) -> (
    Vec<super::types::ScheduledTaskRequest>,
    Option<String>,
    Vec<super::types::SchedulerActionRequest>,
    Option<String>,
) {
    // In --json mode, assistant text lives inside JSON fields with escaping.
    // Decode assistant message payloads first, then parse scheduler blocks.
    // Codex may emit JSONL to stdout or stderr depending on runtime environment.
    let assistant_output = extract_assistant_text_from_jsonl(stdout_output)
        .or_else(|| extract_assistant_text_from_jsonl(stderr_output))
        .or_else(|| extract_assistant_text_from_jsonl(combined_output));
    let scheduling_output = assistant_output.as_deref().unwrap_or("");
    let (mut scheduled_tasks, mut scheduled_tasks_error) =
        extract_scheduled_tasks(scheduling_output);
    let (mut scheduler_actions, mut scheduler_actions_error) =
        extract_scheduler_actions(scheduling_output);

    if assistant_output.is_none() {
        // Avoid parsing prompt scaffolding as scheduler JSON when assistant extraction fails.
        // Fall back to raw output only if it yields concrete tasks/actions.
        let (fallback_tasks, fallback_tasks_error) = extract_scheduled_tasks(combined_output);
        let (fallback_actions, fallback_actions_error) = extract_scheduler_actions(combined_output);
        if !fallback_tasks.is_empty() || !fallback_actions.is_empty() {
            scheduled_tasks = fallback_tasks;
            scheduled_tasks_error = fallback_tasks_error;
            scheduler_actions = fallback_actions;
            scheduler_actions_error = fallback_actions_error;
        } else {
            scheduled_tasks_error = None;
            scheduler_actions_error = None;
        }
    }

    if scheduled_tasks.is_empty()
        && scheduler_actions.is_empty()
        && (scheduled_tasks_error.is_some() || scheduler_actions_error.is_some())
    {
        if let Some(session_output) = extract_assistant_text_from_recent_session(workspace_dir) {
            let (session_tasks, session_tasks_error) = extract_scheduled_tasks(&session_output);
            let (session_actions, session_actions_error) =
                extract_scheduler_actions(&session_output);
            if !session_tasks.is_empty() || !session_actions.is_empty() {
                scheduled_tasks = session_tasks;
                scheduled_tasks_error = session_tasks_error;
                scheduler_actions = session_actions;
                scheduler_actions_error = session_actions_error;
            }
        }
    }

    (
        scheduled_tasks,
        scheduled_tasks_error,
        scheduler_actions,
        scheduler_actions_error,
    )
}

fn extract_assistant_text_from_recent_session(workspace_dir: &Path) -> Option<String> {
    let home = env::var("HOME").ok()?;
    let sessions_root = PathBuf::from(home).join(".codex").join("sessions");
    if !sessions_root.exists() {
        return None;
    }

    let mut session_files = Vec::new();
    collect_session_jsonl_files(&sessions_root, &mut session_files).ok()?;
    session_files.sort_by(|a, b| {
        let a_time = a
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let b_time = b
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        b_time.cmp(&a_time)
    });

    let workspace_marker = workspace_dir.to_string_lossy();
    for session_path in session_files.into_iter().take(40) {
        let Ok(contents) = fs::read_to_string(&session_path) else {
            continue;
        };
        if !contents.contains(workspace_marker.as_ref()) {
            continue;
        }
        let Some(assistant_output) = extract_assistant_text_from_jsonl(&contents) else {
            continue;
        };
        if assistant_output.contains("SCHEDULED_TASKS_JSON_BEGIN")
            || assistant_output.contains("SCHEDULER_ACTIONS_JSON_BEGIN")
        {
            return Some(assistant_output);
        }
    }
    None
}

fn collect_session_jsonl_files(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_session_jsonl_files(&path, files)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
    Ok(())
}

fn extract_assistant_text_from_jsonl(output: &str) -> Option<String> {
    let mut collected = String::new();
    let mut found = false;

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if collect_item_completed_agent_message(&value, &mut collected) {
            found = true;
        }
        if collect_event_msg_agent_message(&value, &mut collected) {
            found = true;
        }
        if collect_response_item_assistant_message(&value, &mut collected) {
            found = true;
        }
    }

    found.then_some(collected)
}

fn append_collected_text(target: &mut String, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(text);
}

fn collect_item_completed_agent_message(value: &serde_json::Value, target: &mut String) -> bool {
    if value.get("type").and_then(|v| v.as_str()) != Some("item.completed") {
        return false;
    }
    let Some(item) = value.get("item") else {
        return false;
    };
    if item.get("type").and_then(|v| v.as_str()) != Some("agent_message") {
        return false;
    }
    let Some(text) = item.get("text").and_then(|v| v.as_str()) else {
        return false;
    };
    append_collected_text(target, text);
    true
}

fn collect_event_msg_agent_message(value: &serde_json::Value, target: &mut String) -> bool {
    if value.get("type").and_then(|v| v.as_str()) != Some("event_msg") {
        return false;
    }
    let Some(payload) = value.get("payload") else {
        return false;
    };
    match payload.get("type").and_then(|v| v.as_str()) {
        Some("agent_message") => {
            let Some(text) = payload.get("message").and_then(|v| v.as_str()) else {
                return false;
            };
            append_collected_text(target, text);
            true
        }
        Some("task_complete") => {
            let Some(text) = payload.get("last_agent_message").and_then(|v| v.as_str()) else {
                return false;
            };
            append_collected_text(target, text);
            true
        }
        _ => false,
    }
}

fn collect_response_item_assistant_message(value: &serde_json::Value, target: &mut String) -> bool {
    if value.get("type").and_then(|v| v.as_str()) != Some("response_item") {
        return false;
    }
    let Some(payload) = value.get("payload") else {
        return false;
    };
    if payload.get("type").and_then(|v| v.as_str()) != Some("message") {
        return false;
    }
    if payload.get("role").and_then(|v| v.as_str()) != Some("assistant") {
        return false;
    }

    let mut appended = false;
    if let Some(content) = payload.get("content").and_then(|v| v.as_array()) {
        for part in content {
            if part.get("type").and_then(|v| v.as_str()) == Some("output_text") {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    append_collected_text(target, text);
                    appended = true;
                }
            }
        }
    }
    appended
}

// ============================================================================
// Warm Pool Execution (Queue-based)
// ============================================================================

use super::pool_manager::PoolManager;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Task completion message received from the completion queue.
#[derive(Debug, serde::Deserialize)]
struct TaskCompletion {
    task_id: String,
    #[serde(default)]
    container_name: Option<String>,
    exit_code: i32,
}

/// Run a task using a warm container from the pool.
///
/// Instead of provisioning a new ACI container per task (2-4 min cold start),
/// this submits the task to a queue where pre-provisioned warm containers
/// pick it up and process it.
///
/// Flow:
/// 1. Create ephemeral share and upload workspace
/// 2. Push task message to queue (share_url, sas, agent_command)
/// 3. Wait for completion message on completion queue
/// 4. Download results from share
/// 5. Cleanup and replenish pool
pub fn run_codex_warm_pool(
    pool_manager: &PoolManager,
    request: &RunTaskParams,
    timeout: Duration,
    mut timing: TaskTimingBuilder,
) -> Result<RunTaskOutput, RunTaskError> {
    let config = load_azure_aci_config()?;
    let task_id = uuid::Uuid::new_v4().to_string();
    timing.set_task_id(&task_id);
    let workspace_dir = &request.workspace_dir;

    eprintln!(
        "[run_task] warm_pool task_id={} workspace={}",
        task_id,
        workspace_dir.display()
    );

    // 0. Build and write prompt to workspace (required by agent command)
    let memory_context = load_memory_context(&request.workspace_dir, &request.memory_dir)?;
    let prompt = build_prompt(
        &request.input_email_dir,
        &request.input_attachments_dir,
        &request.memory_dir,
        &request.reference_dir,
        &request.workspace_dir,
        &request.runner,
        &memory_context,
        !request.reply_to.is_empty(),
        &request.channel,
        request.has_unified_account,
        &request.user_identities,
    );
    let prompt_path = workspace_dir.join(".codex_remote_prompt.txt");
    fs::write(&prompt_path, &prompt)?;

    // 0b. Create codex config in workspace (will be uploaded to container)
    // Container's CODEX_HOME is set to /app/.workspace/task/.codex
    let azure_endpoint = azure_endpoint_from_env()?;
    let container_workspace = Path::new("/app/.workspace/task");
    let codex_home = workspace_dir.join(".codex");
    ensure_codex_config_at(&codex_home, container_workspace, &azure_endpoint)?;

    // 0c. Create GitHub askpass script in workspace (for git operations)
    let _ = resolve_github_auth(Some(&codex_home))?;

    // 0d. Materialize Google Workspace CLI credentials if available
    let _ = collect_google_workspace_cli_env_overrides(workspace_dir)?;

    // 0e. Write Google access token to workspace if provided
    if let Some(ref token) = request.google_access_token {
        fs::write(workspace_dir.join(".google_access_token"), token)?;
    }

    // 0f. Write Notion access token for channel-agnostic Notion operations
    if let Some(ref token) = request.notion_access_token {
        fs::write(
            workspace_dir.join(".notion_env"),
            format!("NOTION_API_TOKEN={}\n", token),
        )?;
    }

    // 1. Create ephemeral share and upload workspace
    timing.start_stage();
    let share_name = format!("task-{}", uuid::Uuid::new_v4().simple());
    create_ephemeral_share(&config, &share_name)?;
    timing.end_ephemeral_create();

    timing.start_stage();
    let upload_result = upload_workspace_to_share(&config, &share_name, workspace_dir);
    if let Err(e) = &upload_result {
        eprintln!(
            "[run_task] warm_pool upload failed, cleaning up share: {:?}",
            e
        );
        let _ = delete_ephemeral_share(&config, &share_name);
        return Err(upload_result.unwrap_err());
    }
    timing.end_ephemeral_upload();

    // 2. Generate SAS token and share URL
    let sas = generate_share_sas(&config, &share_name)?;
    let share_url = format!(
        "https://{}.file.core.windows.net/{}",
        config.storage_account, share_name
    );

    // 3. Build agent command (reuse existing logic)
    let agent_command =
        build_warm_pool_agent_command(workspace_dir, &request.model_name, &request.channel)?;

    // 4. Push task to queue
    let task_msg = serde_json::json!({
        "task_id": task_id,
        "share_url": share_url,
        "sas_token": sas,
        "agent_command": agent_command,
    });

    push_to_queue(
        pool_manager.storage_account(),
        pool_manager.storage_key(),
        pool_manager.task_queue(),
        &task_msg,
    )?;

    eprintln!("[run_task] warm_pool task pushed to queue: {}", task_id);

    // 5. Wait for completion message
    timing.start_stage();
    let completion = poll_completion_queue(
        pool_manager.storage_account(),
        pool_manager.storage_key(),
        pool_manager.completion_queue(),
        &task_id,
        timeout,
    )?;
    timing.end_codex_execution();

    eprintln!(
        "[run_task] warm_pool task completed: {} exit_code={}",
        task_id, completion.exit_code
    );

    // 6. Download results
    timing.start_stage();
    download_workspace_from_share(&config, &share_name, workspace_dir)?;
    timing.end_result_download();

    // 7. Cleanup ephemeral share
    if let Err(e) = delete_ephemeral_share(&config, &share_name) {
        eprintln!("[run_task] warm_pool failed to delete share: {:?}", e);
    }

    // 8. Delete the completed container and decrement counter
    if let Some(container_name) = &completion.container_name {
        eprintln!(
            "[run_task] warm_pool deleting container: {}",
            container_name
        );
        if let Err(e) = crate::run_task::pool_manager::delete_container(
            &pool_manager.config().resource_group,
            container_name,
        ) {
            eprintln!("[run_task] warm_pool failed to delete container: {:?}", e);
        } else {
            pool_manager.decrement_count();
        }
    } else {
        eprintln!("[run_task] warm_pool completion missing container_name, skipping delete");
    }

    // 9. Replenish pool (container exited after processing)
    pool_manager.replenish();

    // 9. Read output files and construct response
    // Use channel-specific reply path (same logic as ACI flow)
    let default_reply_path = match request.channel.to_lowercase().as_str() {
        "slack" | "discord" | "telegram" | "sms" | "whatsapp" | "bluebubbles" | "lark"
        | "wechat" | "wechat_mp" => workspace_dir.join("reply_message.txt"),
        _ => workspace_dir.join("reply_email_draft.html"),
    };
    let reply_html_path = resolve_expected_reply_path(workspace_dir, default_reply_path);
    let reply_attachments_dir = match request.channel.to_lowercase().as_str() {
        "slack" | "discord" | "telegram" | "sms" | "whatsapp" | "bluebubbles" | "lark"
        | "wechat" | "wechat_mp" | "notion" => workspace_dir.join("reply_attachments"),
        _ => workspace_dir.join("reply_email_attachments"),
    };

    let codex_output_path = workspace_dir.join("codex_output.txt");
    let codex_output = fs::read_to_string(&codex_output_path).unwrap_or_default();
    let token_usage = extract_token_usage(&codex_output);
    let recovery_note = validate_warm_pool_codex_result(
        !request.reply_to.is_empty(),
        &reply_html_path,
        completion.exit_code,
        &codex_output,
    )?;

    // Extract scheduled tasks and actions from codex output
    let (scheduled_tasks, scheduled_tasks_error) = extract_scheduled_tasks(&codex_output);
    let (scheduler_actions, scheduler_actions_error) = extract_scheduler_actions(&codex_output);

    TIMING_COLLECTOR.record(timing.finish());

    Ok(RunTaskOutput {
        reply_html_path,
        reply_attachments_dir,
        codex_output,
        scheduled_tasks,
        scheduled_tasks_error,
        scheduler_actions,
        scheduler_actions_error,
        token_usage,
        recovery_note,
    })
}

/// Build the agent command for warm pool execution.
/// Mirrors the ACI flow command with all necessary flags.
fn build_warm_pool_agent_command(
    workspace_dir: &Path,
    model_name: &str,
    channel: &str,
) -> Result<String, RunTaskError> {
    // Use model from request, fallback to env var, then constant (same as ACI flow)
    let model_name = if model_name.trim().is_empty() {
        env::var("CODEX_MODEL").unwrap_or_else(|_| CODEX_MODEL_NAME.to_string())
    } else {
        model_name.to_string()
    };
    let model_name = model_name.as_str();

    // Bypass sandbox for Google Docs (same as ACI flow)
    let channel_lower = channel.to_ascii_lowercase();
    let is_google_docs = channel_lower == "google_docs" || channel_lower == "googledocs";
    let has_google_token = workspace_dir.join(".google_access_token").exists();
    let bypass_sandbox = codex_bypass_sandbox() || is_google_docs || has_google_token;
    let sandbox_mode = effective_codex_sandbox_mode(&codex_sandbox_mode(), bypass_sandbox);

    let model_name_sh = shell_quote(model_name);
    let sandbox_mode_sh = shell_quote(&sandbox_mode);
    let web_search_cfg = shell_quote("web_search=\"live\"");
    let ask_for_approval_cfg = shell_quote("ask_for_approval=\"never\"");
    let sandbox_cfg = shell_quote(&format!("sandbox=\"{}\"", sandbox_mode));
    let model_provider_cfg = shell_quote("model_provider=\"azure\"");
    let azure_env_cfg =
        shell_quote("model_providers.azure.env_key=\"AZURE_OPENAI_API_KEY_BACKUP\"");
    let bypass_enabled = if bypass_sandbox { "1" } else { "0" };

    // Build command similar to ACI flow with feature detection
    let command = format!(
        r#"set -euo pipefail
cd "$WORKSPACE_LOCAL_DIR"
mkdir -p .config/gh .codex

# GitHub CLI/Git env vars (same as ACI flow github_auth.env_overrides)
export GH_PROMPT_DISABLED=1
export GH_NO_UPDATE_NOTIFIER=1
export GIT_EDITOR=true
export VISUAL=true
export EDITOR=true
if [ -n "${{GITHUB_USERNAME:-}}" ]; then
  export GIT_AUTHOR_NAME="${{GITHUB_USERNAME}}"
  export GIT_COMMITTER_NAME="${{GITHUB_USERNAME}}"
  export GIT_AUTHOR_EMAIL="${{GITHUB_USERNAME}}@users.noreply.github.com"
  export GIT_COMMITTER_EMAIL="${{GITHUB_USERNAME}}@users.noreply.github.com"
fi

# Export GitHub token for gh CLI (needs GH_TOKEN or GITHUB_TOKEN)
if [ -n "${{GITHUB_PERSONAL_ACCESS_TOKEN:-}}" ]; then
  export GH_TOKEN="${{GITHUB_PERSONAL_ACCESS_TOKEN}}"
  export GITHUB_TOKEN="${{GITHUB_PERSONAL_ACCESS_TOKEN}}"
fi

# Set GIT_ASKPASS to use the uploaded askpass script (same as ACI flow)
askpass_script="$(find .codex -name 'dowhiz-git-askpass-*' -type f 2>/dev/null | head -n1)"
if [ -n "$askpass_script" ] && [ -x "$askpass_script" ]; then
  export GIT_ASKPASS="$PWD/$askpass_script"
  export GIT_TERMINAL_PROMPT=0
fi

# Set Google access token env vars from file (same as ACI flow)
if [ -f .google_access_token ]; then
  token="$(cat .google_access_token)"
  export GOOGLE_ACCESS_TOKEN="$token"
  export GOOGLE_WORKSPACE_CLI_TOKEN="$token"
fi

codex_help="$(codex exec --help 2>/dev/null || true)"
codex_cmd=(codex exec --json)
if printf '%s' "$codex_help" | grep -q -- '--search'; then
  codex_cmd+=(--search)
else
  codex_cmd+=(-c {web_search_cfg})
fi
if printf '%s' "$codex_help" | grep -q -- '--ask-for-approval'; then
  codex_cmd+=(--ask-for-approval never)
else
  codex_cmd+=(-c {ask_for_approval_cfg})
fi
if printf '%s' "$codex_help" | grep -q -- '--sandbox'; then
  codex_cmd+=(--sandbox {sandbox_mode})
else
  codex_cmd+=(-c {sandbox_cfg})
fi
if [ "{bypass}" = "1" ]; then
  if printf '%s' "$codex_help" | grep -q -- '--dangerously-bypass-approvals-and-sandbox'; then
    codex_cmd+=(--dangerously-bypass-approvals-and-sandbox)
  elif printf '%s' "$codex_help" | grep -q -- '--yolo'; then
    codex_cmd+=(--yolo)
  fi
fi
codex_cmd+=(--add-dir "$WORKSPACE_LOCAL_DIR/.config/gh" --skip-git-repo-check -m {model_name} -c {model_provider_cfg} -c {azure_env_cfg} "$(cat .codex_remote_prompt.txt)")
set +e
"${{codex_cmd[@]}}" > codex_output.txt 2>&1
status=$?
printf '%s' "$status" > codex_exit_code.txt"#,
        web_search_cfg = web_search_cfg,
        ask_for_approval_cfg = ask_for_approval_cfg,
        sandbox_mode = sandbox_mode_sh,
        sandbox_cfg = sandbox_cfg,
        bypass = bypass_enabled,
        model_name = model_name_sh,
        model_provider_cfg = model_provider_cfg,
        azure_env_cfg = azure_env_cfg,
    );

    Ok(command)
}

/// Push a message to an Azure Storage Queue.
fn push_to_queue(
    account: &str,
    key: &str,
    queue: &str,
    msg: &serde_json::Value,
) -> Result<(), RunTaskError> {
    let content = BASE64.encode(msg.to_string());

    let output = Command::new("az")
        .arg("storage")
        .arg("message")
        .arg("put")
        .arg("--queue-name")
        .arg(queue)
        .arg("--account-name")
        .arg(account)
        .arg("--account-key")
        .arg(key)
        .arg("--content")
        .arg(&content)
        .arg("--output")
        .arg("none")
        .output()
        .map_err(RunTaskError::Io)?;

    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: format!(
                "az storage message put failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    Ok(())
}

/// Poll the completion queue for a specific task's completion message.
fn poll_completion_queue(
    account: &str,
    key: &str,
    queue: &str,
    task_id: &str,
    timeout: Duration,
) -> Result<TaskCompletion, RunTaskError> {
    let start = Instant::now();
    let poll_interval = Duration::from_secs(2);

    eprintln!(
        "[run_task] warm_pool polling completion queue for task_id={}",
        task_id
    );

    while start.elapsed() < timeout {
        let output = Command::new("az")
            .arg("storage")
            .arg("message")
            .arg("get")
            .arg("--queue-name")
            .arg(queue)
            .arg("--account-name")
            .arg(account)
            .arg("--account-key")
            .arg(key)
            .arg("--visibility-timeout")
            .arg("30")
            .arg("--output")
            .arg("json")
            .output()
            .map_err(RunTaskError::Io)?;

        if output.status.success() {
            let msgs: Vec<serde_json::Value> =
                serde_json::from_slice(&output.stdout).unwrap_or_default();

            for msg in msgs {
                let message_id = msg.get("id").and_then(|v| v.as_str());
                let pop_receipt = msg.get("popReceipt").and_then(|v| v.as_str());
                let content = msg.get("content").and_then(|v| v.as_str());

                if let (Some(content), Some(msg_id), Some(receipt)) =
                    (content, message_id, pop_receipt)
                {
                    if let Ok(decoded) = BASE64.decode(content) {
                        if let Ok(completion) = serde_json::from_slice::<TaskCompletion>(&decoded) {
                            if completion.task_id == task_id {
                                // Delete the message
                                let _ = delete_queue_message(account, key, queue, msg_id, receipt);
                                return Ok(completion);
                            } else {
                                // Not our task, make it visible again immediately
                                let _ = update_message_visibility(
                                    account, key, queue, msg_id, receipt, 0,
                                );
                            }
                        }
                    }
                }
            }
        }

        thread::sleep(poll_interval);
    }

    Err(RunTaskError::CommandTimeout {
        command: "poll_completion_queue",
        timeout_secs: timeout.as_secs(),
        output: format!("Timeout waiting for task completion: {}", task_id),
    })
}

/// Delete a message from an Azure Storage Queue.
fn delete_queue_message(
    account: &str,
    key: &str,
    queue: &str,
    message_id: &str,
    pop_receipt: &str,
) -> Result<(), RunTaskError> {
    let output = Command::new("az")
        .arg("storage")
        .arg("message")
        .arg("delete")
        .arg("--queue-name")
        .arg(queue)
        .arg("--account-name")
        .arg(account)
        .arg("--account-key")
        .arg(key)
        .arg("--id")
        .arg(message_id)
        .arg("--pop-receipt")
        .arg(pop_receipt)
        .arg("--output")
        .arg("none")
        .output()
        .map_err(RunTaskError::Io)?;

    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: format!(
                "az storage message delete failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    Ok(())
}

/// Update a message's visibility timeout (used to make messages visible again).
fn update_message_visibility(
    account: &str,
    key: &str,
    queue: &str,
    message_id: &str,
    pop_receipt: &str,
    visibility_timeout: u32,
) -> Result<(), RunTaskError> {
    let output = Command::new("az")
        .arg("storage")
        .arg("message")
        .arg("update")
        .arg("--queue-name")
        .arg(queue)
        .arg("--account-name")
        .arg(account)
        .arg("--account-key")
        .arg(key)
        .arg("--id")
        .arg(message_id)
        .arg("--pop-receipt")
        .arg(pop_receipt)
        .arg("--visibility-timeout")
        .arg(visibility_timeout.to_string())
        .arg("--output")
        .arg("none")
        .output()
        .map_err(RunTaskError::Io)?;

    if !output.status.success() {
        return Err(RunTaskError::CodexFailed {
            status: output.status.code(),
            output: format!(
                "az storage message update failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    struct EnvVarGuard {
        key: String,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = env::var(key).ok();
            env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }

        fn unset(key: &str) -> Self {
            let previous = env::var(key).ok();
            env::remove_var(key);
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                env::set_var(&self.key, previous);
            } else {
                env::remove_var(&self.key);
            }
        }
    }

    struct CurrentDirGuard {
        previous: PathBuf,
    }

    impl CurrentDirGuard {
        fn set(path: &Path) -> Self {
            let previous = env::current_dir().expect("read current dir");
            env::set_current_dir(path).expect("set current dir");
            Self { previous }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.previous);
        }
    }

    #[test]
    fn test_extract_token_usage_success() {
        let output = r#"{"type":"thread.started","thread_id":"abc123"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"4"}}
{"type":"turn.completed","usage":{"input_tokens":8980,"cached_input_tokens":0,"output_tokens":90}}"#;

        let usage = extract_token_usage(output);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input_tokens, 8980);
        assert_eq!(usage.cached_input_tokens, 0);
        assert_eq!(usage.output_tokens, 90);
    }

    #[test]
    fn test_extract_token_usage_no_turn_completed() {
        let output = r#"{"type":"thread.started","thread_id":"abc123"}
{"type":"turn.started"}
{"type":"error","message":"Something went wrong"}"#;

        let usage = extract_token_usage(output);
        assert!(usage.is_none());
    }

    #[test]
    fn test_extract_token_usage_no_usage_field() {
        let output = r#"{"type":"turn.completed"}"#;

        let usage = extract_token_usage(output);
        assert!(usage.is_none());
    }

    #[test]
    fn test_extract_token_usage_empty_output() {
        let usage = extract_token_usage("");
        assert!(usage.is_none());
    }

    #[test]
    fn test_extract_token_usage_with_errors_in_output() {
        // Real-world output with errors before success
        let output = r#"{"type":"thread.started","thread_id":"019ca608-b971-71a3-abfd-4cf287a3acdf"}
{"type":"turn.started"}
{"type":"error","message":"Reconnecting... 1/5"}
{"type":"error","message":"Reconnecting... 2/5"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"Done"}}
{"type":"turn.completed","usage":{"input_tokens":1000,"output_tokens":50}}"#;

        let usage = extract_token_usage(output);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn test_detect_codex_runtime_failure_from_task_complete_failed() {
        let output = r#"{"type":"event_msg","payload":{"type":"task_complete","status":"failed","exit_code":101,"last_agent_message":"compile failed"}} "#;
        let failure = detect_codex_runtime_failure(output).expect("expected failure");
        assert_eq!(failure.status_code, Some(101));
        assert!(failure.message.contains("status=failed"));
        assert!(failure.message.contains("exit_code=101"));
        assert!(failure.message.contains("compile failed"));
    }

    #[test]
    fn test_detect_codex_runtime_failure_from_turn_aborted() {
        let output =
            r#"{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#;
        let failure = detect_codex_runtime_failure(output).expect("expected failure");
        assert_eq!(failure.status_code, None);
        assert!(failure.message.contains("turn aborted"));
        assert!(failure.message.contains("interrupted"));
    }

    #[test]
    fn test_detect_codex_runtime_failure_ignores_terminal_success() {
        let output = r#"{"type":"event_msg","payload":{"type":"task_complete","status":"failed","exit_code":101}}
{"type":"event_msg","payload":{"type":"task_complete","status":"success","exit_code":0}}"#;
        assert!(detect_codex_runtime_failure(output).is_none());
    }

    #[test]
    fn test_detect_codex_runtime_failure_none_when_success_only() {
        let output = r#"{"type":"event_msg","payload":{"type":"task_complete","status":"success","exit_code":0}}"#;
        assert!(detect_codex_runtime_failure(output).is_none());
    }

    #[test]
    fn test_extract_assistant_text_from_jsonl_item_completed() {
        let output = r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"hello\nSCHEDULED_TASKS_JSON_BEGIN\n[{\"type\":\"send_email\",\"delay_seconds\":60,\"subject\":\"x\",\"html_path\":\"x.html\"}]\nSCHEDULED_TASKS_JSON_END"}}"#;

        let parsed = extract_assistant_text_from_jsonl(output);
        assert!(parsed.is_some());
        let parsed = parsed.unwrap();
        assert!(parsed.contains("SCHEDULED_TASKS_JSON_BEGIN"));
        assert!(parsed.contains("\"delay_seconds\":60"));
    }

    #[test]
    fn test_extract_assistant_text_from_jsonl_response_item_message() {
        let output = r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"SCHEDULER_ACTIONS_JSON_BEGIN\n[{\"action\":\"cancel\",\"task_ids\":[\"a\"]}]\nSCHEDULER_ACTIONS_JSON_END"}]}}"#;

        let parsed = extract_assistant_text_from_jsonl(output);
        assert!(parsed.is_some());
        let parsed = parsed.unwrap();
        assert!(parsed.contains("SCHEDULER_ACTIONS_JSON_BEGIN"));
        assert!(parsed.contains("\"action\":\"cancel\""));
    }

    #[test]
    fn test_parse_scheduling_from_outputs_reads_stderr_jsonl() {
        let stderr = r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"SCHEDULED_TASKS_JSON_BEGIN\n[{\"type\":\"send_email\",\"delay_seconds\":60,\"subject\":\"x\",\"html_path\":\"x.html\"}]\nSCHEDULED_TASKS_JSON_END"}}"#;
        let combined = format!("{stderr}\n");
        let (tasks, task_error, actions, action_error) =
            parse_scheduling_from_outputs("", stderr, &combined, Path::new("/tmp/workspace"));
        assert_eq!(tasks.len(), 1);
        assert!(task_error.is_none());
        assert!(actions.is_empty());
        assert!(action_error.is_none());
    }

    #[test]
    fn test_parse_scheduling_from_outputs_ignores_prompt_markers_without_assistant() {
        let prompt_like = concat!(
            "SCHEDULED_TASKS_JSON_BEGIN\n",
            "<JSON array here>\n",
            "SCHEDULED_TASKS_JSON_END\n",
            "SCHEDULER_ACTIONS_JSON_BEGIN\n",
            "<JSON array here>\n",
            "SCHEDULER_ACTIONS_JSON_END\n"
        );
        let (tasks, task_error, actions, action_error) =
            parse_scheduling_from_outputs("", "", prompt_like, Path::new("/tmp/workspace"));
        assert!(tasks.is_empty());
        assert!(actions.is_empty());
        assert!(task_error.is_none());
        assert!(action_error.is_none());
    }

    #[test]
    fn test_parse_scheduling_from_outputs_falls_back_to_recent_session_file() {
        let _lock = env_lock();
        let temp_root =
            std::env::temp_dir().join(format!("codex-session-fallback-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_root);
        let session_dir = temp_root.join(".codex/sessions/2026/03/01");
        fs::create_dir_all(&session_dir).expect("create session dir");
        let workspace = Path::new("/tmp/fallback-workspace");

        let session_path = session_dir.join("rollout-test.jsonl");
        let session_jsonl = format!(
            "{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"workspace: {}\"}}]}}}}\n{{\"type\":\"item.completed\",\"item\":{{\"id\":\"item_0\",\"type\":\"agent_message\",\"text\":\"SCHEDULED_TASKS_JSON_BEGIN\\n[{{\\\"type\\\":\\\"send_email\\\",\\\"delay_seconds\\\":60,\\\"subject\\\":\\\"fallback\\\",\\\"html_path\\\":\\\"x.html\\\"}}]\\nSCHEDULED_TASKS_JSON_END\"}}}}\n",
            workspace.display()
        );
        fs::write(&session_path, session_jsonl).expect("write session");

        let _home_guard = EnvVarGuard::set("HOME", temp_root.to_string_lossy().as_ref());
        let invalid_stdout = r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"SCHEDULED_TASKS_JSON_BEGIN\n<JSON>\nSCHEDULED_TASKS_JSON_END"}}"#;
        let (tasks, task_error, actions, action_error) =
            parse_scheduling_from_outputs(invalid_stdout, "", invalid_stdout, workspace);

        assert_eq!(tasks.len(), 1);
        assert!(task_error.is_none());
        assert!(actions.is_empty());
        assert!(action_error.is_none());

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn test_collect_payment_env_overrides_uses_employee_prefix_fallback() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset("GOATX402_API_URL"),
            EnvVarGuard::unset("GOATX402_API_KEY"),
            EnvVarGuard::unset("EMPLOYEE_PAYMENT_ENV_PREFIX"),
            EnvVarGuard::unset("PAYMENT_ENV_PREFIX"),
            EnvVarGuard::unset("EMPLOYEE_GITHUB_ENV_PREFIX"),
            EnvVarGuard::unset("GITHUB_ENV_PREFIX"),
            EnvVarGuard::set("EMPLOYEE_ID", "little_bear"),
            EnvVarGuard::set("OLIVER_GOATX402_API_URL", "https://example.x402.test"),
            EnvVarGuard::set("OLIVER_GOATX402_API_KEY", "api-key-prefixed"),
        ];

        let overrides = collect_payment_env_overrides();
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "GOATX402_API_URL" && v == "https://example.x402.test"));
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "GOATX402_API_KEY" && v == "api-key-prefixed"));
    }

    #[test]
    fn test_collect_payment_env_overrides_prefers_unprefixed_values() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set("EMPLOYEE_PAYMENT_ENV_PREFIX", "OLIVER"),
            EnvVarGuard::set("GOATX402_API_KEY", "api-key-global"),
            EnvVarGuard::set("OLIVER_GOATX402_API_KEY", "api-key-prefixed"),
        ];

        let overrides = collect_payment_env_overrides();
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "GOATX402_API_KEY" && v == "api-key-global"));
    }

    #[test]
    fn test_collect_bright_data_env_overrides_sets_canonical_and_alias_keys() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set(BRIGHT_DATA_API_KEY_ENV_KEY, "bright-key"),
            EnvVarGuard::unset(BRIGHTDATA_API_KEY_ENV_KEY),
            EnvVarGuard::set("BRIGHT_DATA_XIAOHONGSHU_COLLECTOR", "collector-123"),
            EnvVarGuard::unset("BRIGHT_DATA_XIAOHONGSHU_TRIGGER_URL"),
        ];

        let overrides = collect_bright_data_env_overrides();
        assert!(overrides
            .iter()
            .any(|(k, v)| { k == BRIGHT_DATA_API_KEY_ENV_KEY && v == "bright-key" }));
        assert!(overrides
            .iter()
            .any(|(k, v)| { k == BRIGHTDATA_API_KEY_ENV_KEY && v == "bright-key" }));
        assert!(overrides
            .iter()
            .any(|(k, v)| { k == "BRIGHT_DATA_XIAOHONGSHU_COLLECTOR" && v == "collector-123" }));
        assert!(!overrides
            .iter()
            .any(|(k, _)| { k == "BRIGHT_DATA_XIAOHONGSHU_TRIGGER_URL" }));
    }

    #[test]
    fn test_collect_bright_data_env_overrides_falls_back_to_cli_alias() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset(BRIGHT_DATA_API_KEY_ENV_KEY),
            EnvVarGuard::set(BRIGHTDATA_API_KEY_ENV_KEY, "alias-only-key"),
            EnvVarGuard::unset("BRIGHT_DATA_XIAOHONGSHU_COLLECTOR"),
            EnvVarGuard::set(
                "BRIGHT_DATA_XIAOHONGSHU_TRIGGER_URL",
                "https://brightdata.example/trigger",
            ),
        ];

        let overrides = collect_bright_data_env_overrides();
        assert!(overrides
            .iter()
            .any(|(k, v)| { k == BRIGHT_DATA_API_KEY_ENV_KEY && v == "alias-only-key" }));
        assert!(overrides
            .iter()
            .any(|(k, v)| { k == BRIGHTDATA_API_KEY_ENV_KEY && v == "alias-only-key" }));
        assert!(overrides.iter().any(|(k, v)| {
            k == "BRIGHT_DATA_XIAOHONGSHU_TRIGGER_URL" && v == "https://brightdata.example/trigger"
        }));
    }

    #[test]
    fn test_collect_human_approval_gate_env_overrides_collects_expected_keys() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set("POSTMARK_SERVER_TOKEN", "pm-token"),
            EnvVarGuard::set("HUMAN_APPROVAL_REPLY_TO", "inbox@example.com"),
            EnvVarGuard::set("GOOGLE_PASSWORD", "google-password"),
            EnvVarGuard::set("EMPLOYEE_CONFIG_PATH", "/tmp/missing-employee-config.toml"),
            EnvVarGuard::unset("EMPLOYEE_ID"),
        ];

        let overrides = collect_human_approval_gate_env_overrides();
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "POSTMARK_SERVER_TOKEN" && v == "pm-token"));
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "HUMAN_APPROVAL_REPLY_TO" && v == "inbox@example.com"));
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "GOOGLE_PASSWORD" && v == "google-password"));
    }

    #[test]
    fn test_collect_human_approval_gate_env_overrides_skips_unset_or_blank_values() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset("POSTMARK_SERVER_TOKEN"),
            EnvVarGuard::set("HUMAN_APPROVAL_FROM", "   "),
            EnvVarGuard::unset("HUMAN_APPROVAL_REPLY_TO"),
            EnvVarGuard::unset("POSTMARK_API_BASE_URL"),
            EnvVarGuard::unset("GOOGLE_PASSWORD"),
            EnvVarGuard::unset("EMPLOYEE_ID"),
            EnvVarGuard::set("EMPLOYEE_CONFIG_PATH", "/tmp/missing-employee-config.toml"),
        ];

        let overrides = collect_human_approval_gate_env_overrides();
        assert!(overrides.is_empty());
    }

    #[test]
    fn test_collect_human_approval_gate_env_overrides_uses_employee_config_mailbox_defaults() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("employee.staging.toml");
        fs::write(
            &config_path,
            r#"
default_employee_id = "boiled_egg"

[[employees]]
id = "boiled_egg"
addresses = ["dowhiz@deep-tutor.com"]
"#,
        )
        .expect("write employee config");

        let _guards = vec![
            EnvVarGuard::set(
                "EMPLOYEE_CONFIG_PATH",
                config_path.to_string_lossy().as_ref(),
            ),
            EnvVarGuard::set("EMPLOYEE_ID", "boiled_egg"),
            EnvVarGuard::unset("HUMAN_APPROVAL_FROM"),
            EnvVarGuard::unset("HUMAN_APPROVAL_REPLY_TO"),
            EnvVarGuard::unset("GOOGLE_PASSWORD"),
        ];

        let overrides = collect_human_approval_gate_env_overrides();
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "HUMAN_APPROVAL_FROM" && v == "dowhiz@deep-tutor.com"));
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "HUMAN_APPROVAL_REPLY_TO" && v == "dowhiz@deep-tutor.com"));
    }

    #[test]
    fn test_collect_human_approval_gate_env_overrides_resolves_relative_employee_config_path() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let service_root = temp.path().join("DoWhiz_service");
        fs::create_dir_all(&service_root).expect("create service root");

        let config_path = service_root.join("employee.staging.toml");
        fs::write(
            &config_path,
            r#"
default_employee_id = "boiled_egg"

[[employees]]
id = "boiled_egg"
addresses = ["dowhiz@deep-tutor.com"]
"#,
        )
        .expect("write employee config");

        let _cwd_guard = CurrentDirGuard::set(temp.path());
        let _guards = vec![
            EnvVarGuard::set("EMPLOYEE_CONFIG_PATH", "employee.staging.toml"),
            EnvVarGuard::set("EMPLOYEE_ID", "boiled_egg"),
            EnvVarGuard::unset("HUMAN_APPROVAL_FROM"),
            EnvVarGuard::unset("HUMAN_APPROVAL_REPLY_TO"),
            EnvVarGuard::unset("GOOGLE_PASSWORD"),
        ];

        let overrides = collect_human_approval_gate_env_overrides();
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "HUMAN_APPROVAL_FROM" && v == "dowhiz@deep-tutor.com"));
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "HUMAN_APPROVAL_REPLY_TO" && v == "dowhiz@deep-tutor.com"));
    }

    #[test]
    fn test_collect_human_approval_gate_env_overrides_keeps_explicit_human_approval_from() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("employee.toml");
        fs::write(
            &config_path,
            r#"
[[employees]]
id = "boiled_egg"
addresses = ["dowhiz@deep-tutor.com"]
"#,
        )
        .expect("write employee config");

        let _guards = vec![
            EnvVarGuard::set(
                "EMPLOYEE_CONFIG_PATH",
                config_path.to_string_lossy().as_ref(),
            ),
            EnvVarGuard::set("EMPLOYEE_ID", "boiled_egg"),
            EnvVarGuard::set("HUMAN_APPROVAL_FROM", "manual@dowhiz.com"),
            EnvVarGuard::unset("HUMAN_APPROVAL_REPLY_TO"),
            EnvVarGuard::unset("GOOGLE_PASSWORD"),
        ];

        let overrides = collect_human_approval_gate_env_overrides();
        let from_values: Vec<&String> = overrides
            .iter()
            .filter(|(key, _)| key == "HUMAN_APPROVAL_FROM")
            .map(|(_, value)| value)
            .collect();
        assert_eq!(from_values.len(), 1);
        assert_eq!(from_values[0], "manual@dowhiz.com");
        assert!(overrides
            .iter()
            .any(|(k, v)| k == "HUMAN_APPROVAL_REPLY_TO" && v == "dowhiz@deep-tutor.com"));
    }

    #[test]
    fn test_collect_google_workspace_cli_env_overrides_builds_credentials_file() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset(GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID",
                "client-id-123",
            ),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET",
                "client-secret-456",
            ),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN",
                "refresh-token-789",
            ),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE",
                "authorized_user",
            ),
        ];
        let temp = tempfile::tempdir().expect("tempdir");

        let overrides =
            collect_google_workspace_cli_env_overrides(temp.path()).expect("collect overrides");
        let credentials_path = overrides
            .iter()
            .find(|(key, _)| key == GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV)
            .map(|(_, value)| PathBuf::from(value))
            .expect("credentials path override");
        assert_eq!(
            credentials_path,
            temp.path().join(GOOGLE_WORKSPACE_CLI_CREDENTIALS_REL_PATH)
        );
        assert!(credentials_path.exists());

        let json = fs::read_to_string(&credentials_path).expect("read credentials file");
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("parse credentials json");
        assert_eq!(
            parsed.get("client_id").and_then(|value| value.as_str()),
            Some("client-id-123")
        );
        assert_eq!(
            parsed.get("client_secret").and_then(|value| value.as_str()),
            Some("client-secret-456")
        );
        assert_eq!(
            parsed.get("refresh_token").and_then(|value| value.as_str()),
            Some("refresh-token-789")
        );
        assert_eq!(
            parsed.get("type").and_then(|value| value.as_str()),
            Some("authorized_user")
        );
    }

    #[test]
    fn test_collect_google_workspace_cli_env_overrides_uses_existing_file_env() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set(
                GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV,
                ".auth/google_workspace_cli_credentials.json",
            ),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID"),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET"),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN"),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE"),
        ];
        let temp = tempfile::tempdir().expect("tempdir");

        let overrides =
            collect_google_workspace_cli_env_overrides(temp.path()).expect("collect overrides");
        let credentials_path = overrides
            .iter()
            .find(|(key, _)| key == GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV)
            .map(|(_, value)| PathBuf::from(value))
            .expect("credentials path override");
        assert_eq!(
            credentials_path,
            temp.path()
                .join(".auth/google_workspace_cli_credentials.json")
        );
    }

    #[test]
    fn test_collect_google_workspace_cli_env_overrides_materializes_external_file_env() {
        let _lock = env_lock();
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let external = tempfile::tempdir().expect("external tempdir");
        let external_file = external.path().join("credentials.json");
        let external_file_str = external_file.to_string_lossy().to_string();
        fs::write(
            &external_file,
            "{\n  \"client_id\": \"external-client\"\n}\n",
        )
        .expect("write external credentials");

        let _guards = vec![
            EnvVarGuard::set(GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV, &external_file_str),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID"),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET"),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN"),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE"),
        ];

        let overrides = collect_google_workspace_cli_env_overrides(workspace.path())
            .expect("collect overrides");
        let credentials_path = overrides
            .iter()
            .find(|(key, _)| key == GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV)
            .map(|(_, value)| PathBuf::from(value))
            .expect("credentials path override");
        assert_eq!(
            credentials_path,
            workspace
                .path()
                .join(GOOGLE_WORKSPACE_CLI_CREDENTIALS_REL_PATH)
        );
        let content = fs::read_to_string(&credentials_path).expect("read materialized credentials");
        assert!(content.contains("external-client"));
    }

    #[test]
    fn test_collect_google_workspace_cli_env_overrides_external_file_falls_back_to_components() {
        let _lock = env_lock();
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let missing_external = workspace.path().join("..").join("missing-credentials.json");
        let missing_external_str = missing_external.to_string_lossy().to_string();
        let _guards = vec![
            EnvVarGuard::set(
                GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV,
                &missing_external_str,
            ),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID",
                "fallback-client",
            ),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET",
                "fallback-secret",
            ),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN",
                "fallback-refresh",
            ),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE",
                "authorized_user",
            ),
        ];

        let overrides = collect_google_workspace_cli_env_overrides(workspace.path())
            .expect("collect overrides");
        let credentials_path = overrides
            .iter()
            .find(|(key, _)| key == GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV)
            .map(|(_, value)| PathBuf::from(value))
            .expect("credentials path override");
        assert_eq!(
            credentials_path,
            workspace
                .path()
                .join(GOOGLE_WORKSPACE_CLI_CREDENTIALS_REL_PATH)
        );
        let content = fs::read_to_string(&credentials_path).expect("read generated credentials");
        assert!(content.contains("fallback-client"));
        assert!(content.contains("fallback-secret"));
        assert!(content.contains("fallback-refresh"));
    }

    #[test]
    fn test_collect_google_workspace_cli_env_overrides_skips_when_components_incomplete() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset(GOOGLE_WORKSPACE_CLI_CREDENTIAL_FILE_ENV),
            EnvVarGuard::set(
                "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID",
                "client-id-123",
            ),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET"),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN"),
            EnvVarGuard::unset("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE"),
        ];
        let temp = tempfile::tempdir().expect("tempdir");

        let overrides =
            collect_google_workspace_cli_env_overrides(temp.path()).expect("collect overrides");
        assert!(overrides.is_empty());
        assert!(!temp
            .path()
            .join(GOOGLE_WORKSPACE_CLI_CREDENTIALS_REL_PATH)
            .exists());
    }

    #[test]
    fn test_codex_sandbox_mode_prefers_codex_sandbox_mode() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set("CODEX_SANDBOX_MODE", "danger-full-access"),
            EnvVarGuard::set("RUN_TASK_CODEX_SANDBOX_MODE", "workspace-write"),
        ];

        assert_eq!(codex_sandbox_mode(), "danger-full-access");
    }

    #[test]
    fn test_codex_bypass_sandbox_respects_unprefixed_key() {
        let _lock = env_lock();
        let _guards = vec![EnvVarGuard::set("CODEX_BYPASS_SANDBOX", "1")];

        assert!(codex_bypass_sandbox());
    }

    #[test]
    fn test_resolve_execution_backend_defaults_to_local() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset("RUN_TASK_EXECUTION_BACKEND"),
            EnvVarGuard::unset("DEPLOY_TARGET"),
        ];
        assert_eq!(resolve_execution_backend(), ExecutionBackend::Local);
    }

    #[test]
    fn test_resolve_execution_backend_auto_staging_uses_azure_aci() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset("RUN_TASK_EXECUTION_BACKEND"),
            EnvVarGuard::set("DEPLOY_TARGET", "staging"),
        ];
        assert_eq!(resolve_execution_backend(), ExecutionBackend::AzureAci);
    }

    #[test]
    fn test_resolve_execution_backend_auto_production_uses_azure_aci() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset("RUN_TASK_EXECUTION_BACKEND"),
            EnvVarGuard::set("DEPLOY_TARGET", "production"),
        ];
        assert_eq!(resolve_execution_backend(), ExecutionBackend::AzureAci);
    }

    #[test]
    fn test_resolve_execution_backend_uses_run_task_execution_backend_when_set() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set("DEPLOY_TARGET", "staging"),
            EnvVarGuard::set("RUN_TASK_EXECUTION_BACKEND", "azure_aci"),
        ];
        assert_eq!(resolve_execution_backend(), ExecutionBackend::AzureAci);
    }

    #[test]
    fn test_codex_command_timeout_defaults_to_overall_budget_when_unset() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set("RUN_TASK_TIMEOUT_SECS", "1200"),
            EnvVarGuard::unset("RUN_TASK_CODEX_TIMEOUT_SECS"),
            EnvVarGuard::unset("TASK_TIMEOUT_SECS"),
        ];

        assert_eq!(codex_command_timeout(), Duration::from_secs(1200));
    }

    #[test]
    fn test_codex_command_timeout_respects_explicit_override_and_budget() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set("RUN_TASK_TIMEOUT_SECS", "300"),
            EnvVarGuard::set("RUN_TASK_CODEX_TIMEOUT_SECS", "900"),
            EnvVarGuard::unset("TASK_TIMEOUT_SECS"),
        ];

        assert_eq!(codex_command_timeout(), Duration::from_secs(300));
    }

    #[test]
    fn test_is_aci_quota_error_detects_container_group_quota_reached() {
        let err = RunTaskError::CodexFailed {
            status: Some(1),
            output: "ERROR: (ContainerGroupQuotaReached) quota exceeded".to_string(),
        };
        assert!(is_aci_quota_error(&err));
    }

    #[test]
    fn test_is_aci_not_found_error_detects_missing_container_group() {
        let err = RunTaskError::CodexFailed {
            status: Some(3),
            output: "ERROR: (ResourceNotFound) The Resource 'Microsoft.ContainerInstance/containerGroups/dwz-codex-abc' under resource group 'rg' was not found.".to_string(),
        };
        assert!(is_aci_not_found_error(&err));
    }

    #[test]
    fn test_load_azure_aci_config_uses_unprefixed_keys() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "stg-rg"),
            EnvVarGuard::set(
                "RUN_TASK_AZURE_ACI_IMAGE",
                "stg.azurecr.io/dowhiz-service:staging",
            ),
            EnvVarGuard::set("RUN_TASK_AZURE_ACI_HOST_SHARE_ROOT", "/stg/run_task"),
            EnvVarGuard::set("RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT", "stgaccount"),
            EnvVarGuard::set("RUN_TASK_AZURE_ACI_STORAGE_KEY", "stg-key"),
            EnvVarGuard::set("RUN_TASK_AZURE_ACI_FILE_SHARE", "stg-share"),
        ];

        let config = load_azure_aci_config().expect("load aci config");
        assert_eq!(config.resource_group, "stg-rg");
        assert_eq!(config.image, "stg.azurecr.io/dowhiz-service:staging");
        assert_eq!(config.host_share_root, PathBuf::from("/stg/run_task"));
        assert_eq!(config.storage_account, "stgaccount");
        assert_eq!(config.storage_key, "stg-key");
        assert_eq!(config.file_share, "stg-share");
    }

    #[test]
    fn test_load_azure_aci_config_requires_unprefixed_resource_group() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::unset("RUN_TASK_AZURE_ACI_RESOURCE_GROUP"),
            EnvVarGuard::set(
                "RUN_TASK_AZURE_ACI_IMAGE",
                "stg.azurecr.io/dowhiz-service:staging",
            ),
            EnvVarGuard::set("RUN_TASK_AZURE_ACI_HOST_SHARE_ROOT", "/stg/run_task"),
            EnvVarGuard::set("RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT", "stgaccount"),
            EnvVarGuard::set("RUN_TASK_AZURE_ACI_STORAGE_KEY", "stg-key"),
        ];

        let err = load_azure_aci_config().expect_err("aci config should require resource group");
        match err {
            RunTaskError::MissingEnv { key } => {
                assert_eq!(key, "RUN_TASK_AZURE_ACI_RESOURCE_GROUP")
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn test_ensure_local_execution_allowed_rejects_staging_without_override() {
        let _lock = env_lock();
        let _guards = vec![
            EnvVarGuard::set("DEPLOY_TARGET", "staging"),
            EnvVarGuard::unset("RUN_TASK_ALLOW_LOCAL_EXECUTION"),
        ];
        let err = ensure_local_execution_allowed()
            .expect_err("staging should reject local execution by default");
        match err {
            RunTaskError::LocalExecutionForbidden { deploy_target } => {
                assert_eq!(deploy_target, "staging")
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn test_build_aci_container_name_is_unique() {
        let first = build_aci_container_name();
        let second = build_aci_container_name();
        assert_ne!(first, second);
    }

    #[test]
    #[cfg(unix)]
    fn test_create_aci_container_passes_bright_data_env_overrides() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin_dir = temp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let capture_path = temp.path().join("az-args.txt");
        let az_path = bin_dir.join("az");
        fs::write(
            &az_path,
            r#"#!/bin/sh
set -e
capture_file="${TEST_AZ_CAPTURE_FILE:?}"
printf '%s\n' "$@" > "$capture_file"
"#,
        )
        .expect("write fake az");
        let mut perms = fs::metadata(&az_path).expect("az metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&az_path, perms).expect("chmod fake az");

        let original_path = env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", bin_dir.display(), original_path);
        let capture_value = capture_path.to_string_lossy().to_string();
        let _guards = vec![
            EnvVarGuard::set("PATH", &path_value),
            EnvVarGuard::set("TEST_AZ_CAPTURE_FILE", &capture_value),
        ];

        let config = AzureAciConfig {
            resource_group: "stg-rg".to_string(),
            image: "stg.azurecr.io/dowhiz-service:test".to_string(),
            location: None,
            registry_server: None,
            registry_username: None,
            registry_password: None,
            cpu: "1.0".to_string(),
            memory_gb: "2.0".to_string(),
            storage_account: "storageacct".to_string(),
            storage_key: "storagekey".to_string(),
            file_share: "run-task-share".to_string(),
            host_share_root: PathBuf::from("/host/share"),
            container_share_root: PathBuf::from("/mnt/dowhiz-share"),
        };
        let env_overrides = vec![
            (
                BRIGHT_DATA_API_KEY_ENV_KEY.to_string(),
                "bright-key".to_string(),
            ),
            (
                BRIGHTDATA_API_KEY_ENV_KEY.to_string(),
                "bright-key".to_string(),
            ),
            (
                "BRIGHT_DATA_XIAOHONGSHU_COLLECTOR".to_string(),
                "collector-123".to_string(),
            ),
        ];

        create_aci_container(
            &config,
            "dwz-codex-bright-data-test",
            "/bin/bash -lc 'echo ok'",
            &env_overrides,
            &config.file_share,
        )
        .expect("create container");

        let args = fs::read_to_string(&capture_path).expect("read captured args");
        assert!(args.contains("--environment-variables"));
        assert!(args.contains("BRIGHT_DATA_API_KEY=bright-key"));
        assert!(args.contains("BRIGHTDATA_API_KEY=bright-key"));
        assert!(args.contains("BRIGHT_DATA_XIAOHONGSHU_COLLECTOR=collector-123"));
    }

    #[test]
    #[cfg(unix)]
    fn test_create_aci_container_dedupes_duplicate_env_keys_with_last_value() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin_dir = temp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let capture_path = temp.path().join("az-args.txt");
        let az_path = bin_dir.join("az");
        fs::write(
            &az_path,
            r#"#!/bin/sh
set -e
capture_file="${TEST_AZ_CAPTURE_FILE:?}"
printf '%s\n' "$@" > "$capture_file"
"#,
        )
        .expect("write fake az");
        let mut perms = fs::metadata(&az_path).expect("az metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&az_path, perms).expect("chmod fake az");

        let original_path = env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", bin_dir.display(), original_path);
        let capture_value = capture_path.to_string_lossy().to_string();
        let _guards = vec![
            EnvVarGuard::set("PATH", &path_value),
            EnvVarGuard::set("TEST_AZ_CAPTURE_FILE", &capture_value),
        ];

        let config = AzureAciConfig {
            resource_group: "stg-rg".to_string(),
            image: "stg.azurecr.io/dowhiz-service:test".to_string(),
            location: None,
            registry_server: None,
            registry_username: None,
            registry_password: None,
            cpu: "1.0".to_string(),
            memory_gb: "2.0".to_string(),
            storage_account: "storageacct".to_string(),
            storage_key: "storagekey".to_string(),
            file_share: "run-task-share".to_string(),
            host_share_root: PathBuf::from("/host/share"),
            container_share_root: PathBuf::from("/mnt/dowhiz-share"),
        };
        let env_overrides = vec![
            (
                BROWSER_HANDOFF_SIGNING_SECRET_ENV_KEY.to_string(),
                "first-secret".to_string(),
            ),
            ("OTHER_KEY".to_string(), "other-value".to_string()),
            (
                BROWSER_HANDOFF_SIGNING_SECRET_ENV_KEY.to_string(),
                "final-secret".to_string(),
            ),
        ];

        create_aci_container(
            &config,
            "dwz-codex-env-dedupe-test",
            "/bin/bash -lc 'echo ok'",
            &env_overrides,
            &config.file_share,
        )
        .expect("create container");

        let args = fs::read_to_string(&capture_path).expect("read captured args");
        assert_eq!(
            args.matches("BROWSER_HANDOFF_SIGNING_SECRET=").count(),
            1,
            "expected duplicate env key to be emitted once"
        );
        assert!(args.contains("BROWSER_HANDOFF_SIGNING_SECRET=final-secret"));
        assert!(!args.contains("BROWSER_HANDOFF_SIGNING_SECRET=first-secret"));
        assert!(args.contains("OTHER_KEY=other-value"));
    }

    #[test]
    fn test_dedupe_env_overrides_last_wins_preserves_order() {
        let env_overrides = vec![
            ("FIRST".to_string(), "1".to_string()),
            ("SHARED".to_string(), "old".to_string()),
            ("SECOND".to_string(), "2".to_string()),
            ("SHARED".to_string(), "new".to_string()),
        ];

        let deduped = dedupe_env_overrides_last_wins(&env_overrides);

        assert_eq!(
            deduped,
            vec![
                ("FIRST".to_string(), "1".to_string()),
                ("SECOND".to_string(), "2".to_string()),
                ("SHARED".to_string(), "new".to_string()),
            ]
        );
    }

    #[test]
    fn test_azure_aci_execution_succeeded_accepts_remote_artifact_completion() {
        let execution = AzureAciExecutionArtifacts {
            container_state: "Running".to_string(),
            container_logs: String::new(),
            container_show_json: None,
            remote_artifact_completion: true,
        };

        assert!(azure_aci_execution_succeeded(&execution, Some(0)));
        assert!(!azure_aci_execution_succeeded(&execution, Some(1)));
        assert!(!azure_aci_execution_succeeded(&execution, None));
    }

    #[test]
    fn test_reply_artifact_ready_rejects_empty_email_reply() {
        let temp = tempfile::tempdir().expect("tempdir");
        let reply = temp.path().join("reply_email_draft.html");
        fs::write(&reply, "   \n\t").expect("write reply");

        assert!(!reply_artifact_ready(&reply));
    }

    #[test]
    fn test_maybe_recover_from_ready_reply_artifact_returns_note() {
        let temp = tempfile::tempdir().expect("tempdir");
        let reply = temp.path().join("reply_email_draft.html");
        fs::write(&reply, "<html><body>ready</body></html>").expect("write reply");

        let note = maybe_recover_from_ready_reply_artifact(
            true,
            &reply,
            Some(23),
            "response.failed event received",
        )
        .expect("expected recovery note");

        assert!(note.contains("Recovered ready reply artifact"));
    }

    #[test]
    fn test_validate_warm_pool_codex_result_reports_nonzero_exit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let reply = temp.path().join("reply_email_draft.html");

        let err = validate_warm_pool_codex_result(true, &reply, 1, "stream disconnected")
            .expect_err("expected CodexFailed");

        match err {
            RunTaskError::CodexFailed { status, output } => {
                assert_eq!(status, Some(1));
                assert!(output.contains("stream disconnected"));
            }
            other => panic!("expected CodexFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_warm_pool_codex_result_reports_output_missing_for_empty_reply() {
        let temp = tempfile::tempdir().expect("tempdir");
        let reply = temp.path().join("reply_email_draft.html");
        fs::write(&reply, "   \n\t").expect("write empty reply");

        let err = validate_warm_pool_codex_result(
            true,
            &reply,
            0,
            r#"{"type":"event_msg","payload":{"type":"task_complete","status":"success","exit_code":0}}"#,
        )
        .expect_err("expected OutputMissing");

        assert!(matches!(err, RunTaskError::OutputMissing { .. }));
    }

    #[test]
    fn test_validate_warm_pool_codex_result_recovers_ready_reply_after_refusal() {
        let temp = tempfile::tempdir().expect("tempdir");
        let reply = temp.path().join("reply_email_draft.html");
        fs::write(&reply, "<html><body>ready</body></html>").expect("write reply");

        let note = validate_warm_pool_codex_result(
            true,
            &reply,
            1,
            "I'm sorry, but I cannot assist with that request.",
        )
        .expect("expected recovery")
        .expect("expected recovery note");

        assert!(note.contains("late Codex refusal"));
    }

    #[test]
    #[cfg(unix)]
    fn test_poll_aci_state_returns_immediately_when_remote_exit_code_exists() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin_dir = temp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let capture_path = temp.path().join("az-called.txt");
        let az_path = bin_dir.join("az");
        fs::write(
            &az_path,
            format!(
                "#!/bin/sh\nset -e\nprintf 'called' > '{}'\nprintf 'Running'\n",
                capture_path.display()
            ),
        )
        .expect("write fake az");
        let mut perms = fs::metadata(&az_path).expect("az metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&az_path, perms).expect("chmod fake az");

        let remote_exit_code_path = temp.path().join(REMOTE_EXIT_CODE_FILENAME);
        fs::write(&remote_exit_code_path, "0").expect("write remote exit code");

        let original_path = env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", bin_dir.display(), original_path);
        let _guards = vec![EnvVarGuard::set("PATH", &path_value)];

        let config = AzureAciConfig {
            resource_group: "stg-rg".to_string(),
            image: "stg.azurecr.io/dowhiz-service:test".to_string(),
            location: None,
            registry_server: None,
            registry_username: None,
            registry_password: None,
            cpu: "1.0".to_string(),
            memory_gb: "2.0".to_string(),
            storage_account: "storageacct".to_string(),
            storage_key: "storagekey".to_string(),
            file_share: "run-task-share".to_string(),
            host_share_root: PathBuf::from("/host/share"),
            container_share_root: PathBuf::from("/mnt/dowhiz-share"),
        };

        let poll_state = poll_aci_state(
            &config,
            "dwz-codex-test",
            &remote_exit_code_path,
            Duration::from_secs(2),
            None,
        )
        .expect("poll state");

        assert_eq!(poll_state.container_state, "remote_artifact_completion");
        assert!(poll_state.remote_artifact_completion);
        assert!(
            !capture_path.exists(),
            "az should not be queried once the remote exit artifact is already present"
        );
    }

    #[test]
    fn test_register_and_deregister_aci_container() {
        let name = "test-container-12345";
        register_aci_container(name);
        {
            let containers = ACTIVE_ACI_CONTAINERS.lock().unwrap();
            assert!(containers.contains(name));
        }
        deregister_aci_container(name);
        {
            let containers = ACTIVE_ACI_CONTAINERS.lock().unwrap();
            assert!(!containers.contains(name));
        }
    }

    /// E2E test that creates a real ACI container and verifies cleanup works.
    /// Run with: cargo test -p run_task_module test_aci_cleanup_e2e -- --ignored --nocapture
    #[test]
    #[ignore]
    fn test_aci_cleanup_e2e() {
        // Skip if Azure credentials not configured
        let config = match load_azure_aci_config() {
            Ok(config) => config,
            Err(err) => {
                eprintln!("Skipping ACI cleanup E2E test: {:?}", err);
                return;
            }
        };

        // Create a minimal container that just sleeps
        let container_name = build_aci_container_name();
        eprintln!("[test] Creating ACI container: {}", container_name);

        let mut create_cmd = Command::new("az");
        create_cmd
            .arg("container")
            .arg("create")
            .arg("--name")
            .arg(&container_name)
            .arg("--resource-group")
            .arg(&config.resource_group)
            .arg("--image")
            .arg("mcr.microsoft.com/azuredocs/aci-helloworld:latest")
            .arg("--os-type")
            .arg("Linux")
            .arg("--restart-policy")
            .arg("Never")
            .arg("--cpu")
            .arg("0.5")
            .arg("--memory")
            .arg("0.5")
            .arg("--only-show-errors")
            .arg("--output")
            .arg("json");

        let create_output = create_cmd
            .output()
            .expect("failed to run az container create");
        if !create_output.status.success() {
            let stderr = String::from_utf8_lossy(&create_output.stderr);
            panic!("Failed to create test container: {}", stderr);
        }
        eprintln!("[test] Container created successfully");

        // Register it (simulating what run_codex_task_azure_aci does)
        register_aci_container(&container_name);
        {
            let containers = ACTIVE_ACI_CONTAINERS.lock().unwrap();
            assert!(
                containers.contains(&container_name),
                "Container should be registered"
            );
        }

        // Now call cleanup (simulating shutdown without normal deletion)
        eprintln!("[test] Calling cleanup_all_aci_containers...");
        let cleaned = cleanup_all_aci_containers();
        assert_eq!(cleaned, 1, "Should have cleaned up 1 container");

        // Verify the registry is empty
        {
            let containers = ACTIVE_ACI_CONTAINERS.lock().unwrap();
            assert!(
                containers.is_empty(),
                "Registry should be empty after cleanup"
            );
        }

        // Verify container is actually deleted by trying to show it
        eprintln!("[test] Verifying container is deleted...");
        let mut show_cmd = Command::new("az");
        show_cmd
            .arg("container")
            .arg("show")
            .arg("--name")
            .arg(&container_name)
            .arg("--resource-group")
            .arg(&config.resource_group)
            .arg("--only-show-errors");

        let show_output = show_cmd.output().expect("failed to run az container show");
        assert!(
            !show_output.status.success(),
            "Container should not exist after cleanup"
        );
        eprintln!("[test] SUCCESS: Container was deleted by cleanup!");
    }

    #[test]
    fn test_resolve_expected_reply_path_with_cross_channel_routing_to_email() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let workspace = temp_dir.path();

        // Write reply_routing.json specifying email target
        let routing = r#"{"channel": "email", "identifier": "user@example.com"}"#;
        fs::write(workspace.join("reply_routing.json"), routing).expect("write routing file");

        let default_path = workspace.join("reply_message.txt");
        let resolved = resolve_expected_reply_path(workspace, default_path.clone());

        // Should resolve to reply_email_draft.html for email target
        assert_eq!(resolved, workspace.join("reply_email_draft.html"));
        assert_ne!(resolved, default_path);
    }

    #[test]
    fn test_resolve_expected_reply_path_fallback_when_no_routing_file() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let workspace = temp_dir.path();

        // No reply_routing.json exists
        let default_path = workspace.join("reply_message.txt");
        let resolved = resolve_expected_reply_path(workspace, default_path.clone());

        // Should return the default path as fallback
        assert_eq!(resolved, default_path);
    }

    #[test]
    fn test_use_ephemeral_share_disabled_by_default() {
        let _lock = env_lock();
        let _guard = EnvVarGuard::unset("RUN_TASK_AZURE_ACI_EPHEMERAL_SHARE");
        assert!(!use_ephemeral_share());
    }

    #[test]
    fn test_use_ephemeral_share_enabled_with_1() {
        let _lock = env_lock();
        let _guard = EnvVarGuard::set("RUN_TASK_AZURE_ACI_EPHEMERAL_SHARE", "1");
        assert!(use_ephemeral_share());
    }

    #[test]
    fn test_use_ephemeral_share_enabled_with_true() {
        let _lock = env_lock();
        let _guard = EnvVarGuard::set("RUN_TASK_AZURE_ACI_EPHEMERAL_SHARE", "true");
        assert!(use_ephemeral_share());
    }

    #[test]
    fn test_use_ephemeral_share_disabled_with_0() {
        let _lock = env_lock();
        let _guard = EnvVarGuard::set("RUN_TASK_AZURE_ACI_EPHEMERAL_SHARE", "0");
        assert!(!use_ephemeral_share());
    }

    #[test]
    fn test_ephemeral_share_prefix_format() {
        assert_eq!(EPHEMERAL_SHARE_PREFIX, "task-");
        let task_id = "dwz-codex-123-456-0";
        let share_name = format!("{}{}", EPHEMERAL_SHARE_PREFIX, task_id);
        assert_eq!(share_name, "task-dwz-codex-123-456-0");
    }

    #[test]
    #[cfg(unix)]
    fn test_create_ephemeral_share_calls_az_storage_share_create() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin_dir = temp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let capture_path = temp.path().join("az-args.txt");
        let az_path = bin_dir.join("az");
        fs::write(
            &az_path,
            r#"#!/bin/sh
set -e
capture_file="${TEST_AZ_CAPTURE_FILE:?}"
printf '%s\n' "$@" > "$capture_file"
"#,
        )
        .expect("write fake az");
        let mut perms = fs::metadata(&az_path).expect("az metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&az_path, perms).expect("chmod fake az");

        let original_path = env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", bin_dir.display(), original_path);
        let capture_value = capture_path.to_string_lossy().to_string();
        let _guards = vec![
            EnvVarGuard::set("PATH", &path_value),
            EnvVarGuard::set("TEST_AZ_CAPTURE_FILE", &capture_value),
        ];

        let config = AzureAciConfig {
            resource_group: "test-rg".to_string(),
            image: "test.azurecr.io/image:tag".to_string(),
            location: None,
            registry_server: None,
            registry_username: None,
            registry_password: None,
            cpu: "1.0".to_string(),
            memory_gb: "2.0".to_string(),
            storage_account: "teststorage".to_string(),
            storage_key: "testkey123".to_string(),
            file_share: "main-share".to_string(),
            host_share_root: PathBuf::from("/mnt/share"),
            container_share_root: PathBuf::from("/mnt/dowhiz-share"),
        };

        create_ephemeral_share(&config, "task-test-123").expect("create share");

        let args = fs::read_to_string(&capture_path).expect("read captured args");
        assert!(args.contains("storage"));
        assert!(args.contains("share"));
        assert!(args.contains("create"));
        assert!(args.contains("--name"));
        assert!(args.contains("task-test-123"));
        assert!(args.contains("--account-name"));
        assert!(args.contains("teststorage"));
        assert!(args.contains("--account-key"));
        assert!(args.contains("testkey123"));
    }

    #[test]
    #[cfg(unix)]
    fn test_upload_workspace_to_share_calls_azcopy() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin_dir = temp.path().join("bin");
        let workspace_dir = temp.path().join("workspace");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&workspace_dir).expect("create workspace dir");
        fs::write(workspace_dir.join("test.txt"), "test content").expect("write test file");

        // Fake az that outputs a fake SAS token when called with generate-sas
        let az_path = bin_dir.join("az");
        fs::write(
            &az_path,
            r#"#!/bin/sh
if echo "$@" | grep -q "generate-sas"; then
    echo "sv=2022-11-02&ss=f&srt=sco&sp=rwdlc&se=2099-01-01&sig=fakesig"
else
    exit 0
fi
"#,
        )
        .expect("write fake az");
        let mut az_perms = fs::metadata(&az_path).expect("az metadata").permissions();
        az_perms.set_mode(0o755);
        fs::set_permissions(&az_path, az_perms).expect("chmod fake az");

        let capture_path = temp.path().join("azcopy-args.txt");
        let azcopy_path = bin_dir.join("azcopy");
        fs::write(
            &azcopy_path,
            r#"#!/bin/sh
set -e
capture_file="${TEST_AZCOPY_CAPTURE_FILE:?}"
printf '%s\n' "$@" > "$capture_file"
"#,
        )
        .expect("write fake azcopy");
        let mut perms = fs::metadata(&azcopy_path)
            .expect("azcopy metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&azcopy_path, perms).expect("chmod fake azcopy");

        let original_path = env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", bin_dir.display(), original_path);
        let capture_value = capture_path.to_string_lossy().to_string();
        let _guards = vec![
            EnvVarGuard::set("PATH", &path_value),
            EnvVarGuard::set("TEST_AZCOPY_CAPTURE_FILE", &capture_value),
        ];

        let config = AzureAciConfig {
            resource_group: "test-rg".to_string(),
            image: "test.azurecr.io/image:tag".to_string(),
            location: None,
            registry_server: None,
            registry_username: None,
            registry_password: None,
            cpu: "1.0".to_string(),
            memory_gb: "2.0".to_string(),
            storage_account: "teststorage".to_string(),
            storage_key: "testkey123".to_string(),
            file_share: "main-share".to_string(),
            host_share_root: PathBuf::from("/mnt/share"),
            container_share_root: PathBuf::from("/mnt/dowhiz-share"),
        };

        upload_workspace_to_share(&config, "task-test-123", &workspace_dir).expect("upload");

        let args = fs::read_to_string(&capture_path).expect("read captured args");
        assert!(args.contains("copy"));
        assert!(args.contains("--recursive"));
        assert!(args.contains("teststorage.file.core.windows.net"));
        assert!(args.contains("task-test-123"));
        assert!(args.contains(&workspace_dir.to_string_lossy().to_string()));
    }

    #[test]
    #[cfg(unix)]
    fn test_download_workspace_from_share_calls_azcopy() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin_dir = temp.path().join("bin");
        let workspace_dir = temp.path().join("workspace");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&workspace_dir).expect("create workspace dir");

        // Fake az that outputs a fake SAS token when called with generate-sas
        let az_path = bin_dir.join("az");
        fs::write(
            &az_path,
            r#"#!/bin/sh
if echo "$@" | grep -q "generate-sas"; then
    echo "sv=2022-11-02&ss=f&srt=sco&sp=rwdlc&se=2099-01-01&sig=fakesig"
else
    exit 0
fi
"#,
        )
        .expect("write fake az");
        let mut az_perms = fs::metadata(&az_path).expect("az metadata").permissions();
        az_perms.set_mode(0o755);
        fs::set_permissions(&az_path, az_perms).expect("chmod fake az");

        let capture_path = temp.path().join("azcopy-args.txt");
        let azcopy_path = bin_dir.join("azcopy");
        fs::write(
            &azcopy_path,
            r#"#!/bin/sh
set -e
capture_file="${TEST_AZCOPY_CAPTURE_FILE:?}"
printf '%s\n' "$@" > "$capture_file"
"#,
        )
        .expect("write fake azcopy");
        let mut perms = fs::metadata(&azcopy_path)
            .expect("azcopy metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&azcopy_path, perms).expect("chmod fake azcopy");

        let original_path = env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", bin_dir.display(), original_path);
        let capture_value = capture_path.to_string_lossy().to_string();
        let _guards = vec![
            EnvVarGuard::set("PATH", &path_value),
            EnvVarGuard::set("TEST_AZCOPY_CAPTURE_FILE", &capture_value),
        ];

        let config = AzureAciConfig {
            resource_group: "test-rg".to_string(),
            image: "test.azurecr.io/image:tag".to_string(),
            location: None,
            registry_server: None,
            registry_username: None,
            registry_password: None,
            cpu: "1.0".to_string(),
            memory_gb: "2.0".to_string(),
            storage_account: "teststorage".to_string(),
            storage_key: "testkey123".to_string(),
            file_share: "main-share".to_string(),
            host_share_root: PathBuf::from("/mnt/share"),
            container_share_root: PathBuf::from("/mnt/dowhiz-share"),
        };

        download_workspace_from_share(&config, "task-test-123", &workspace_dir).expect("download");

        let args = fs::read_to_string(&capture_path).expect("read captured args");
        assert!(args.contains("copy"));
        assert!(args.contains("--recursive"));
        assert!(args.contains("teststorage.file.core.windows.net"));
        assert!(args.contains("task-test-123"));
        assert!(args.contains(&workspace_dir.to_string_lossy().to_string()));
    }

    #[test]
    #[cfg(unix)]
    fn test_delete_ephemeral_share_calls_az_storage_share_delete() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin_dir = temp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let capture_path = temp.path().join("az-args.txt");
        let az_path = bin_dir.join("az");
        fs::write(
            &az_path,
            r#"#!/bin/sh
set -e
capture_file="${TEST_AZ_CAPTURE_FILE:?}"
printf '%s\n' "$@" > "$capture_file"
"#,
        )
        .expect("write fake az");
        let mut perms = fs::metadata(&az_path).expect("az metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&az_path, perms).expect("chmod fake az");

        let original_path = env::var("PATH").unwrap_or_default();
        let path_value = format!("{}:{}", bin_dir.display(), original_path);
        let capture_value = capture_path.to_string_lossy().to_string();
        let _guards = vec![
            EnvVarGuard::set("PATH", &path_value),
            EnvVarGuard::set("TEST_AZ_CAPTURE_FILE", &capture_value),
        ];

        let config = AzureAciConfig {
            resource_group: "test-rg".to_string(),
            image: "test.azurecr.io/image:tag".to_string(),
            location: None,
            registry_server: None,
            registry_username: None,
            registry_password: None,
            cpu: "1.0".to_string(),
            memory_gb: "2.0".to_string(),
            storage_account: "teststorage".to_string(),
            storage_key: "testkey123".to_string(),
            file_share: "main-share".to_string(),
            host_share_root: PathBuf::from("/mnt/share"),
            container_share_root: PathBuf::from("/mnt/dowhiz-share"),
        };

        delete_ephemeral_share(&config, "task-test-123").expect("delete share");

        let args = fs::read_to_string(&capture_path).expect("read captured args");
        assert!(args.contains("storage"));
        assert!(args.contains("share"));
        assert!(args.contains("delete"));
        assert!(args.contains("--name"));
        assert!(args.contains("task-test-123"));
        assert!(args.contains("--delete-snapshots"));
        assert!(args.contains("include"));
    }

    #[test]
    #[cfg(unix)]
    fn test_build_aci_create_command_uses_provided_file_share() {
        let config = AzureAciConfig {
            resource_group: "test-rg".to_string(),
            image: "test.azurecr.io/image:tag".to_string(),
            location: None,
            registry_server: None,
            registry_username: None,
            registry_password: None,
            cpu: "1.0".to_string(),
            memory_gb: "2.0".to_string(),
            storage_account: "teststorage".to_string(),
            storage_key: "testkey123".to_string(),
            file_share: "main-share".to_string(),
            host_share_root: PathBuf::from("/mnt/share"),
            container_share_root: PathBuf::from("/mnt/dowhiz-share"),
        };

        let cmd = build_aci_create_command(
            &config,
            "test-container",
            "echo hello",
            "task-ephemeral-share",
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"--azure-file-volume-share-name".to_string()));
        let share_idx = args
            .iter()
            .position(|a| a == "--azure-file-volume-share-name")
            .unwrap();
        assert_eq!(args[share_idx + 1], "task-ephemeral-share");
    }

    #[test]
    fn test_task_completion_deserialize_with_container_name() {
        let json = r#"{"task_id":"abc123","container_name":"dwz-warm-xyz789","exit_code":0}"#;
        let completion: TaskCompletion = serde_json::from_str(json).unwrap();
        assert_eq!(completion.task_id, "abc123");
        assert_eq!(
            completion.container_name,
            Some("dwz-warm-xyz789".to_string())
        );
        assert_eq!(completion.exit_code, 0);
    }

    #[test]
    fn test_task_completion_deserialize_without_container_name() {
        // Backwards compatibility: old completion messages without container_name
        let json = r#"{"task_id":"abc123","exit_code":1}"#;
        let completion: TaskCompletion = serde_json::from_str(json).unwrap();
        assert_eq!(completion.task_id, "abc123");
        assert_eq!(completion.container_name, None);
        assert_eq!(completion.exit_code, 1);
    }

    #[test]
    fn test_task_completion_deserialize_with_null_container_name() {
        let json = r#"{"task_id":"abc123","container_name":null,"exit_code":0}"#;
        let completion: TaskCompletion = serde_json::from_str(json).unwrap();
        assert_eq!(completion.task_id, "abc123");
        assert_eq!(completion.container_name, None);
        assert_eq!(completion.exit_code, 0);
    }

    #[test]
    fn test_task_completion_matches_warm_worker_format() {
        // This test verifies that TaskCompletion can parse the JSON format
        // produced by warm_worker.sh:
        //   jq -n --arg tid "$TASK_ID" --arg cname "$CONTAINER_NAME" --argjson code "$EXIT_CODE" \
        //       '{task_id: $tid, container_name: $cname, exit_code: $code}'

        // Simulate what jq produces with typical values
        let json = serde_json::json!({
            "task_id": "72fdf768-1234-5678-abcd-ef0123456789",
            "container_name": "dwz-warm-048872263421470fa1ca623fee83d5a6",
            "exit_code": 0
        });

        let completion: TaskCompletion = serde_json::from_value(json).unwrap();
        assert_eq!(completion.task_id, "72fdf768-1234-5678-abcd-ef0123456789");
        assert_eq!(
            completion.container_name,
            Some("dwz-warm-048872263421470fa1ca623fee83d5a6".to_string())
        );
        assert_eq!(completion.exit_code, 0);
    }

    #[test]
    fn test_task_completion_with_nonzero_exit_code() {
        let json = r#"{"task_id":"failed-task","container_name":"dwz-warm-xyz","exit_code":1}"#;
        let completion: TaskCompletion = serde_json::from_str(json).unwrap();
        assert_eq!(completion.exit_code, 1);
        assert_eq!(completion.container_name, Some("dwz-warm-xyz".to_string()));
    }
}
