mod test_support;

use run_task_module::RunTaskParams;
use scheduler_module::account_store::AccountStore;
use scheduler_module::employee_config::{EmployeeDirectory, EmployeeProfile};
use scheduler_module::index_store::IndexStore;
use scheduler_module::service::{
    process_inbound_payload, PostmarkInbound, ServiceConfig, DEFAULT_INBOUND_BODY_MAX_BYTES,
};
use scheduler_module::user_store::UserStore;
use scheduler_module::{Scheduler, SchedulerError, TaskExecution, TaskExecutor, TaskKind};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

const EXPECTED_SOUL_BLOCK: &str = r#"<SOUL>
Your name is Oliver, a little bear, who is cute and smart and capable. You always get task done.
Go bears!
</SOUL>
"#;

#[derive(Clone, Default)]
struct RecordingExecutor {
    sent_subjects: Arc<Mutex<Vec<String>>>,
}

struct EnvGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = env::var(key).ok();
        env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => env::set_var(self.key, value),
            None => env::remove_var(self.key),
        }
    }
}

fn test_employee_directory(root: &Path) -> (EmployeeProfile, EmployeeDirectory) {
    let agents_path = root.join("AGENTS.md");
    let claude_path = root.join("CLAUDE.md");
    let soul_path = root.join("SOUL.md");
    fs::write(&agents_path, EXPECTED_SOUL_BLOCK).expect("write agents");
    fs::write(&claude_path, EXPECTED_SOUL_BLOCK).expect("write claude");
    fs::write(&soul_path, EXPECTED_SOUL_BLOCK).expect("write soul");
    let addresses = vec!["service@example.com".to_string()];
    let address_set: HashSet<String> = addresses
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();
    let employee = EmployeeProfile {
        id: "test-employee".to_string(),
        display_name: None,
        runner: "codex".to_string(),
        model: None,
        addresses: addresses.clone(),
        address_set: address_set.clone(),
        runtime_root: None,
        agents_path: Some(agents_path),
        claude_path: Some(claude_path),
        soul_path: Some(soul_path),
        skills_dir: None,
        discord_enabled: false,
        slack_enabled: false,
        bluebubbles_enabled: false,
    };
    let mut employee_by_id = HashMap::new();
    employee_by_id.insert(employee.id.clone(), employee.clone());
    let mut service_addresses = HashSet::new();
    service_addresses.extend(address_set);
    let directory = EmployeeDirectory {
        employees: vec![employee.clone()],
        employee_by_id,
        default_employee_id: Some(employee.id.clone()),
        service_addresses,
    };
    (employee, directory)
}

impl TaskExecutor for RecordingExecutor {
    fn execute(&self, task: &TaskKind) -> Result<TaskExecution, SchedulerError> {
        match task {
            TaskKind::RunTask(run) => {
                let params = RunTaskParams {
                    workspace_dir: run.workspace_dir.clone(),
                    input_email_dir: run.input_email_dir.clone(),
                    input_attachments_dir: run.input_attachments_dir.clone(),
                    memory_dir: run.memory_dir.clone(),
                    reference_dir: run.reference_dir.clone(),
                    reply_to: run.reply_to.clone(),
                    model_name: run.model_name.clone(),
                    runner: run.runner.clone(),
                    codex_disabled: run.codex_disabled,
                    channel: run.channel.to_string(),
                    google_access_token:
                        scheduler_module::load_google_access_token_from_service_env(),
                    notion_access_token: None,
                    has_unified_account: false,
                    user_identities: Default::default(),
                    thread_epoch: run.thread_epoch,
                    thread_state_path: run.thread_state_path.clone(),
                };
                let output = match run_task_module::run_task(&params) {
                    Ok(output) => output,
                    Err(run_task_module::RunTaskError::Canceled { reason, .. }) => {
                        let mut execution = TaskExecution::default();
                        execution.skip_auto_reply = true;
                        execution.superseded = true;
                        execution.terminal_note = Some(reason);
                        return Ok(execution);
                    }
                    Err(err) => return Err(SchedulerError::TaskFailed(err.to_string())),
                };
                Ok(TaskExecution {
                    follow_up_tasks: output.scheduled_tasks,
                    follow_up_error: output.scheduled_tasks_error,
                    scheduler_actions: output.scheduler_actions,
                    scheduler_actions_error: output.scheduler_actions_error,
                    skip_auto_reply: false,
                    superseded: false,
                    terminal_note: output.recovery_note,
                })
            }
            TaskKind::SendReply(send) => {
                self.sent_subjects
                    .lock()
                    .expect("sent_subjects lock poisoned")
                    .push(send.subject.clone());
                Ok(TaskExecution::default())
            }
            TaskKind::Noop => Ok(TaskExecution::default()),
        }
    }
}

fn write_fake_codex(bin_dir: &Path) -> io::Result<()> {
    let script = r#"#!/bin/sh
set -e
if [ -f "incoming_email/postmark_payload.json" ]; then
  if command -v python3 >/dev/null 2>&1; then
    subject=$(python3 - <<'PY'
import json
with open("incoming_email/postmark_payload.json", "r", encoding="utf-8") as fh:
    payload = json.load(fh)
print(payload.get("Subject", "(no subject)"))
PY
    )
  elif command -v python >/dev/null 2>&1; then
    subject=$(python - <<'PY'
import json
with open("incoming_email/postmark_payload.json", "r", encoding="utf-8") as fh:
    payload = json.load(fh)
print(payload.get("Subject", "(no subject)"))
PY
    )
  else
    subject="(no subject)"
  fi
else
  subject="(no subject)"
fi
cat > reply_email_draft.html <<EOF
<html><body>Reply to ${subject}</body></html>
EOF
exit 0
"#;
    let path = bin_dir.join("codex");
    fs::write(&path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms)?;
    }
    Ok(())
}

fn write_slow_fake_codex(bin_dir: &Path) -> io::Result<()> {
    let script = r#"#!/bin/sh
set -e
printf 'started' > codex_started.flag
while true; do
  sleep 1
done
"#;
    let path = bin_dir.join("codex");
    fs::write(&path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms)?;
    }
    Ok(())
}

fn first_workspace_dir(root: &Path) -> PathBuf {
    let mut entries = fs::read_dir(root).expect("read workspaces dir");
    while let Some(entry) = entries.next() {
        let path = entry.expect("workspace entry").path();
        if path.is_dir() {
            return path;
        }
    }
    panic!("no workspace directory created");
}

fn wait_for_path(path: &Path, timeout: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {}", path.display());
}

#[test]
fn thread_latest_epoch_end_to_end() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    let users_root = root.join("users");
    let state_root = root.join("state");
    let bin_root = root.join("bin");
    let home_root = root.join("home");
    fs::create_dir_all(&users_root).expect("users root");
    fs::create_dir_all(&state_root).expect("state root");
    fs::create_dir_all(&bin_root).expect("bin root");
    fs::create_dir_all(&home_root).expect("home root");

    write_fake_codex(&bin_root).expect("write fake codex");
    let original_path = env::var("PATH").unwrap_or_default();
    let path_value = format!("{}:{}", bin_root.display(), original_path);
    let _path_guard = EnvGuard::set("PATH", path_value);
    let _api_guard = EnvGuard::set("AZURE_OPENAI_API_KEY_BACKUP", "test-key");
    let _endpoint_guard = EnvGuard::set("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.test");
    let _docker_guard = EnvGuard::set("RUN_TASK_DOCKER_IMAGE", "");
    let _home_guard = EnvGuard::set("HOME", &home_root);

    let Some(ingestion_db_url) =
        test_support::require_supabase_db_url("thread_latest_epoch_end_to_end")
    else {
        return;
    };
    let (employee_profile, employee_directory) = test_employee_directory(root);
    let config = ServiceConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        employee_id: employee_profile.id.clone(),
        employee_config_path: root.join("employee.toml"),
        employee_profile,
        employee_directory,
        workspace_root: root.join("workspaces"),
        scheduler_state_path: state_root.join("tasks.db"),
        processed_ids_path: state_root.join("processed_ids.txt"),
        ingestion_db_url,
        ingestion_poll_interval: Duration::from_millis(50),
        users_root: users_root.clone(),
        users_db_path: state_root.join("users.db"),
        task_index_path: state_root.join("task_index.db"),
        codex_model: "gpt-5.4".to_string(),
        codex_disabled: false,
        scheduler_poll_interval: Duration::from_millis(50),
        scheduler_max_concurrency: 2,
        scheduler_user_max_concurrency: 1,
        inbound_body_max_bytes: DEFAULT_INBOUND_BODY_MAX_BYTES,
        skills_source_dir: None,
        slack_bot_token: None,
        slack_bot_user_id: None,
        slack_store_path: state_root.join("slack.db"),
        slack_client_id: None,
        slack_client_secret: None,
        slack_redirect_uri: None,
        discord_bot_token: None,
        discord_bot_user_id: None,
        google_docs_enabled: false,
        bluebubbles_url: None,
        bluebubbles_password: None,
        telegram_bot_token: None,
        whatsapp_access_token: None,
        whatsapp_phone_number_id: None,
        whatsapp_verify_token: None,
    };

    let user_store = UserStore::new(&config.users_db_path).expect("user store");
    let index_store = IndexStore::new(&config.task_index_path).expect("index store");
    let account_store = AccountStore::new(&config.ingestion_db_url).expect("account store");

    let inbound_raw_1 = r#"{
  "From": "Alice <alice@example.com>",
  "To": "Service <service@example.com>",
  "Subject": "Hello 1",
  "TextBody": "First message",
  "Attachments": [
    {
      "Name": "brief_v1.txt",
      "Content": "djE=",
      "ContentType": "text/plain"
    }
  ],
  "Headers": [{"Name": "Message-ID", "Value": "<msg-1@example.com>"}]
}"#;
    let payload_1: PostmarkInbound = serde_json::from_str(inbound_raw_1).expect("parse inbound 1");
    process_inbound_payload(
        &config,
        &user_store,
        &index_store,
        &account_store,
        &payload_1,
        inbound_raw_1.as_bytes(),
        None,
    )
    .expect("process inbound 1");

    let user = user_store
        .get_or_create_user("email", "alice@example.com")
        .expect("user lookup");
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let workspace = first_workspace_dir(&user_paths.workspaces_root);

    let executor = RecordingExecutor::default();
    let mut scheduler =
        Scheduler::load(&user_paths.tasks_db_path, executor.clone()).expect("load scheduler");
    scheduler.tick().expect("tick run_task 1");

    let pending_send = scheduler
        .tasks()
        .iter()
        .filter(|task| matches!(task.kind, TaskKind::SendReply(_)) && task.enabled)
        .count();
    assert_eq!(pending_send, 1, "pending send should exist");

    let inbound_raw_2 = r#"{
  "From": "Alice <alice@example.com>",
  "To": "Service <service@example.com>",
  "Subject": "Hello 2",
  "TextBody": "Second message",
  "Attachments": [
    {
      "Name": "brief_v2.txt",
      "Content": "djI=",
      "ContentType": "text/plain"
    }
  ],
  "Headers": [
    {"Name": "Message-ID", "Value": "<msg-2@example.com>"},
    {"Name": "References", "Value": "<msg-1@example.com>"}
  ]
}"#;
    let payload_2: PostmarkInbound = serde_json::from_str(inbound_raw_2).expect("parse inbound 2");
    process_inbound_payload(
        &config,
        &user_store,
        &index_store,
        &account_store,
        &payload_2,
        inbound_raw_2.as_bytes(),
        None,
    )
    .expect("process inbound 2");

    let thread_request =
        fs::read_to_string(workspace.join("incoming_email").join("thread_request.md"))
            .expect("thread_request");
    assert!(
        thread_request.contains("First message"),
        "thread_request should include the first message"
    );
    assert!(
        thread_request.contains("Second message"),
        "thread_request should include the second message"
    );
    assert!(
        thread_request.contains("Latest inbound message"),
        "thread_request should mark the latest inbound message"
    );
    assert!(
        workspace
            .join("incoming_attachments")
            .join("brief_v1.txt")
            .exists(),
        "merged attachment view should keep the first attachment"
    );
    assert!(
        workspace
            .join("incoming_attachments")
            .join("brief_v2.txt")
            .exists(),
        "merged attachment view should keep the second attachment"
    );
    assert!(
        workspace
            .join("incoming_attachments")
            .join("thread_manifest.json")
            .exists(),
        "merged attachment manifest should be written"
    );

    let mut scheduler =
        Scheduler::load(&user_paths.tasks_db_path, executor.clone()).expect("reload scheduler");
    let enabled_sends_after_cancel = scheduler
        .tasks()
        .iter()
        .filter(|task| matches!(task.kind, TaskKind::SendReply(_)) && task.enabled)
        .count();
    assert_eq!(
        enabled_sends_after_cancel, 0,
        "stale send should be cancelled"
    );

    scheduler.tick().expect("tick run_task 2");
    scheduler.tick().expect("tick send 2");

    let sent = executor
        .sent_subjects
        .lock()
        .expect("sent_subjects lock poisoned");
    assert_eq!(sent.len(), 1, "only latest send should fire");
    let reply_html =
        fs::read_to_string(workspace.join("reply_email_draft.html")).expect("reply draft");
    assert!(
        reply_html.contains("Hello 2"),
        "latest reply should use second email"
    );

    let agents_path = workspace.join("AGENTS.md");
    let claude_path = workspace.join("CLAUDE.md");
    assert_eq!(
        fs::read_to_string(&agents_path).expect("read AGENTS.md"),
        EXPECTED_SOUL_BLOCK
    );
    assert_eq!(
        fs::read_to_string(&claude_path).expect("read CLAUDE.md"),
        EXPECTED_SOUL_BLOCK
    );

    let drafts_dir = workspace.join("drafts");
    let drafts_count = fs::read_dir(drafts_dir).expect("drafts dir").count();
    assert!(drafts_count >= 2, "draft history should be preserved");
}

#[test]
fn follow_up_supersedes_running_task_and_reruns_from_merged_thread_snapshot() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    let users_root = root.join("users");
    let state_root = root.join("state");
    let bin_root = root.join("bin");
    let home_root = root.join("home");
    fs::create_dir_all(&users_root).expect("users root");
    fs::create_dir_all(&state_root).expect("state root");
    fs::create_dir_all(&bin_root).expect("bin root");
    fs::create_dir_all(&home_root).expect("home root");

    write_slow_fake_codex(&bin_root).expect("write slow fake codex");
    let original_path = env::var("PATH").unwrap_or_default();
    let path_value = format!("{}:{}", bin_root.display(), original_path);
    let _path_guard = EnvGuard::set("PATH", path_value);
    let _api_guard = EnvGuard::set("AZURE_OPENAI_API_KEY_BACKUP", "test-key");
    let _endpoint_guard = EnvGuard::set("AZURE_OPENAI_ENDPOINT_BACKUP", "https://example.test");
    let _docker_guard = EnvGuard::set("RUN_TASK_DOCKER_IMAGE", "");
    let _home_guard = EnvGuard::set("HOME", &home_root);
    let _timeout_guard = EnvGuard::set("RUN_TASK_TIMEOUT_SECS", "5");

    let Some(ingestion_db_url) = test_support::require_supabase_db_url(
        "follow_up_supersedes_running_task_and_reruns_from_merged_thread_snapshot",
    ) else {
        return;
    };
    let (employee_profile, employee_directory) = test_employee_directory(root);
    let config = ServiceConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        employee_id: employee_profile.id.clone(),
        employee_config_path: root.join("employee.toml"),
        employee_profile,
        employee_directory,
        workspace_root: root.join("workspaces"),
        scheduler_state_path: state_root.join("tasks.db"),
        processed_ids_path: state_root.join("processed_ids.txt"),
        ingestion_db_url,
        ingestion_poll_interval: Duration::from_millis(50),
        users_root: users_root.clone(),
        users_db_path: state_root.join("users.db"),
        task_index_path: state_root.join("task_index.db"),
        codex_model: "gpt-5.4".to_string(),
        codex_disabled: false,
        scheduler_poll_interval: Duration::from_millis(50),
        scheduler_max_concurrency: 2,
        scheduler_user_max_concurrency: 1,
        inbound_body_max_bytes: DEFAULT_INBOUND_BODY_MAX_BYTES,
        skills_source_dir: None,
        slack_bot_token: None,
        slack_bot_user_id: None,
        slack_store_path: state_root.join("slack.db"),
        slack_client_id: None,
        slack_client_secret: None,
        slack_redirect_uri: None,
        discord_bot_token: None,
        discord_bot_user_id: None,
        google_docs_enabled: false,
        bluebubbles_url: None,
        bluebubbles_password: None,
        telegram_bot_token: None,
        whatsapp_access_token: None,
        whatsapp_phone_number_id: None,
        whatsapp_verify_token: None,
    };

    let user_store = UserStore::new(&config.users_db_path).expect("user store");
    let index_store = IndexStore::new(&config.task_index_path).expect("index store");
    let account_store = AccountStore::new(&config.ingestion_db_url).expect("account store");

    let inbound_raw_1 = r#"{
  "From": "Alice <alice@example.com>",
  "To": "Service <service@example.com>",
  "Subject": "Hello 1",
  "TextBody": "First message",
  "Attachments": [
    {
      "Name": "brief_v1.txt",
      "Content": "djE=",
      "ContentType": "text/plain"
    }
  ],
  "Headers": [{"Name": "Message-ID", "Value": "<msg-1@example.com>"}]
}"#;
    let payload_1: PostmarkInbound = serde_json::from_str(inbound_raw_1).expect("parse inbound 1");
    process_inbound_payload(
        &config,
        &user_store,
        &index_store,
        &account_store,
        &payload_1,
        inbound_raw_1.as_bytes(),
        None,
    )
    .expect("process inbound 1");

    let user = user_store
        .get_or_create_user("email", "alice@example.com")
        .expect("user lookup");
    let user_paths = user_store.user_paths(&config.users_root, &user.user_id);
    let workspace = first_workspace_dir(&user_paths.workspaces_root);

    let executor = RecordingExecutor::default();
    let tasks_db_path = user_paths.tasks_db_path.clone();
    let tick_executor = executor.clone();
    let tick_handle = std::thread::spawn(move || {
        let mut scheduler =
            Scheduler::load(&tasks_db_path, tick_executor).expect("load scheduler for task 1");
        scheduler.tick().expect("tick run_task 1");
    });

    wait_for_path(
        &workspace.join("codex_started.flag"),
        Duration::from_secs(2),
    );

    let inbound_raw_2 = r#"{
  "From": "Alice <alice@example.com>",
  "To": "Service <service@example.com>",
  "Subject": "Hello 2",
  "TextBody": "Second message",
  "Attachments": [
    {
      "Name": "brief_v2.txt",
      "Content": "djI=",
      "ContentType": "text/plain"
    }
  ],
  "Headers": [
    {"Name": "Message-ID", "Value": "<msg-2@example.com>"},
    {"Name": "References", "Value": "<msg-1@example.com>"}
  ]
}"#;
    let payload_2: PostmarkInbound = serde_json::from_str(inbound_raw_2).expect("parse inbound 2");
    process_inbound_payload(
        &config,
        &user_store,
        &index_store,
        &account_store,
        &payload_2,
        inbound_raw_2.as_bytes(),
        None,
    )
    .expect("process inbound 2");

    tick_handle.join().expect("join superseded task");

    let status_rows = scheduler_module::load_tasks_with_status(&user_paths.tasks_db_path);
    assert!(
        status_rows
            .iter()
            .any(|task| task.execution_status.as_deref() == Some("superseded")),
        "first run should complete as superseded"
    );

    let thread_request =
        fs::read_to_string(workspace.join("incoming_email").join("thread_request.md"))
            .expect("thread_request");
    assert!(
        thread_request.contains("First message"),
        "merged request should retain the original message"
    );
    assert!(
        thread_request.contains("Second message"),
        "merged request should retain the follow-up message"
    );
    assert!(
        workspace
            .join("incoming_attachments")
            .join("brief_v1.txt")
            .exists(),
        "merged attachment view should preserve the original attachment"
    );
    assert!(
        workspace
            .join("incoming_attachments")
            .join("brief_v2.txt")
            .exists(),
        "merged attachment view should include the follow-up attachment"
    );

    write_fake_codex(&bin_root).expect("swap to fast fake codex");

    let mut scheduler =
        Scheduler::load(&user_paths.tasks_db_path, executor.clone()).expect("reload scheduler");
    let enabled_sends_before_rerun = scheduler
        .tasks()
        .iter()
        .filter(|task| matches!(task.kind, TaskKind::SendReply(_)) && task.enabled)
        .count();
    assert_eq!(
        enabled_sends_before_rerun, 0,
        "superseded run should not leave an enabled send task behind"
    );

    scheduler.tick().expect("tick run_task 2");
    scheduler.tick().expect("tick send 2");

    let sent = executor
        .sent_subjects
        .lock()
        .expect("sent_subjects lock poisoned");
    assert_eq!(sent.len(), 1, "only the fresh rerun should send a reply");

    let reply_html =
        fs::read_to_string(workspace.join("reply_email_draft.html")).expect("reply draft");
    assert!(
        reply_html.contains("Hello 2"),
        "fresh rerun should reply using the latest follow-up"
    );
}
