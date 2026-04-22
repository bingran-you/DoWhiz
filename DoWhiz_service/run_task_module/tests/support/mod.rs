use run_task_module::RunTaskParams;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub static ENV_MUTEX: Mutex<()> = Mutex::new(());

pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> io::Result<Self> {
        let mut path = env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        path.push(format!("{}_{}_{}", label, process::id(), now));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub struct EnvGuard {
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    pub fn set(vars: &[(&str, &str)]) -> Self {
        let mut saved = Vec::with_capacity(vars.len() + 1);
        let mut has_docker_override = false;
        let mut has_docker_use_override = false;
        let mut has_codex_e2e_override = false;
        for (key, value) in vars {
            saved.push((key.to_string(), env::var_os(key)));
            env::set_var(key, value);
            if *key == "RUN_TASK_DOCKER_IMAGE" {
                has_docker_override = true;
            }
            if *key == "RUN_TASK_USE_DOCKER" {
                has_docker_use_override = true;
            }
            if *key == "RUN_CODEX_E2E" {
                has_codex_e2e_override = true;
            }
        }
        if !has_docker_override {
            saved.push((
                "RUN_TASK_DOCKER_IMAGE".to_string(),
                env::var_os("RUN_TASK_DOCKER_IMAGE"),
            ));
            env::set_var("RUN_TASK_DOCKER_IMAGE", "");
        }
        if !has_docker_use_override {
            saved.push((
                "RUN_TASK_USE_DOCKER".to_string(),
                env::var_os("RUN_TASK_USE_DOCKER"),
            ));
            env::set_var("RUN_TASK_USE_DOCKER", "0");
        }
        if !has_codex_e2e_override {
            saved.push(("RUN_CODEX_E2E".to_string(), env::var_os("RUN_CODEX_E2E")));
            env::set_var("RUN_CODEX_E2E", "0");
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            match value {
                Some(prev) => env::set_var(&key, prev),
                None => env::remove_var(&key),
            }
        }
    }
}

#[allow(dead_code)]
pub struct EnvUnsetGuard {
    saved: Vec<(String, Option<OsString>)>,
}

#[allow(dead_code)]
impl EnvUnsetGuard {
    pub fn remove(keys: &[&str]) -> Self {
        let mut saved = Vec::with_capacity(keys.len());
        for key in keys {
            saved.push((key.to_string(), env::var_os(key)));
            env::remove_var(key);
        }
        Self { saved }
    }
}

impl Drop for EnvUnsetGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            match value {
                Some(prev) => env::set_var(&key, prev),
                None => env::remove_var(&key),
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum FakeCodexMode {
    Success,
    InvestmentStructured,
    InvestmentGeneric,
    NoOutput,
    EmptyReply,
    Fail,
    ReplyThenFail,
    TurnAborted,
    GithubEnvCheck,
    X402EnvCheck,
    EnsureNoYolo,
    EnsureYolo,
    EnsureDangerSandbox,
    EnsureAddDir,
    EnsureHumanApprovalGateMcpEnv,
    Sleep,
}

#[cfg(unix)]
pub fn write_fake_codex(dir: &Path, mode: FakeCodexMode) -> io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let script_path = dir.join("codex");
    let script = match mode {
        FakeCodexMode::Success => {
            r#"#!/bin/sh
set -e
echo '{"type":"item.delta","item":{"type":"agent_message"},"delta":{"text":"ok"}}'
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeCodexMode::InvestmentStructured => {
            r#"#!/bin/sh
set -e
cat > reply_email_draft.html <<'HTML'
<h2>Request Framing</h2>
<ul>
  <li><strong>Ticker:</strong> NVDA</li>
  <li><strong>Name:</strong> NVIDIA</li>
  <li><strong>Type:</strong> Stock</li>
  <li><strong>Research Mode:</strong> Deep research</li>
  <li><strong>User Objective:</strong> Decide whether now is actionable (stated)</li>
  <li><strong>Horizon:</strong> Long-term (inferred)</li>
  <li><strong>Question Type:</strong> Long-term accumulation</li>
</ul>
<h2>Final Recommendation</h2>
<ul>
  <li><strong>Rating:</strong> Wait</li>
  <li><strong>Horizon:</strong> Long-term (inferred)</li>
  <li><strong>Confidence:</strong> Medium</li>
  <li><strong>Timing Verdict:</strong> Wait</li>
  <li><strong>Add Criteria:</strong> Better entry after earnings or improved valuation support.</li>
  <li><strong>Invalidation Criteria:</strong> Demand slows or margin guidance weakens.</li>
  <li><strong>Biggest Near-Term Risk:</strong> Event volatility around earnings.</li>
  <li><strong>Biggest Long-Term Strength:</strong> AI compute leadership.</li>
</ul>
<h2>Verified Facts</h2>
<ul><li>Fact.</li></ul>
<h2>Derived Metrics</h2>
<ul><li>Metric: price / eps = 10x</li></ul>
<h2>Inference / Judgment</h2>
<ul><li>Judgment.</li></ul>
<h2>Scenario Analysis</h2>
<p><strong>Bull Case:</strong> Demand remains strong.</p>
<p><strong>Base Case:</strong> Growth normalizes.</p>
<p><strong>Bear Case:</strong> Spending slows.</p>
HTML
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeCodexMode::InvestmentGeneric => {
            r#"#!/bin/sh
set -e
cat > reply_email_draft.html <<'HTML'
<p>NVIDIA is a good business, but I would wait until after earnings and buy in tranches instead of going all in now.</p>
HTML
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeCodexMode::NoOutput => {
            r#"#!/bin/sh
set -e
echo '{"type":"item.delta","item":{"type":"agent_message"},"delta":{"text":"ok"}}'
"#
        }
        FakeCodexMode::EmptyReply => {
            r#"#!/bin/sh
set -e
printf '   \n\t' > reply_email_draft.html
mkdir -p reply_email_attachments
"#
        }
        FakeCodexMode::Fail => {
            r#"#!/bin/sh
echo "simulated failure" >&2
exit 2
"#
        }
        FakeCodexMode::ReplyThenFail => {
            r#"#!/bin/sh
set -e
echo "<html><body>Recovered reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
echo "response.failed event received" >&2
exit 23
"#
        }
        FakeCodexMode::TurnAborted => {
            r#"#!/bin/sh
set -e
echo '{"type":"event_msg","payload":{"type":"agent_message","message":"starting"}}'
echo '{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}'
"#
        }
        FakeCodexMode::GithubEnvCheck => {
            r#"#!/bin/sh
set -e
check_env() {
  key="$1"
  eval "value=\${$key}"
  if [ -z "$value" ]; then
    echo "missing $key" >&2
    exit 3
  fi
}
check_env "GH_TOKEN"
check_env "GITHUB_TOKEN"
check_env "GITHUB_USERNAME"
if [ -z "$GIT_ASKPASS" ] || [ ! -x "$GIT_ASKPASS" ]; then
  echo "missing GIT_ASKPASS" >&2
  exit 3
fi
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeCodexMode::X402EnvCheck => {
            r#"#!/bin/sh
set -e
check_env() {
  key="$1"
  eval "value=\${$key}"
  if [ -z "$value" ]; then
    echo "missing $key" >&2
    exit 3
  fi
}
check_exact_env() {
  key="$1"
  expected_key="EXPECTED_${key}"
  eval "expected=\${$expected_key}"
  if [ -n "$expected" ]; then
    eval "actual=\${$key}"
    if [ "$actual" != "$expected" ]; then
      echo "unexpected $key: expected '$expected' got '$actual'" >&2
      exit 3
    fi
  fi
}
check_env "GOATX402_API_URL"
check_env "GOATX402_MERCHANT_ID"
check_env "GOATX402_API_KEY"
check_env "GOATX402_API_SECRET"
check_exact_env "GOATX402_API_URL"
check_exact_env "GOATX402_MERCHANT_ID"
check_exact_env "GOATX402_API_KEY"
check_exact_env "GOATX402_API_SECRET"
echo "<html><body>x402 payment route ready; simulated tx submitted</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "mock_tx_hash=0xabc123" > reply_email_attachments/x402_receipt.txt
"#
        }
        FakeCodexMode::EnsureNoYolo => {
            r#"#!/bin/sh
set -e
for arg in "$@"; do
  if [ "$arg" = "--yolo" ]; then
    echo "unexpected --yolo" >&2
    exit 3
  fi
done
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeCodexMode::EnsureYolo => {
            r#"#!/bin/sh
set -e
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
"#
        }
        FakeCodexMode::EnsureDangerSandbox => {
            r#"#!/bin/sh
set -e
found="0"
prev=""
for arg in "$@"; do
  if [ "$prev" = "-c" ] && [ "$arg" = "sandbox=\"danger-full-access\"" ]; then
    found="1"
    break
  fi
  prev="$arg"
done
if [ "$found" != "1" ]; then
  echo "missing danger-full-access sandbox arg" >&2
  exit 3
fi
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeCodexMode::EnsureAddDir => {
            r#"#!/bin/sh
set -e
expected="${EXPECTED_ADD_DIR:-}"
found="0"
prev=""
for arg in "$@"; do
  if [ "$prev" = "--add-dir" ]; then
    if [ -z "$expected" ] || [ "$arg" = "$expected" ]; then
      found="1"
      break
    fi
  fi
  prev="$arg"
done
if [ "$found" != "1" ]; then
  echo "missing --add-dir ${expected}" >&2
  exit 3
fi
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeCodexMode::EnsureHumanApprovalGateMcpEnv => {
            r#"#!/bin/sh
set -e
if [ "${HUMAN_APPROVAL_GATE_REQUIRE_MCP:-}" != "1" ]; then
  echo "missing HUMAN_APPROVAL_GATE_REQUIRE_MCP=1" >&2
  exit 3
fi
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeCodexMode::Sleep => {
            r#"#!/bin/sh
set -e
sleep "${SLEEP_SECS:-2}"
"#
        }
    };

    fs::write(&script_path, script)?;
    let mut perms = fs::metadata(&script_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms)?;
    Ok(script_path)
}

#[cfg(unix)]
#[allow(dead_code)]
pub fn write_fake_gh(dir: &Path) -> io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let script_path = dir.join("gh");
    let script = r#"#!/bin/sh
set -e
if [ "$1" = "auth" ] && [ "$2" = "login" ]; then
  token="$(cat)"
  if [ -z "$token" ]; then
    echo "missing token" >&2
    exit 3
  fi
  exit 0
fi
if [ "$1" = "auth" ] && [ "$2" = "setup-git" ]; then
  exit 0
fi
if [ "$1" = "auth" ] && [ "$2" = "status" ]; then
  exit 0
fi
exit 0
"#;
    fs::write(&script_path, script)?;
    let mut perms = fs::metadata(&script_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms)?;
    Ok(script_path)
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum FakeClaudeMode {
    Success,
    InvestmentStructured,
    InvestmentGeneric,
    EnsureModel,
    Fail,
    ReplyThenSleep,
    Sleep,
}

#[cfg(unix)]
#[allow(dead_code)]
pub fn write_fake_claude(dir: &Path, mode: FakeClaudeMode) -> io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let script_path = dir.join("claude");
    let script = match mode {
        FakeClaudeMode::Success => {
            r#"#!/bin/sh
set -e
echo '{"type":"message_delta","delta":{"text":"ok"}}'
echo "<html><body>Test reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeClaudeMode::InvestmentStructured => {
            r#"#!/bin/sh
set -e
echo '{"type":"message_delta","delta":{"text":"ok"}}'
cat > reply_email_draft.html <<'HTML'
<h2>Request Framing</h2>
<ul>
  <li><strong>Ticker:</strong> NVDA</li>
  <li><strong>Name:</strong> NVIDIA</li>
  <li><strong>Type:</strong> Stock</li>
  <li><strong>Research Mode:</strong> Deep research</li>
  <li><strong>User Objective:</strong> Decide whether now is actionable (stated)</li>
  <li><strong>Horizon:</strong> Long-term (inferred)</li>
  <li><strong>Question Type:</strong> Long-term accumulation</li>
</ul>
<h2>Final Recommendation</h2>
<ul>
  <li><strong>Rating:</strong> Wait</li>
  <li><strong>Horizon:</strong> Long-term (inferred)</li>
  <li><strong>Confidence:</strong> Medium</li>
  <li><strong>Timing Verdict:</strong> Wait</li>
  <li><strong>Add Criteria:</strong> Better entry after earnings or improved valuation support.</li>
  <li><strong>Invalidation Criteria:</strong> Demand slows or margin guidance weakens.</li>
  <li><strong>Biggest Near-Term Risk:</strong> Event volatility around earnings.</li>
  <li><strong>Biggest Long-Term Strength:</strong> AI compute leadership.</li>
</ul>
<h2>Verified Facts</h2>
<ul><li>Fact.</li></ul>
<h2>Derived Metrics</h2>
<ul><li>Metric: price / eps = 10x</li></ul>
<h2>Inference / Judgment</h2>
<ul><li>Judgment.</li></ul>
<h2>Scenario Analysis</h2>
<p><strong>Bull Case:</strong> Demand remains strong.</p>
<p><strong>Base Case:</strong> Growth normalizes.</p>
<p><strong>Bear Case:</strong> Spending slows.</p>
HTML
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeClaudeMode::InvestmentGeneric => {
            r#"#!/bin/sh
set -e
echo '{"type":"message_delta","delta":{"text":"ok"}}'
cat > reply_email_draft.html <<'HTML'
<p>Good business, but not a good all-in buy. Wait until after earnings.</p>
HTML
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeClaudeMode::EnsureModel => {
            r#"#!/bin/sh
set -e
expected="${EXPECTED_CLAUDE_MODEL:-}"
actual=""
prev=""
for arg in "$@"; do
  if [ "$prev" = "--model" ]; then
    actual="$arg"
    break
  fi
  prev="$arg"
done
if [ -n "$expected" ] && [ "$actual" != "$expected" ]; then
  echo "unexpected claude model: expected '$expected' got '$actual'" >&2
  exit 3
fi
echo '{"type":"message_delta","delta":{"text":"ok"}}'
echo "<html><body>Claude fallback reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
"#
        }
        FakeClaudeMode::Fail => {
            r#"#!/bin/sh
echo "simulated claude failure" >&2
exit 7
"#
        }
        FakeClaudeMode::ReplyThenSleep => {
            r#"#!/bin/sh
set -e
echo '{"type":"message_delta","delta":{"text":"partial ok"}}'
echo "<html><body>Claude timeout recovery reply</body></html>" > reply_email_draft.html
mkdir -p reply_email_attachments
echo "attachment" > reply_email_attachments/attachment.txt
sleep "${SLEEP_SECS:-2}"
"#
        }
        FakeClaudeMode::Sleep => {
            r#"#!/bin/sh
set -e
sleep "${SLEEP_SECS:-2}"
"#
        }
    };

    fs::write(&script_path, script)?;
    let mut perms = fs::metadata(&script_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms)?;
    Ok(script_path)
}

pub fn create_workspace(root: &Path) -> io::Result<PathBuf> {
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace)?;
    fs::create_dir_all(workspace.join("incoming_email"))?;
    fs::create_dir_all(workspace.join("incoming_attachments"))?;
    fs::create_dir_all(workspace.join("memory"))?;
    fs::create_dir_all(workspace.join("references"))?;

    fs::write(
        workspace.join("incoming_email").join("email.html"),
        "<pre>Hello</pre>",
    )?;
    fs::write(
        workspace.join("incoming_attachments").join("doc_v1.txt"),
        "v1",
    )?;
    fs::write(
        workspace.join("incoming_attachments").join("doc_v2.txt"),
        "v2",
    )?;
    Ok(workspace)
}

pub fn build_params(workspace: &Path) -> RunTaskParams {
    RunTaskParams {
        workspace_dir: workspace.to_path_buf(),
        input_email_dir: PathBuf::from("incoming_email"),
        input_attachments_dir: PathBuf::from("incoming_attachments"),
        memory_dir: PathBuf::from("memory"),
        reference_dir: PathBuf::from("references"),
        reply_to: vec!["user@example.com".to_string()],
        model_name: "gpt-5.4".to_string(),
        runner: "codex".to_string(),
        codex_disabled: false,
        channel: "email".to_string(),
        google_access_token: std::env::var("GOOGLE_ACCESS_TOKEN").ok(),
        notion_access_token: std::env::var("NOTION_API_TOKEN").ok(),
        has_unified_account: true, // Default to true for tests
        user_identities: Default::default(),
        thread_epoch: None,
        thread_state_path: None,
    }
}

pub fn install_runtime_skills_and_employee_guidance(
    workspace: &Path,
    employee_id: &str,
) -> io::Result<()> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service_root = manifest_dir
        .parent()
        .expect("run_task_module lives under DoWhiz_service");
    let skills_root = service_root.join("skills");
    let employee_root = service_root.join("employees").join(employee_id);

    copy_dir_recursive(&skills_root, &workspace.join(".agents").join("skills"))?;
    for filename in ["AGENTS.md", "CLAUDE.md", "SOUL.md"] {
        let src = employee_root.join(filename);
        if src.exists() {
            fs::copy(src, workspace.join(filename))?;
        }
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}
