mod support;

use run_task_module::{
    run_claude_fallback_after_codex_failure, run_task, RunTaskError, RunTaskParams,
};
use send_emails_module::normalize_email_html;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use support::{
    build_params, create_workspace, install_runtime_skills_and_employee_guidance,
    write_fake_claude, write_fake_codex, write_fake_gh, EnvGuard, EnvUnsetGuard, FakeClaudeMode,
    FakeCodexMode, TempDir, ENV_MUTEX,
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

fn assert_non_empty_renderable_html(text: &str) {
    assert!(!text.trim().is_empty(), "reply should not be empty");
    let normalized = normalize_email_html("Test subject", text);
    assert!(
        !normalized.trim().is_empty(),
        "normalized reply should still contain renderable HTML"
    );
    assert!(
        normalized.contains('<') && normalized.contains('>'),
        "normalized reply should still look like HTML"
    );
}

fn write_investment_request(workspace: &Path, subject: &str, prompt: &str) {
    let payload = format!(
        r#"{{
  "Subject": "{subject}",
  "TextBody": "{prompt}",
  "HtmlBody": "<p>{prompt}</p>"
}}"#
    );
    fs::write(
        workspace
            .join("incoming_email")
            .join("postmark_payload.json"),
        payload,
    )
    .unwrap();
    fs::write(
        workspace.join("incoming_email").join("email.html"),
        format!("<p>{prompt}</p>"),
    )
    .unwrap();
}

#[cfg(unix)]
fn write_shell_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
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
fn run_task_uses_danger_bypass_flag_when_supported() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_danger_flag").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
set -e
if [ "$1" = "exec" ] && [ "$2" = "--help" ]; then
  printf '%s\n' '--search' '--ask-for-approval' '--sandbox' '--dangerously-bypass-approvals-and-sandbox' '--cd'
  exit 0
fi
found="0"
for arg in "$@"; do
  if [ "$arg" = "--dangerously-bypass-approvals-and-sandbox" ]; then
    found="1"
  fi
done
if [ "$found" != "1" ]; then
  echo "missing --dangerously-bypass-approvals-and-sandbox" >&2
  exit 3
fi
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#,
    );

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
fn run_task_falls_back_to_legacy_yolo_when_only_yolo_is_supported() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_legacy_yolo").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
set -e
if [ "$1" = "exec" ] && [ "$2" = "--help" ]; then
  printf '%s\n' '--search' '--ask-for-approval' '--sandbox' '--yolo' '--cd'
  exit 0
fi
found="0"
for arg in "$@"; do
  if [ "$arg" = "--yolo" ]; then
    found="1"
  fi
done
if [ "$found" != "1" ]; then
  echo "missing --yolo" >&2
  exit 3
fi
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#,
    );

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
fn run_task_sets_workspace_gh_config_dir_without_add_dir() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_gh_config_dir").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
set -e
if [ "$1" = "exec" ] && [ "$2" = "--help" ]; then
  printf '%s\n' '--search' '--ask-for-approval' '--sandbox' '--dangerously-bypass-approvals-and-sandbox' '--cd'
  exit 0
fi
if [ -z "${GH_CONFIG_DIR:-}" ]; then
  echo "missing GH_CONFIG_DIR" >&2
  exit 3
fi
if [ ! -d "$GH_CONFIG_DIR" ]; then
  echo "GH_CONFIG_DIR does not exist: $GH_CONFIG_DIR" >&2
  exit 3
fi
if [ -n "${EXPECTED_GH_CONFIG_DIR:-}" ] && [ "$GH_CONFIG_DIR" != "$EXPECTED_GH_CONFIG_DIR" ]; then
  echo "unexpected GH_CONFIG_DIR: expected '$EXPECTED_GH_CONFIG_DIR' got '$GH_CONFIG_DIR'" >&2
  exit 3
fi
for arg in "$@"; do
  if [ "$arg" = "--add-dir" ]; then
    echo "unexpected --add-dir" >&2
    exit 3
  fi
done
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#,
    );

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
        (
            "EXPECTED_GH_CONFIG_DIR",
            workspace.join(".config").join("gh").to_str().unwrap(),
        ),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).unwrap();
    assert!(result.reply_html_path.exists());
    assert!(result.reply_attachments_dir.is_dir());
    assert!(workspace.join(".config").join("gh").is_dir());
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
fn run_task_claude_fallback_uses_explicit_settings_and_clears_ambient_auth() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_claude_foundry_settings").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Fail).unwrap();
    write_shell_script(
        &bin_dir,
        "claude",
        r#"#!/bin/sh
set -e
found_settings="0"
prev=""
for arg in "$@"; do
  if [ "$prev" = "--settings" ]; then
    found_settings="1"
  fi
  prev="$arg"
done
if [ "$found_settings" != "1" ]; then
  echo "missing --settings" >&2
  exit 3
fi
if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
  echo "ambient ANTHROPIC_API_KEY leaked into claude child" >&2
  exit 3
fi
echo '{"type":"message_delta","delta":{"text":"ok"}}'
echo "<html><body>Claude fallback reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#,
    );

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("ANTHROPIC_API_KEY", "ambient-bad-key"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).expect("run_task should recover with explicit Claude settings");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Claude fallback reply"));
}

#[test]
#[cfg(unix)]
fn run_task_reports_explicit_claude_auth_failure_when_fallback_login_is_invalid() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_claude_auth_failure").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Fail).unwrap();
    write_shell_script(
        &bin_dir,
        "claude",
        r#"#!/bin/sh
echo "Invalid API key · Please run /login" >&2
exit 7
"#,
    );

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("ANTHROPIC_API_KEY", "ambient-bad-key"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let err = run_task(&params).expect_err("fallback should fail with explicit auth message");
    match err {
        RunTaskError::FallbackFailed { fallback, .. } => {
            assert!(
                fallback.contains("Claude authentication failed while attempting DoWhiz fallback")
            );
            assert!(fallback.contains("Please run /login"));
        }
        other => panic!("expected FallbackFailed, got {:?}", other),
    }
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
fn run_task_recovers_ready_reply_after_claude_exit_zero_without_assistant_text() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_claude_empty_output_recovery").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::Fail).unwrap();
    write_fake_claude(&bin_dir, FakeClaudeMode::ReplyWithoutAssistantText).unwrap();

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
    let result =
        run_task(&params).expect("run_task should recover reply after empty Claude output");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Claude artifact recovery reply"));
    let recovery_note = result.recovery_note.as_deref().unwrap_or("");
    assert!(recovery_note.contains(
        "Recovered ready reply artifact after Claude exited successfully without assistant text"
    ));
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
fn run_task_treats_completed_turn_with_valid_reply_and_nonzero_exit_as_success() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_completed_turn_nonzero").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::ReplyThenTurnCompleteExitNonzero).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result = run_task(&build_params(&workspace)).expect("completed turn should be accepted");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Recovered reply"));
    let note = result.recovery_note.as_deref().unwrap_or("");
    assert!(note.contains("Codex completed the turn and wrote a valid reply artifact"));
    assert!(!note.contains("Claude fallback"));
}

#[test]
#[cfg(unix)]
fn run_task_returns_early_when_valid_reply_artifact_exists() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_timeout_valid_reply").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "NVDA monitor check",
        "Check whether anything material changed for NVDA and summarize the update.",
    );
    install_runtime_skills_and_employee_guidance(&workspace, "little_bear").unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
set -e
if [ "$1" = "exec" ] && [ "$2" = "--help" ]; then
  printf '%s\n' '--search' '--ask-for-approval' '--sandbox' '--dangerously-bypass-approvals-and-sandbox' '--cd'
  exit 0
fi
exec python3 - <<'PY'
from pathlib import Path
import time

Path("reply_email_draft.html").write_text("""<section>
  <p><strong>As of:</strong> 2026-05-01 · <strong>Price:</strong> $114.50</p>
  <p><strong>Investor question:</strong> Check whether anything material changed for NVDA and summarize the update.</p>
</section>
<section>
  <h2>Decision Card</h2>
  <table>
    <tr><th>Field</th><th>Value</th></tr>
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
  <p>Recent checks did not move the thesis enough to justify new action.</p>
</section>
<section>
  <h2>What Would Change The View</h2>
  <ul>
    <li><strong>Upgrade / Review Now:</strong> Next report shows data-center revenue growth re-accelerating above 25% while gross margin stays above 74%.</li>
    <li><strong>Downgrade / De-risk:</strong> Management cuts the next-quarter revenue guide by 5% or more.</li>
    <li><strong>Invalidation:</strong> A new export-control change threatens more than 10% of expected revenue.</li>
  </ul>
</section>
<section>
  <h2>Evidence Chips</h2>
  <ul>
    <li><a href="https://investor.nvidia.com/">NVIDIA IR</a></li>
    <li><a href="https://www.sec.gov/">SEC EDGAR</a></li>
    <li><a href="https://www.reuters.com/world/china/prices-nvidias-b300-server-1-million-china-us-curbs-sources-say-2026-04-30/">Reuters</a></li>
  </ul>
</section>
""")
attachments = Path("reply_email_attachments")
attachments.mkdir(exist_ok=True)
(attachments / "attachment.txt").write_text("attachment")
time.sleep(10)
PY
"#,
    );

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("RUN_TASK_TIMEOUT_SECS", "20"),
        ("RUN_TASK_CODEX_TIMEOUT_SECS", "4"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let started_at = Instant::now();
    let result =
        run_task(&build_params(&workspace)).expect("ready artifact should be accepted early");
    let elapsed = started_at.elapsed();
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("No Material Change"));
    assert!(
        elapsed.as_secs_f32() < 6.0,
        "artifact-first runner should have returned quickly, elapsed={elapsed:?}"
    );
    let note = result.recovery_note.unwrap_or_default();
    assert!(note.contains("Returned early once a valid reply artifact existed"));
    assert!(!note.contains("Claude fallback"));
}

#[test]
#[cfg(unix)]
fn run_task_action_only_monitor_requests_still_run_real_analysis() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_action_only_monitor_analysis").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "NVDA monitor check",
        "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.",
    );
    fs::write(
        workspace.join("incoming_email").join("thread_request.md"),
        "# Canonical thread request\nAuto-generated merged view for reruns.\n\n## Latest inbound message\nPreview:\n```text\nCheck whether anything material changed for NVDA since your last note. Only tell me if I should act.\n```\n",
    )
    .unwrap();
    install_runtime_skills_and_employee_guidance(&workspace, "little_bear").unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    let counter_path = temp.path.join("codex_invocations.txt");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        &format!(
            r#"#!/bin/sh
set -e
if [ "$1" = "exec" ] && [ "$2" = "--help" ]; then
  printf '%s\n' '--search' '--ask-for-approval' '--sandbox' '--dangerously-bypass-approvals-and-sandbox' '--cd'
  exit 0
fi
count=0
if [ -f "{counter_path}" ]; then
  count="$(cat "{counter_path}")"
fi
count=$((count + 1))
printf '%s' "$count" > "{counter_path}"
cat > reply_email_draft.html <<'HTML'
<html><body><h1>NVDA act-now check</h1><p><strong>Action:</strong> No immediate action.</p><p><strong>What changed:</strong> No fresh filing or earnings delta was established in the available workspace evidence.</p><p><strong>What would change the view:</strong> A material guidance reset, margin break, or fresh demand signal.</p></body></html>
HTML
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#,
            counter_path = counter_path.display()
        ),
    );
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("RUN_TASK_TIMEOUT_SECS", "20"),
        ("RUN_TASK_CODEX_TIMEOUT_SECS", "8"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result = run_task(&build_params(&workspace))
        .expect("action-only monitor request should still run real analysis");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("NVDA act-now check"));
    assert!(html.contains("No immediate action"));
    assert!(html.contains("What changed"));
    assert_eq!(fs::read_to_string(&counter_path).unwrap(), "1");
    assert!(!html.contains("Quick update on"));
    assert!(!html.contains("Unable to Verify"));
    assert!(!html.contains("No recommendation"));
    let note = result.recovery_note.unwrap_or_default();
    assert!(!note.contains("deterministic"));
    assert!(!note.contains("Claude fallback"));
}

#[test]
#[cfg(unix)]
fn run_task_monitor_timeout_uses_codex_retry_then_claude_fallback() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_monitor_claude_fallback").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "NVDA monitor check",
        "Check whether anything material changed for NVDA and summarize the update.",
    );
    install_runtime_skills_and_employee_guidance(&workspace, "little_bear").unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    let counter_path = temp.path.join("codex_invocations.txt");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        &format!(
            r#"#!/bin/sh
set -e
if [ "$1" = "exec" ] && [ "$2" = "--help" ]; then
  printf '%s\n' '--search' '--ask-for-approval' '--sandbox' '--dangerously-bypass-approvals-and-sandbox' '--cd'
  exit 0
fi
count=0
if [ -f "{counter_path}" ]; then
  count="$(cat "{counter_path}")"
fi
count=$((count + 1))
printf '%s' "$count" > "{counter_path}"
sleep 20
"#,
            counter_path = counter_path.display()
        ),
    );
    write_fake_claude(&bin_dir, FakeClaudeMode::EnsureModel).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("RUN_TASK_TIMEOUT_SECS", "20"),
        ("RUN_TASK_CODEX_TIMEOUT_SECS", "8"),
        ("RUN_TASK_REPLY_DRAFT_RESERVE_SECS", "3"),
        ("EXPECTED_CLAUDE_MODEL", "claude-sonnet-4-5"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result = run_task(&build_params(&workspace))
        .expect("monitor timeout should reach the real fallback chain");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Claude fallback reply"));
    assert_eq!(fs::read_to_string(&counter_path).unwrap(), "2");
    assert!(workspace.join("codex_fast_completion_context.md").exists());
    let note = result.recovery_note.as_deref().unwrap_or("");
    assert!(note.contains("Claude fallback"));
    assert!(!note.contains("deterministic operational"));
}

#[test]
#[cfg(unix)]
fn run_task_deep_research_timeout_returns_operational_failure_after_real_attempts() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_operational_failure_reply").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "Nokia investment memo",
        "Give me a deep research about the Nokia stock, and tell me whether it is a good time to buy.",
    );
    install_runtime_skills_and_employee_guidance(&workspace, "little_bear").unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    let counter_path = temp.path.join("codex_invocations.txt");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        &format!(
            r#"#!/bin/sh
set -e
for arg in "$@"; do
  if [ "$arg" = "--help" ]; then
    echo "codex exec [--dangerously-bypass-approvals-and-sandbox] [--cd]"
    exit 0
  fi
done
workspace="."
prev=""
for arg in "$@"; do
  if [ "$prev" = "--cd" ]; then
    workspace="$arg"
    break
  fi
  prev="$arg"
done
cd "$workspace"
count=0
if [ -f "{counter_path}" ]; then
  count="$(cat "{counter_path}")"
fi
count=$((count + 1))
printf '%s' "$count" > "{counter_path}"
echo "count=$count" >&2
sleep 20
"#,
            counter_path = counter_path.display()
        ),
    );
    write_fake_claude(&bin_dir, FakeClaudeMode::Fail).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("RUN_TASK_TIMEOUT_SECS", "20"),
        ("RUN_TASK_CODEX_TIMEOUT_SECS", "8"),
        ("RUN_TASK_REPLY_DRAFT_RESERVE_SECS", "3"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result = run_task(&build_params(&workspace))
        .expect("all analysis failures should still produce an operational reply");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Investment analysis could not be completed"));
    assert!(html.contains("No investment recommendation is included"));
    assert_eq!(fs::read_to_string(&counter_path).unwrap(), "1");
    assert!(!workspace.join("codex_fast_completion_context.md").exists());
    assert!(!html.contains("Quick update on"));
    assert!(!html.contains("Unable to Verify"));
    assert!(!html.contains("No recommendation"));
    let note = result.recovery_note.as_deref().unwrap_or("");
    assert!(note.contains("operational failure reply"));
    assert!(!note.contains("Claude fallback"));
    assert!(
        result
            .terminal_error_message
            .as_deref()
            .unwrap_or("")
            .contains("operational failure reply"),
        "operational failure reply should not be recorded as a successful task execution"
    );
}

#[test]
#[cfg(unix)]
fn run_task_recovers_reply_from_session_log_after_codex_failure() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_session_recovery").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    let sessions_dir = home_dir
        .join(".codex")
        .join("sessions")
        .join("2026")
        .join("05")
        .join("01");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&sessions_dir).unwrap();
    let recovered_reply = workspace.join("reply_email_draft.html");
    let session_path = sessions_dir.join("rollout-session-recovery.jsonl");
    let patch_payload = format!(
        "*** Begin Patch\n*** Add File: {}\n+<html><body>Recovered from session log</body></html>\n*** End Patch\n",
        recovered_reply.display()
    );
    let session_line = serde_json::json!({
        "type": "response_item",
        "payload": {
            "type": "custom_tool_call",
            "name": "apply_patch",
            "input": patch_payload,
        }
    });
    write_shell_script(
        &bin_dir,
        "codex",
        &format!(
            r#"#!/bin/sh
mkdir -p "{sessions_dir}"
cat > "{session_path}" <<'EOF'
{session_line}
EOF
echo "simulated failure" >&2
exit 23
"#,
            sessions_dir = sessions_dir.display(),
            session_path = session_path.display(),
            session_line = session_line
        ),
    );
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
    let result = run_task(&params).expect("run_task should recover reply from session log");
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(html.contains("Recovered from session log"));
    let recovery_note = result.recovery_note.as_deref().unwrap_or("");
    assert!(recovery_note.contains("Recovered reply artifact from Codex session log"));
    assert!(recovery_note.contains("status 23"));
}

#[test]
#[cfg(unix)]
fn run_task_nonzero_completed_turn_with_generic_investment_reply_is_accepted() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_invalid_nonzero_fallback").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "NVDA investment memo",
        "Give me deep research on NVDA and tell me whether now is a good time to buy.",
    );

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(
        &bin_dir,
        FakeCodexMode::InvestmentGenericTurnCompleteExitNonzero,
    )
    .unwrap();
    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result =
        run_task(&build_params(&workspace)).expect("generic nonzero artifact should still return");
    let html = fs::read_to_string(result.reply_html_path).unwrap();
    assert!(html.contains("wait until after earnings"));
    let note = result.recovery_note.unwrap_or_default();
    assert!(note.contains("late exit as a warning"));
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
fn run_task_investment_content_filter_retry_recovers_without_claude_fallback() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_investment_content_filter_retry").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "Novo Nordisk - NYSE NVO",
        "Please deep research Novo Nordisk stock (NYSE: NVO) and give me investment advice.",
    );

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(
        &bin_dir,
        FakeCodexMode::InvestmentContentFilterThenRetrySuccess,
    )
    .unwrap();
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
    let result = run_task(&params).unwrap();
    let reply = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(reply.contains("Decision Card"));
    assert!(reply.contains("Verified Facts"));
    assert!(!reply.contains("Unable to Verify"));
    assert!(!reply.contains("No recommendation"));
    let note = result.recovery_note.unwrap_or_default();
    assert!(note.contains("content-filter-safe Codex retry"));
    assert!(!note.contains("Claude fallback"));
}

#[test]
#[cfg(unix)]
fn run_task_investment_content_filter_failure_falls_back_to_claude_after_retry() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_investment_content_filter_fallback").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "Novo Nordisk - NYSE NVO",
        "Please deep research Novo Nordisk stock (NYSE: NVO) and give me investment advice.",
    );

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::InvestmentContentFilterAlwaysFail).unwrap();
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
    let result = run_task(&params).unwrap();
    let reply = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(reply.contains("Claude fallback reply"));
    let note = result.recovery_note.unwrap_or_default();
    assert!(note.contains("Claude fallback"));
    assert!(!note.contains("deterministic operational"));
}

#[test]
#[cfg(unix)]
fn run_task_generic_content_filter_returns_explanatory_reply_without_claude_fallback() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_generic_content_filter_notice").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    let claude_counter_path = temp.path.join("claude_invocations.txt");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
set -e
echo "I'm sorry, but I cannot assist with that request." >&2
echo "stream disconnected before completion: Incomplete response returned, reason: content_filter" >&2
exit 1
"#,
    );
    write_shell_script(
        &bin_dir,
        "claude",
        r#"#!/bin/sh
set -e
echo invoked >> "$CLAUDE_COUNTER_PATH"
echo "simulated claude failure" >&2
exit 7
"#,
    );

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("CLAUDE_COUNTER_PATH", claude_counter_path.to_str().unwrap()),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let params = build_params(&workspace);
    let result = run_task(&params).unwrap();
    let reply = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(reply.contains("Azure/OpenAI content filter"));
    assert!(reply.contains("send a new request"));
    assert!(
        !claude_counter_path.exists(),
        "Claude fallback should not run after a generic Azure/OpenAI content-filter refusal"
    );
    let note = result.recovery_note.unwrap_or_default();
    assert!(note.contains("content-filter explanation"));
    assert!(
        result
            .terminal_error_message
            .as_deref()
            .unwrap_or("")
            .contains("content-filter explanation"),
        "content-filter explanation replies should be delivered but recorded as failed executions"
    );
}

#[test]
#[cfg(unix)]
fn run_task_notion_auth_failure_does_not_run_claude_fallback() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_notion_auth_failure").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    let claude_counter_path = temp.path.join("claude_invocations.txt");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
set -e
echo "API Error: API request failed: Status 401 Unauthorized: {\"code\":\"unauthorized\",\"message\":\"API token is invalid.\"}" >&2
exit 1
"#,
    );
    write_shell_script(
        &bin_dir,
        "claude",
        r#"#!/bin/sh
set -e
echo invoked >> "$CLAUDE_COUNTER_PATH"
touch .notion_api_replied
"#,
    );

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("CLAUDE_COUNTER_PATH", claude_counter_path.to_str().unwrap()),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let mut params = build_params(&workspace);
    params.channel = "notion".to_string();
    let err = run_task(&params).unwrap_err();
    assert!(err.to_string().contains("API token is invalid"));
    assert!(
        !workspace.join(".notion_api_replied").exists(),
        "Notion auth failures must not be converted into a local marker without a posted Notion comment"
    );
    assert!(
        !claude_counter_path.exists(),
        "Claude fallback cannot repair invalid Notion auth and should not run"
    );
}

#[test]
#[cfg(unix)]
fn run_task_notion_content_filter_posts_explanation_via_notion_cli() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_notion_content_filter_notice").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    let posted_path = temp.path.join("posted_notion_comment.txt");
    let claude_counter_path = temp.path.join("claude_invocations.txt");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(
        workspace.join(".notion_context.json"),
        r#"{"page_id":"35d37bc1-a421-81cb-887c-c2510bbb37b9"}"#,
    )
    .unwrap();
    fs::write(
        workspace.join(".notion_env"),
        "NOTION_API_TOKEN=valid-token\n",
    )
    .unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
set -e
echo "I'm sorry, but I cannot assist with that request." >&2
echo "stream disconnected before completion: Incomplete response returned, reason: content_filter" >&2
exit 1
"#,
    );
    write_shell_script(
        &bin_dir,
        "notion_api_cli",
        r#"#!/bin/sh
set -e
if [ "$1" != "create-comment" ]; then
  echo "unexpected command $1" >&2
  exit 2
fi
printf '%s\n' "$@" > "$POSTED_NOTION_COMMENT_PATH"
"#,
    );
    write_shell_script(
        &bin_dir,
        "claude",
        r#"#!/bin/sh
set -e
echo invoked >> "$CLAUDE_COUNTER_PATH"
exit 7
"#,
    );

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("POSTED_NOTION_COMMENT_PATH", posted_path.to_str().unwrap()),
        ("CLAUDE_COUNTER_PATH", claude_counter_path.to_str().unwrap()),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let mut params = build_params(&workspace);
    params.channel = "notion".to_string();
    let result = run_task(&params).unwrap();
    assert!(workspace.join(".notion_api_replied").exists());
    let posted = fs::read_to_string(posted_path).unwrap();
    assert!(posted.contains("create-comment"));
    assert!(posted.contains("35d37bc1-a421-81cb-887c-c2510bbb37b9"));
    assert!(posted.contains("Azure/OpenAI content filter"));
    assert!(!claude_counter_path.exists());
    assert!(
        result
            .terminal_error_message
            .as_deref()
            .unwrap_or("")
            .contains("content-filter explanation"),
        "Notion content-filter explanation should be delivered but recorded as failed terminal status"
    );
}

#[test]
#[cfg(unix)]
fn run_task_recovers_generic_reply_after_timeout_without_fallback() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_timeout_preserve_primary_draft").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    let claude_counter_path = temp.path.join("claude_invocations.txt");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_shell_script(
        &bin_dir,
        "codex",
        r#"#!/bin/sh
set -e
cat > reply_email_draft.html <<'HTML'
<p>Draft exists, but it is not a valid final investment artifact yet.</p>
HTML
sleep "${SLEEP_SECS:-2}"
"#,
    );
    write_shell_script(
        &bin_dir,
        "claude",
        &format!(
            r#"#!/bin/sh
set -e
printf '1' > "{claude_counter_path}"
echo "Claude fallback should not have run" >&2
exit 17
"#,
            claude_counter_path = claude_counter_path.display()
        ),
    );

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
    let result = run_task(&params).expect("generic reply should recover after timeout");
    let reply = fs::read_to_string(&result.reply_html_path).unwrap();
    assert!(reply.contains("Draft exists"));
    let note = result.recovery_note.unwrap_or_default();
    assert!(note.contains("Recovered ready reply artifact written during this run"));
    assert!(!note.contains("Claude fallback"));
    assert!(
        !claude_counter_path.exists(),
        "Claude fallback should not run once a generic reply artifact is recovered"
    );
    assert!(
        !workspace.join(".run_task_trace_codex_primary").exists(),
        "primary trace should not be archived into a fallback directory when fallback never runs"
    );
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
fn run_task_accepts_structured_investment_reply_from_fake_codex() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_investment_structured").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "NVDA investment memo",
        "Give me deep research on NVDA and tell me whether now is a good time to buy.",
    );

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::InvestmentStructured).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result = run_task(&build_params(&workspace)).unwrap();
    let html = fs::read_to_string(result.reply_html_path).unwrap();
    assert_non_empty_renderable_html(&html);
    assert!(html.contains("Decision Card"));
    assert!(html.contains("Verified Facts"));
    assert!(html.contains("Triggers"));
    assert!(html.contains("New Money Action"));
    assert!(html.contains("Existing Holder Action"));
}

#[test]
#[cfg(unix)]
fn run_task_final_artifact_preserves_non_empty_html_after_email_normalization() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_investment_final_artifact").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "NVDA investment memo",
        "Give me deep research on NVDA and tell me whether now is a good time to buy.",
    );

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::InvestmentStructured).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result = run_task(&build_params(&workspace)).unwrap();
    assert!(result.reply_html_path.ends_with("reply_email_draft.html"));

    let raw_reply = fs::read_to_string(&result.reply_html_path).unwrap();
    assert_non_empty_renderable_html(&raw_reply);

    let final_html = normalize_email_html("NVDA investment memo", &raw_reply);
    assert_non_empty_renderable_html(&final_html);
}

#[test]
#[cfg(unix)]
fn run_task_generic_investment_reply_no_longer_triggers_claude_fallback() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_investment_fallback").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "NVDA investment memo",
        "Give me deep research on NVDA and tell me whether now is a good time to buy.",
    );

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::InvestmentGeneric).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result = run_task(&build_params(&workspace)).unwrap();
    let html = fs::read_to_string(result.reply_html_path).unwrap();
    assert_non_empty_renderable_html(&html);
    assert!(html.contains("wait until after earnings"));
    assert!(!html.contains("Decision Card"));
    let recovery = result.recovery_note.unwrap_or_default();
    assert!(!recovery.contains("Claude fallback"));
}

#[test]
#[cfg(unix)]
fn run_task_generic_investment_reply_does_not_require_fail_soft_when_non_empty() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp = TempDir::new("codex_task_investment_fail_closed").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    write_investment_request(
        &workspace,
        "NVDA investment memo",
        "Give me deep research on NVDA and tell me whether now is a good time to buy.",
    );

    let home_dir = temp.path.join("home");
    let bin_dir = temp.path.join("bin");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_codex(&bin_dir, FakeCodexMode::InvestmentGeneric).unwrap();

    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), old_path);
    let _env = EnvGuard::set(&[
        ("HOME", home_dir.to_str().unwrap()),
        ("PATH", &new_path),
        ("AZURE_OPENAI_API_KEY_BACKUP", "test-key"),
        ("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.azure.com/"),
        ("GH_AUTH_DISABLED", "1"),
    ]);

    let result = run_task(&build_params(&workspace))
        .expect("generic non-empty investment reply should return directly");
    let html = fs::read_to_string(result.reply_html_path).unwrap();
    assert_non_empty_renderable_html(&html);
    let recovery = result.recovery_note.unwrap_or_default();
    assert!(html.contains("wait until after earnings"));
    assert!(!html.contains("Decision Card"));
    assert!(!recovery.contains("deterministic operational investment fallback"));
    assert!(!recovery.contains("Claude fallback"));
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

#[test]
#[cfg(unix)]
fn run_task_real_codex_investment_e2e_when_enabled() {
    let _lock = ENV_MUTEX.lock().unwrap();
    if !env_enabled("RUN_CODEX_E2E") {
        eprintln!("RUN_CODEX_E2E not set; skipping investment Codex E2E test.");
        return;
    }

    require_env("AZURE_OPENAI_API_KEY_BACKUP");
    require_env("AZURE_OPENAI_ENDPOINT_BACKUP");

    let temp = TempDir::new("codex_task_investment_real_e2e").unwrap();
    let workspace = create_workspace(&temp.path).unwrap();
    install_runtime_skills_and_employee_guidance(&workspace, "little_bear").unwrap();
    write_investment_request(
        &workspace,
        "NVDA investment memo",
        "Give me deep research on NVDA and tell me whether now is a good time to buy.",
    );

    let home_dir = temp.path.join("home");
    fs::create_dir_all(&home_dir).unwrap();
    let _env = EnvGuard::set(&[("HOME", home_dir.to_str().unwrap())]);

    let result = run_task(&build_params(&workspace)).unwrap_or_else(|err| {
        panic!("Real investment Codex E2E test failed: {err}");
    });
    let html = fs::read_to_string(&result.reply_html_path).unwrap();
    assert_non_empty_renderable_html(&html);
    assert!(
        html.len() <= 64 * 1024,
        "investment reply should stay within bounded artifact size"
    );
}
