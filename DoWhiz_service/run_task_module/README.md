# run_task_module

Workspace-based task executor used by scheduler `RunTask` jobs.

Runners:
- `codex`
- `claude`

## Inputs / Outputs

Required input directories (relative to `workspace_dir`):
- `incoming_email`
- `incoming_attachments`
- `memory`
- `references`

Thread follow-up conventions:
- `incoming_email/postmark_payload.json` / `email.html` remain the latest raw inbound payload.
- `incoming_email/thread_request.md` is the canonical merged request when follow-up messages supersede an in-flight run.
- `incoming_email/thread_history.md` maps the full inbound thread and raw source files.
- `incoming_attachments/` is the merged attachment view for the active thread; per-message originals remain under `incoming_attachments/entries/`.

Output files are channel-aware:
- email/google workspace channels -> `reply_email_draft.html` + `reply_email_attachments/`
- chat channels (slack/discord/telegram/sms/whatsapp/bluebubbles) -> `reply_message.txt` + `reply_attachments/`

For email tasks, `reply_email_draft.html` is the pre-send workspace artifact. The final user-visible
HTML is produced later by `send_emails_module::normalize_email_html(...)`, so tests that care about
the rendered customer-facing structure should grade the normalized final HTML, not just an internal
model transcript or the raw shell logs.
For investment replies, the final artifact contract is intentionally stricter than a generic memo:
the normalized HTML must preserve the scan-first decision card, dual action tracks, expectations,
and inline clickable evidence chips with tiered sources.

Late-finalization recovery:
- when Codex has already written the expected reply artifact, `run_task` now treats that artifact as
  a recoverable completion signal even if the CLI later disconnects, refuses, or exits non-zero
  during finalization
- warm-pool executions also validate the completion exit code and require the expected reply
  artifact to be non-empty before reporting success, so Codex failures can fall through to the
  normal error/fallback path instead of being recorded as successful no-reply runs
- when scheduler warm-pool mode is enabled and a warm-pool run still fails, the scheduler now
  retries once through the normal direct `run_task` path so the Codex -> Claude fallback remains
  available for those jobs too
- optional `RUN_TASK_CODEX_TIMEOUT_SECS=<seconds>` caps the primary Codex runtime; by default
  Azure ACI Codex runs are time-boxed to 900 seconds so Claude fallback can still fire within a
  much larger overall `RUN_TASK_TIMEOUT_SECS` window
- optional `RUN_TASK_CODEX_FALLBACK_TIMEOUT_SECS=<seconds>` caps Claude fallback runtime; by
  default the fallback is bounded to 900 seconds and never exceeds the overall run_task timeout
- Claude fallback recovery mode now prioritizes a useful in-channel reply over rebuilding large
  multi-file deliverables from scratch, and it reuses `.codex_remote_output.log` plus
  `.run_task_trace_codex_primary/` when the primary run already gathered evidence
- recovered runs surface a `recovery_note` in `RunTaskOutput` and write
  `.run_task_trace/logs/recovery_note.txt` for debugging

## Execution Backend

Control via `RUN_TASK_EXECUTION_BACKEND=local|azure_aci|auto`.

`auto` behavior:
- `DEPLOY_TARGET=staging|production` -> Azure ACI
- otherwise -> local

Safety rule in code:
- local codex execution is blocked when `DEPLOY_TARGET` is `staging` or `production`.

Optional dockerized local path:
- `RUN_TASK_USE_DOCKER=1`
- `RUN_TASK_DOCKER_IMAGE=<image>`
- optional `RUN_TASK_DOCKER_REQUIRED=1`

## Required Env

Minimum practical requirement:
- `AZURE_OPENAI_API_KEY_BACKUP`
- `AZURE_OPENAI_ENDPOINT_BACKUP` (required for `codex` runner; for example `https://<resource>.openai.azure.com/`)

Common optional controls:
- `CODEX_MODEL`, `CLAUDE_MODEL`
- `RUN_TASK_TIMEOUT_SECS`
- optional `RUN_TASK_CODEX_TIMEOUT_SECS=<seconds>` to cap the primary Codex runtime
- `CODEX_SANDBOX_MODE`, `CODEX_BYPASS_SANDBOX`
- Codex-specific failures automatically retry with the Claude runner, including warm-pool
  executions
- optional `RUN_TASK_CODEX_FALLBACK_CLAUDE_MODEL=<model>` to force the Claude model used by that fallback
- Claude fallback runs allow the built-in `WebSearch`, `WebFetch`, and `TodoWrite` tools in
  addition to the existing file-editing / shell toolset
- Bright Data social scraping:
  - `BRIGHT_DATA_API_KEY`
  - optional `BRIGHT_DATA_XIAOHONGSHU_COLLECTOR`
  - optional `BRIGHT_DATA_XIAOHONGSHU_TRIGGER_URL`
  - run_task forwards these vars into docker and Azure ACI Codex executions so
    shared Bright Data skills can authenticate inside remote task containers
- Google Workspace CLI (`gws`) auth:
  - preferred: `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE`
  - or components: `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID`,
    `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET`,
    `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN`,
    optional `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE`
  - when using component keys, run_task writes
    `.secrets/google_workspace_cli_credentials.json` inside each workspace and
    injects `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE` for local/docker/Azure ACI runs

## Example Usage

```rust
use run_task_module::{run_task, RunTaskParams};
use std::path::PathBuf;

let params = RunTaskParams {
    workspace_dir: PathBuf::from("/path/to/workspace"),
    input_email_dir: PathBuf::from("incoming_email"),
    input_attachments_dir: PathBuf::from("incoming_attachments"),
    memory_dir: PathBuf::from("memory"),
    reference_dir: PathBuf::from("references"),
    reply_to: vec!["user@example.com".to_string()],
    model_name: "gpt-5.4".to_string(),
    runner: "codex".to_string(),
    codex_disabled: false,
    channel: "email".to_string(),
    google_access_token: None,
    has_unified_account: true,
    user_identities: Default::default(),
    thread_epoch: None,
    thread_state_path: None,
};

let out = run_task(&params)?;
println!("reply file: {}", out.reply_html_path.display());
```

## Tests

```bash
cd DoWhiz_service
cargo test -p run_task_module
```

See also:
- `DoWhiz_service/run_task_module/tests/README.md`

When the fallback fires from either the direct Codex path or the warm-pool path:
- the primary Codex trace is preserved under `.run_task_trace_codex_primary/`
- the final Claude attempt writes the active `.run_task_trace/`
- a short handoff note is written to `.run_task_trace/recovery/codex_to_claude_fallback.txt`
