mod support;

use run_task_module::{
    run_claude_fallback_after_codex_failure, run_task, RunTaskError, RunTaskParams,
};
use std::env;
use std::fs;
use std::path::Path;
use support::{
    build_params, create_workspace, write_fake_claude, write_fake_codex, write_fake_gh, EnvGuard,
    EnvUnsetGuard, FakeClaudeMode, FakeCodexMode, TempDir, ENV_MUTEX,
};

fn env_enabled(key: &str) -> bool {
    matches!(env::var(key).as_deref(), Ok("1"))
}

fn require_env(key: &'static str) {
    let value = env::var(key).unwrap_or_default();
    if value.trim().is_empty() {
        panic!("{key} must be set to run the real Codex E2E test");
    }
}

#[test]
#[cfg(unix)]
fn run_task_success_with_fake_codex() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_success").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Success).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("CODEX_MODEL", "override-model"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).unwrap();
    assert!(result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());

    let config_path = home_dir.join(".codex").join("config.toml");
    let config = fs::read_to_string(config_path).unwrap();
    assert!(config.contains("model = \"gpt-5.4\""));
    assert!(config.contains("https://example.azure.com/openai/v1"));
    assert!(!config.contains("model = \"override-model\""));
    assert!(!config.contains("https://knowhiz-service-openai-backup-2.openai.azure.com/openai/v1"));
}

#[test]
#[cfg(unix)]
fn run_task_skips_yolo_without_bypass() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_no_yolo").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::EnsureNoYolo).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("CODEX_BYPASS_SANDBOX", "0"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).unwrap();
    assert!(result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());
}

#[test]
#[cfg(unix)]
fn run_task_uses_yolo_with_bypass() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_yolo").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::EnsureYolo).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("CODEX_BYPASS_SANDBOX", "1"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).unwrap();
    assert!(result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());
}

#[test]
#[cfg(unix)]
fn run_task_uses_danger_sandbox_with_bypass() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_danger_sandbox").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::EnsureDangerSandbox).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("CODEX_BYPASS_SANDBOX", "1"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).unwrap();
    assert!(result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());
}

#[test]
#[cfg(unix)]
fn run_task_sets_human_approval_gate_mcp_env() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_hag_mcp_env").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::EnsureHumanApprovalGateMcpEnv).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).unwrap();
    assert!(result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());
}

#[test]
#[cfg(unix)]
fn run_task_passes_add_dir_for_gh_config() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_add_dir").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::EnsureAddDir).unwrap();

    let gh_config_dir = home_dir.join(".config").join("gh");
    let expected_add_dir = gh_config_dir.to_str().unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
        ("EXPECTED_ADD_DIR", expected_add_dir),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).unwrap();
    assert!(result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());
}

#[test]
#[cfg(unix)]
fn run_task_reports_missing_output() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_missing_output").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::NoOutput).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    match err {
        RunTaskError::FallbackFailed { primary, fallback } => {
            assert!(primary.contains("Expected output not found"));
            assert!(primary.contains("reply_email_draft.html"));
            assert!(fallback.contains("simulated claude failure"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn run_task_reports_empty_reply_as_missing_output() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_empty_reply").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::EmptyReply).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    match err {
        RunTaskError::FallbackFailed { primary, fallback } => {
            assert!(primary.contains("Expected output not found"));
            assert!(primary.contains("reply_email_draft.html"));
            assert!(fallback.contains("simulated claude failure"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn run_task_reports_codex_failure() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_failure").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Fail).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    match err {
        RunTaskError::FallbackFailed { primary, fallback } => {
            assert!(primary.contains("Codex failed"));
            assert!(primary.contains("status: Some(2)"));
            assert!(primary.contains("simulated failure"));
            assert!(fallback.contains("Claude failed"));
            assert!(fallback.contains("simulated claude failure"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn run_task_falls_back_to_claude_after_codex_failure_by_default() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_fallback_to_claude").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Fail).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::EnsureModel).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("EXPECTED_CLAUDE_MODEL", "claude-sonnet-4-5"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).expect("run_task should fall back to Claude");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Claude fallback reply"));
    assert_eq!(
        result.recovery_note.as_deref(),
        Some(
            "Recovered via Claude fallback after primary Codex failure (Codex failed) using Claude model claude-sonnet-4-5"
        )
    );
    assert!(workspace
        .join(".run_task_trace_codex_primary")
        .join("metadata.json")
        .exists());
    let fallback_note = fs::read_to_string(
        workspace
            .join(".run_task_trace")
            .join("recovery")
            .join("codex_to_claude_fallback.txt"),
    )
    .unwrap();
    assert!(fallback_note.contains("status=success"));
    assert!(fallback_note.contains("fallback_model=claude-sonnet-4-5"));
}

#[test]
#[cfg(unix)]
fn run_task_reports_both_errors_when_default_claude_fallback_fails() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_fallback_failure").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Fail).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    match err {
        RunTaskError::FallbackFailed { primary, fallback } => {
            assert!(primary.contains("Codex failed"));
            assert!(primary.contains("simulated failure"));
            assert!(fallback.contains("Claude failed"));
            assert!(fallback.contains("simulated claude failure"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn warm_pool_codex_failure_falls_back_to_claude_by_default() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("warm_pool_codex_fallback_to_claude").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::EnsureModel).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("EXPECTED_CLAUDE_MODEL", "claude-sonnet-4-5"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let trace_dir = workspace.join(".run_task_trace");
    fs::create_dir_all(&trace_dir).unwrap();
    fs::write(trace_dir.join("metadata.json"), "{}").unwrap();
    let reply_html_path = workspace.join("reply_email_draft.html");
    fs::write(&reply_html_path, "stale reply").unwrap();
    let attachments_dir = workspace.join("reply_email_attachments");
    fs::create_dir_all(&attachments_dir).unwrap();
    fs::write(attachments_dir.join("stale.txt"), "stale attachment").unwrap();

    let params = build_params(&workspace);
    let result = run_claude_fallback_after_codex_failure(
        &params,
        RunTaskError::CodexFailed {
            status: Some(1),
            output: "warm pool simulated failure".to_string(),
        },
    )
    .expect("warm-pool fallback should recover with Claude");

    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Claude fallback reply"));
    assert!(!attachments_dir.join("stale.txt").exists());
    assert!(workspace
        .join(".run_task_trace_codex_primary")
        .join("metadata.json")
        .exists());
    let fallback_note = fs::read_to_string(
        workspace
            .join(".run_task_trace")
            .join("recovery")
            .join("codex_to_claude_fallback.txt"),
    )
    .unwrap();
    assert!(fallback_note.contains("status=success"));
    assert!(fallback_note.contains("warm pool simulated failure"));
    assert!(fallback_note.contains("fallback_model=claude-sonnet-4-5"));
}

#[test]
#[cfg(unix)]
fn azure_aci_timeout_error_falls_back_to_claude() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("azure_aci_timeout_fallback_to_claude").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::EnsureModel).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("EXPECTED_CLAUDE_MODEL", "claude-sonnet-4-5"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_claude_fallback_after_codex_failure(
        &params,
        RunTaskError::CommandTimeout {
            command: "az container show",
            timeout_secs: 900,
            output: "container did not reach terminal state before timeout".to_string(),
        },
    )
    .expect("azure aci timeout should fall back to Claude");

    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Claude fallback reply"));
    assert_eq!(
        result.recovery_note.as_deref(),
        Some(
            "Recovered via Claude fallback after primary Codex failure (Azure ACI Codex timed out) using Claude model claude-sonnet-4-5"
        )
    );
}

#[test]
#[cfg(unix)]
fn run_task_recovers_ready_reply_after_claude_fallback_timeout() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_claude_timeout_recovery").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Fail).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::ReplyThenSleep).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("RUN_TASK_TIMEOUT_SECS", "1"),
        ("SLEEP_SECS", "2"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).expect("run_task should recover reply after Claude timeout");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Claude timeout recovery reply"));
    let recovery_note = result.recovery_note.as_deref().unwrap_or("");
    assert!(
        recovery_note.contains("Recovered ready reply artifact after Claude timed out after 1s")
    );
    assert!(recovery_note.contains(
        "Recovered via Claude fallback after primary Codex failure (Codex failed) using Claude model claude-sonnet-4-5"
    ));
}

#[test]
#[cfg(unix)]
fn run_task_recovers_ready_reply_after_late_codex_failure() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_reply_then_fail").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::ReplyThenFail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).expect("run_task should recover late failure");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Recovered reply"));
    assert_eq!(
        result.recovery_note.as_deref(),
        Some("Recovered ready reply artifact after Codex stream disconnect during finalization")
    );
}

#[test]
#[cfg(unix)]
fn run_task_reports_turn_aborted_as_codex_failure() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_turn_aborted").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::TurnAborted).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    match err {
        RunTaskError::FallbackFailed { primary, fallback } => {
            assert!(primary.contains("Codex failed"));
            assert!(primary.contains("status: None"));
            assert!(primary.contains("turn aborted"));
            assert!(fallback.contains("Claude failed"));
            assert!(fallback.contains("simulated claude failure"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn run_task_times_out_with_codex() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_timeout").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Sleep).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
        ("RUN_TASK_TIMEOUT_SECS", "1"),
        ("SLEEP_SECS", "2"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    match err {
        RunTaskError::FallbackFailed { primary, fallback } => {
            assert!(primary.contains("Command timed out (codex after 1s)"));
            assert!(fallback.contains("Claude failed"));
            assert!(fallback.contains("simulated claude failure"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn run_task_times_out_with_claude() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("claude_task_timeout").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Sleep).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("RUN_TASK_TIMEOUT_SECS", "1"),
        ("SLEEP_SECS", "2"),
    ]);

    let mut params = build_params(&workspace);
    params.runner = "claude".to_string();
    let err = run_task(&params).unwrap_err();
    assert!(matches!(
        err,
        RunTaskError::CommandTimeout {
            command: "claude",
            timeout_secs: 1,
            ..
        }
    ));
}

#[test]
#[cfg(unix)]
fn run_task_codex_fallback_uses_configured_claude_timeout() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_fallback_claude_timeout_override").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Fail).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Sleep).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
        ("RUN_TASK_TIMEOUT_SECS", "10"),
        ("RUN_TASK_CODEX_FALLBACK_TIMEOUT_SECS", "1"),
        ("SLEEP_SECS", "2"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    match err {
        RunTaskError::FallbackFailed { primary, fallback } => {
            assert!(primary.contains("simulated failure"));
            assert!(fallback.contains("Command timed out (claude after 1s)"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn run_task_reports_missing_codex_cli() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_missing_cli").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", bin_dir.to_str().unwrap()),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    match err {
        RunTaskError::FallbackFailed { primary, fallback } => {
            assert!(primary.contains("Codex CLI not found on PATH."));
            assert!(fallback.contains("Claude failed"));
            assert!(fallback.contains("simulated claude failure"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn run_task_maps_github_env_from_dotenv() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_github_env").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let env_path = temp.path.join(".env");
    fs::write(
        &env_path,
        r#"GITHUB_USERNAME="octo-user"
GITHUB_PERSONAL_ACCESS_TOKEN="pat-test-token"
"#,
    )
    .unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::GithubEnvCheck).unwrap();
    write_fake_gh(&bin_dir).unwrap();

    let _unset = EnvUnsetGuard::remove(&[
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GITHUB_PERSONAL_ACCESS_TOKEN",
        "GITHUB_USERNAME",
    ]);

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params);
    assert!(result.is_ok(), "expected GH env to reach codex");
}

#[test]
#[cfg(unix)]
fn run_task_maps_employee_github_env_from_dotenv() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_employee_github_env").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let env_path = temp.path.join(".env");
    fs::write(
        &env_path,
        r#"MAGGIE_GITHUB_USERNAME="octo-user"
MAGGIE_GITHUB_PERSONAL_ACCESS_TOKEN="pat-test-token"
"#,
    )
    .unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::GithubEnvCheck).unwrap();
    write_fake_gh(&bin_dir).unwrap();

    let _unset = EnvUnsetGuard::remove(&[
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GITHUB_PERSONAL_ACCESS_TOKEN",
        "GITHUB_USERNAME",
    ]);

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
        ("EMPLOYEE_ID", "mini_mouse"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params);
    assert!(result.is_ok(), "expected employee GH env to reach codex");
}

#[test]
#[cfg(unix)]
fn run_task_maps_x402_env_from_dotenv() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_x402_env").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let env_path = temp.path.join(".env");
    fs::write(
        &env_path,
        r#"GOATX402_API_URL="https://x402-api.example.test"
GOATX402_MERCHANT_ID="dowhiz_agent"
GOATX402_API_KEY="key_direct"
GOATX402_API_SECRET="secret_direct"
"#,
    )
    .unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::X402EnvCheck).unwrap();

    let _unset = EnvUnsetGuard::remove(&[
        "GOATX402_API_URL",
        "GOATX402_MERCHANT_ID",
        "GOATX402_API_KEY",
        "GOATX402_API_SECRET",
        "OLIVER_GOATX402_API_URL",
        "OLIVER_GOATX402_MERCHANT_ID",
        "OLIVER_GOATX402_API_KEY",
        "OLIVER_GOATX402_API_SECRET",
    ]);

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
        ("EXPECTED_GOATX402_API_URL", "https://x402-api.example.test"),
        ("EXPECTED_GOATX402_MERCHANT_ID", "dowhiz_agent"),
        ("EXPECTED_GOATX402_API_KEY", "key_direct"),
        ("EXPECTED_GOATX402_API_SECRET", "secret_direct"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params);
    assert!(result.is_ok(), "expected GOATX402_* env to reach codex");
}

#[test]
#[cfg(unix)]
fn run_task_maps_employee_prefixed_x402_env_from_dotenv() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_x402_prefixed_env").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let env_path = temp.path.join(".env");
    fs::write(
        &env_path,
        r#"OLIVER_GOATX402_API_URL="https://x402-prefixed.example.test"
OLIVER_GOATX402_MERCHANT_ID="dowhiz_agent_prefixed"
OLIVER_GOATX402_API_KEY="key_prefixed"
OLIVER_GOATX402_API_SECRET="secret_prefixed"
"#,
    )
    .unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::X402EnvCheck).unwrap();

    let _unset = EnvUnsetGuard::remove(&[
        "GOATX402_API_URL",
        "GOATX402_MERCHANT_ID",
        "GOATX402_API_KEY",
        "GOATX402_API_SECRET",
        "OLIVER_GOATX402_API_URL",
        "OLIVER_GOATX402_MERCHANT_ID",
        "OLIVER_GOATX402_API_KEY",
        "OLIVER_GOATX402_API_SECRET",
    ]);

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
        ("EMPLOYEE_ID", "little_bear"),
        (
            "EXPECTED_GOATX402_API_URL",
            "https://x402-prefixed.example.test",
        ),
        ("EXPECTED_GOATX402_MERCHANT_ID", "dowhiz_agent_prefixed"),
        ("EXPECTED_GOATX402_API_KEY", "key_prefixed"),
        ("EXPECTED_GOATX402_API_SECRET", "secret_prefixed"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params);
    assert!(
        result.is_ok(),
        "expected prefixed OLIVER_GOATX402_* env to reach codex"
    );
}

#[test]
#[cfg(unix)]
fn run_task_reports_missing_env() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_missing_env").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Success).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", ""),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).unwrap_err();
    assert!(matches!(
        err,
        RunTaskError::MissingEnv {
            key: "AZURE_OPENAI_API_KEY_BACKUP"
        }
    ));
}

#[test]
#[cfg(unix)]
fn run_task_rejects_absolute_input_dir() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_absolute").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let request = RunTaskParams {
        workspace_dir: workspace,
        input_email_dir: Path::new("/absolute/path").to_path_buf(),
        input_attachments_dir: Path::new("incoming_attachments").to_path_buf(),
        memory_dir: Path::new("memory").to_path_buf(),
        reference_dir: Path::new("references").to_path_buf(),
        reply_to: vec!["user@example.com".to_string()],
        model_name: "test-model".to_string(),
        runner: "codex".to_string(),
        codex_disabled: false,
        channel: "email".to_string(),
        google_access_token: std::env::var("GOOGLE_ACCESS_TOKEN").ok(),
        notion_access_token: std::env::var("NOTION_API_TOKEN").ok(),
        has_unified_account: true,
        user_identities: Default::default(),
        thread_epoch: None,
        thread_state_path: None,
    };

    let err = run_task(&request).unwrap_err();
    assert!(matches!(err, RunTaskError::InvalidPath { .. }));
}

#[test]
fn run_task_codex_disabled_writes_placeholder() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_disabled").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let mut params = build_params(&workspace);
    params.codex_disabled = true;

    let result = run_task(&params).unwrap();
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Codex disabled"));
    assert!(result.reply_attachments_dir.is_dir());
}

#[test]
fn run_task_codex_disabled_skips_placeholder_without_reply_to() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_disabled_no_reply").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let mut params = build_params(&workspace);
    params.codex_disabled = true;
    params.reply_to.clear();

    let result = run_task(&params).unwrap();
    assert!(!result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());
}

#[test]
#[cfg(unix)]
fn run_task_real_codex_e2e_when_enabled() {
    let _lock = ENV_MUTEX.lock().unwrap();
    if !env_enabled("RUN_CODEX_E2E") {
        eprintln!("RUN_CODEX_E2E not set; skipping real Codex E2E test.");
        return;
    }

    require_env("AZURE_OPENAI_API_KEY_BACKUP");
    require_env("AZURE_OPENAI_ENDPOINT_BACKUP");

    let temp = TempDir::new("codex_task_real_e2e").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    fs::create_dir_all(&home_dir).unwrap();
    let _env = EnvGuard::set(&[("HOME", home_dir.to_str().unwrap())]);

    let params = build_params(&workspace);

    let result = run_task(&params).unwrap_or_else(|err| {
        panic!("Real Codex E2E test failed: {err}");
    });
    assert!(result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());

    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(!html.trim().is_empty());
}
