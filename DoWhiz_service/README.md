# DoWhiz_service

Rust backend for DoWhiz digital employees.

This service layer currently runs as:
- `inbound_gateway`: ingress/webhook router + dedupe + raw payload storage + queue enqueue
- `rust_service`: queue consumer + scheduler + task execution + outbound replies

## Table of Contents

- [1) Architecture](#1-architecture)
- [2) Components and Binaries](#2-components-and-binaries)
- [3) Config Files](#3-config-files)
- [4) Environment Variables](#4-environment-variables)
- [5) Local Run Workflows](#5-local-run-workflows)
- [6) Staging / Production Deployment](#6-staging--production-deployment)
- [7) Testing](#7-testing)
- [8) Runtime State and Data Stores](#8-runtime-state-and-data-stores)
- [9) Troubleshooting](#9-troubleshooting)

## 1) Architecture

### 1.1 Runtime boundary

- Gateway handles inbound HTTP/webhook and Discord gateway ingress.
- Worker does **not** host inbound webhook routes; it consumes ingestion queue messages.
- Both gateway and worker expose account/auth routes (`/auth/*`) and agent market routes.
- Billing routes (`/billing/*`) are mounted on worker only when Stripe config exists.

### 1.2 End-to-end flow

```text
Inbound (email/slack/discord/sms/telegram/whatsapp/google workspace/bluebubbles)
  -> inbound_gateway
  -> build route + dedupe key + raw payload ref
  -> ingestion queue
  -> rust_service worker claim_next(employee_id)
  -> process channel-specific inbound
  -> enqueue RunTask
  -> run_task_module (codex/claude)
  -> SendReply task(s) + optional follow-ups
  -> outbound channel adapter
```

### 1.3 Queue and storage behavior

- Ingestion queue backend resolver defaults to `postgres`.
- `inbound_gateway` enforces `INGESTION_QUEUE_BACKEND=servicebus` (or alias equivalent).
- Raw payload storage defaults to Supabase; Azure Blob backend is recommended for gateway production.
- Scheduler/user/index state is Mongo-backed.

### 1.4 Startup workspace product layer

Startup workspace orchestration lives in `scheduler_module` (not `run_task_module`) and is split into:
- `scheduler_module/src/domain/*`: canonical blueprint/resource/task/agent/artifact models
- `scheduler_module/src/service/startup_workspace/*`: intake normalization, bootstrap planning, resource/provisioning mapping, provider runtime-state snapshots
- `scheduler_module/src/service/workspace.rs`: bootstrap artifact persistence under each workspace (`startup_workspace/*.json`)

Runtime provider visibility endpoint:
- `GET /api/workspace/provider-state` (implemented in `scheduler_module/src/service/auth.rs`)

## 2) Components and Binaries

Cargo workspace members:
- `scheduler_module`
- `run_task_module`
- `send_emails_module`

Key binaries (from `scheduler_module/src/bin`):

| Binary | Purpose |
|---|---|
| `inbound_gateway` | Main ingress gateway (webhooks + queue enqueue) |
| `rust_service` | Worker service (queue consumer + scheduler + auth routes) |
| `set_postmark_inbound_hook` | Utility to update Postmark inbound webhook |
| `inbound_fanout` | Legacy fanout ingress helper |
| `google-docs` / `google-sheets` / `google-slides` | Workspace integration CLI tools |
| `browserbase_session_manager` | Helper CLI that creates/reuses Browserbase contexts and active sessions for browser tasks |
| `human_approval_gate` / `human_approval_gate_mcp` | Human approval gate for CAPTCHA/password/2FA blockers; CLI for manual use and MCP server for blocking Codex runs |

Key scripts:

| Script | Purpose |
|---|---|
| `scripts/run_gateway_local.sh` | Start `inbound_gateway` |
| `scripts/run_employee.sh` | Start `rust_service` for one employee (uses configured public hook URL; ngrok optional for local only) |
| `scripts/start_all.sh` | Local-only stack bootstrap (gateway + worker + ngrok + hook) |
| `scripts/run_e2e.sh` | Live email E2E harness (uses `POSTMARK_TEST_HOOK_URL`/`POSTMARK_INBOUND_HOOK_URL` when available) |
| `scripts/ensure_aci_share_mount.sh` | Validate/mount Azure Files for ACI backend |

## 3) Config Files

### 3.1 Employee config

Default path resolution:
- `EMPLOYEE_CONFIG_PATH` if set
- otherwise `DoWhiz_service/employee.toml`

Primary files:
- `employee.toml` (production/default)
- `employee.staging.toml` (staging profile)

Each employee can define:
- `id`, `display_name`, `runner` (`codex` / `claude`), `model`
- `addresses` (first address is default outbound from)
- optional `runtime_root`
- optional `agents_path`, `claude_path`, `soul_path`, `skills_dir`
- channel toggles: `discord_enabled`, `slack_enabled`, `bluebubbles_enabled`

When `skills_dir` is set, the shared skill directories under that path are copied into
each task workspace at `.agents/skills/`, so new shared skills can be added without
changing run_task runtime code.

Current default:
- Built-in employees currently point `skills_dir` at `DoWhiz_service/skills`.
- `DoWhiz_service/skills/manifest.toml` is the maintained index of runtime-copyable skill directories.
- Placeholder folders like `DoWhiz_service/employees/<id>/skills/` are inactive unless `skills_dir` is explicitly pointed at them.

### 3.2 Gateway config

Default path resolution:
- `GATEWAY_CONFIG_PATH` if set
- otherwise `DoWhiz_service/gateway.toml`

Files:
- `gateway.toml`
- `gateway.staging.toml`
- `gateway.example.toml`

Route model (`channel + key -> employee_id + tenant_id`):
- exact key match has highest priority
- `key = "*"` acts as channel default
- email fallback can route by service address from `employee.toml`
- global defaults (`[defaults]`) are fallback of last resort

Notes:
- Discord message routing uses bot-token-to-employee mapping for selected client; route table is mainly used to enable channel defaults/tenant defaults.
- Discord inbound requests prepare a transient `discord_context/` folder inside the task workspace with thread context plus a large recent channel-history window for agent summarization; this context is not persisted outside the workspace.
- Discord inbound attachment URLs are preserved in archived raw payloads, and current-message files are downloaded into `incoming_attachments/` before the task runs.

## 4) Environment Variables

Copy base template:

```bash
cp .env.example DoWhiz_service/.env
```

### 4.1 Runtime policy

- Runtime `.env` should use **unprefixed** keys.
- `DEPLOY_TARGET` is optional (`production`/`staging`/others) and affects runtime policy decisions.
- Some ingestion/storage paths support `SCALE_OLIVER_*` fallback aliases; keep unprefixed keys authoritative.

### 4.2 Required for typical gateway + worker flow

| Key | Why |
|---|---|
| `MONGODB_URI` | Scheduler/user/index persistence |
| `SUPABASE_DB_URL` (or `SUPABASE_POOLER_URL` fallback in some paths) | Account/auth/billing store |
| `AZURE_OPENAI_API_KEY_BACKUP` | Required by Codex/Claude task execution |
| `AZURE_OPENAI_ENDPOINT_BACKUP` | Required by Codex task execution (Azure OpenAI endpoint) |
| `POSTMARK_SERVER_TOKEN` | Email outbound and webhook utility |
| `INGESTION_QUEUE_BACKEND=servicebus` | Required by gateway |
| `SERVICE_BUS_CONNECTION_STRING` **or** `SERVICE_BUS_NAMESPACE` + `SERVICE_BUS_POLICY_NAME` + `SERVICE_BUS_POLICY_KEY` | Service Bus queue auth |
| `SERVICE_BUS_QUEUE_NAME` | Service Bus queue target |

### 4.3 Raw payload storage backend

Default backend is Supabase. Recommended gateway production backend is Azure.

If using Supabase raw payload storage:
- `SUPABASE_PROJECT_URL`
- `SUPABASE_SECRET_KEY`
- optional `SUPABASE_STORAGE_BUCKET` (default `ingestion-raw`)

If using Azure raw payload storage (`RAW_PAYLOAD_STORAGE_BACKEND=azure`):
- `AZURE_STORAGE_CONTAINER_INGEST`
- one auth option:
  - `AZURE_STORAGE_CONTAINER_SAS_URL`, or
  - `AZURE_STORAGE_ACCOUNT` + `AZURE_STORAGE_SAS_TOKEN`, or
  - `AZURE_STORAGE_CONNECTION_STRING_INGEST`/`AZURE_STORAGE_CONNECTION_STRING`

Task debug archive bundles reuse the same Azure auth chain by default. If you want a dedicated
archive container instead of sharing the raw-ingest container, set:
- `TASK_DEBUG_ARCHIVE_ENABLED=1` (default enabled)
- `AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES=<container-name>` (default `task-debug-archives`)
- optional dedicated auth:
  - `AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES_SAS_URL`, or
  - reuse `AZURE_STORAGE_ACCOUNT` + `AZURE_STORAGE_SAS_TOKEN`, or
  - reuse `AZURE_STORAGE_CONNECTION_STRING_INGEST`/`AZURE_STORAGE_CONNECTION_STRING`

Archive uploads try the configured auth candidates in that order until one succeeds. When an
upload lands in Azure, the Mongo row records the actual storage account used so historical debug
bundles can be traced back unambiguously even if multiple storage accounts are configured.

If Azure upload is unavailable or fails, the worker still writes the zip to a durable local
fallback path under `.task_debug_archives_failed/` near the workspace/archive root and records that
fallback path in Mongo.

For the operational lookup/download/debugging flow, see:
- `DoWhiz_service/docs/task_debug_archives.md`

### 4.4 RunTask backend controls

- `RUN_TASK_EXECUTION_BACKEND=local|azure_aci|auto`
- `auto` behavior:
  - `DEPLOY_TARGET in {staging,production}` -> Azure ACI
  - otherwise local
- `TASK_TIMEOUT_SECS` controls scheduler watchdog stale-task detection. If unset, the watchdog
  allows two full `RUN_TASK_TIMEOUT_SECS` windows plus 30 seconds of headroom so a timed-out
  Codex primary can still hand off to Claude fallback.
- `RUN_TASK_TIMEOUT_SECS` (optional) controls each runner command timeout for run_task (default:
  `36000`). Runtime caps it below the effective watchdog budget to avoid stale-task retry loops.
- `RUN_TASK_CODEX_TIMEOUT_SECS` (optional) caps primary Codex runtime before Claude fallback is
  considered; if unset, Codex keeps the overall `RUN_TASK_TIMEOUT_SECS` budget.

In staging/production targets, local codex execution is blocked unless you explicitly avoid that policy.

Docker execution path (local worker):
- `RUN_TASK_USE_DOCKER=1`
- `RUN_TASK_DOCKER_IMAGE=<image>`
- optional `RUN_TASK_DOCKER_REQUIRED=1`
- Codex-specific failures automatically retry with the Claude runner, including warm-pool
  executions
- Azure ACI timeout errors (`az container create` / `az container show`) are treated as
  fallback-eligible primary Codex failures, so long-running or stuck remote Codex attempts can
  hand off to Claude
- optional `RUN_TASK_CODEX_FALLBACK_CLAUDE_MODEL=<model>` to force the Claude model used by that fallback
- optional `RUN_TASK_CODEX_FALLBACK_TIMEOUT_SECS=<seconds>` to cap Claude fallback runtime; if
  unset, the fallback keeps the overall `RUN_TASK_TIMEOUT_SECS` budget
- Claude fallback recovery mode now prioritizes writing a useful in-channel reply before starting
  new PDFs or other large deliverables, and it reuses `.codex_remote_output.log` plus
  `.run_task_trace_codex_primary/` when the primary Codex attempt already gathered evidence
- Codex success still requires the expected reply artifact to be present and non-empty; warm-pool
  completion queue exit codes are validated before the scheduler records success
- when scheduler warm-pool mode is enabled, a warm-pool failure automatically retries through the
  normal direct `run_task` path; that retry can then use the Codex -> Claude fallback above

Azure ACI execution path (required vars):
- `RUN_TASK_AZURE_ACI_RESOURCE_GROUP`
- `RUN_TASK_AZURE_ACI_IMAGE`
- `RUN_TASK_AZURE_ACI_HOST_SHARE_ROOT`
- `RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT`
- `RUN_TASK_AZURE_ACI_STORAGE_KEY`
- optional: location/registry/cpu/memory/share/container root vars

### 4.5 Channel-specific integrations (optional)

- Slack: `SLACK_*`, `SLACK_SIGNING_SECRET`
- Discord: `DISCORD_*` and/or employee-specific Discord token envs
- Telegram: `TELEGRAM_BOT_TOKEN` or employee-derived env keys
- WhatsApp: `WHATSAPP_ACCESS_TOKEN`, `WHATSAPP_PHONE_NUMBER_ID`, `WHATSAPP_VERIFY_TOKEN`
- WeChat Work: `WECHAT_CORP_ID`, `WECHAT_CORP_SECRET`, `WECHAT_AGENT_ID`, `WECHAT_TOKEN`, `WECHAT_ENCODING_AES_KEY`
- Twilio SMS: `TWILIO_*`
- Google Workspace: `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, refresh tokens, `GOOGLE_*_ENABLED`
- Google Workspace CLI (`gws`):
  `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE` (preferred) or
  `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID`,
  `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET`,
  `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN`,
  optional `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE` (default `authorized_user`).
  If component keys are set, run_task materializes
  `.secrets/google_workspace_cli_credentials.json` in each workspace and injects
  `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE` for local/docker/Azure ACI execution.
- Bright Data social scraping:
  `BRIGHT_DATA_API_KEY` for shared auth, plus optional
  `BRIGHT_DATA_XIAOHONGSHU_COLLECTOR` or
  `BRIGHT_DATA_XIAOHONGSHU_TRIGGER_URL` when Bright Data Scraper Studio has already
  provisioned a Xiaohongshu / RedNote custom scraper. Shared workspace skill:
  `DoWhiz_service/skills/bright-data-social`. run_task forwards these keys into
  local, docker, and Azure ACI task environments so the shared skill can
  authenticate inside real worker containers.
- Google Drive push: `GOOGLE_DRIVE_PUSH_ENABLED`, `GOOGLE_DRIVE_WEBHOOK_URL`
- Browser-based web auth for private Notion/Google pages is agent-driven at task runtime
  (no service-side bootstrap step).
- Browserbase-backed browser persistence (optional):
  `BROWSERBASE_API_KEY`, `BROWSERBASE_PROJECT_ID`, optional
  `BROWSERBASE_API_BASE_URL`, optional `BROWSERBASE_SESSION_TIMEOUT_SECONDS`.
  When configured, run_task forwards these env vars into local, docker, and Azure ACI
  task environments. The bundled `playwright-cli` wrapper then calls
  `browserbase_session_manager` to create or reuse a persistent Browserbase Context,
  storing durable per-user auth state in `.secrets/browserbase/registry.json` and the
  currently live task session in `.secrets/browserbase/active_session.json`. `scheduler_module`
  only mirrors the durable Browserbase context state between per-user secrets and each
  task workspace; it does not persist `active_session.json` across tasks, so every new
  task restores the user's auth context into a fresh live Browserbase session instead of
  inheriting a stale expiring session from an older container. If Browserbase rejects
  session creation with HTTP 402, `browserbase_session_manager` now surfaces an
  explicit quota/billing message so operators know why no live browser or handoff
  link could be created.
- `human_approval_gate` (via skill `human-approval-gate`) provides a blocking
  approval flow for login CAPTCHA/password/OTP/device-approval steps. In
  run_task/Codex environments, the preferred path is the injected MCP tool
  `dowhiz_human_approval_gate_request_and_wait`, which sends the email with the
  current browser screenshot(s) and blocks the same Codex turn until the first
  same-thread reply or timeout, preserving the current browser session while it
  waits. run_task injects `tool_timeout_sec = 1860` for that MCP server so the
  Codex-side tool call can remain blocked for the default 30-minute HAG wait
  window plus a small buffer. The blocker must be modeled as `captcha`, `password`, or
  `two_factor`. For `two_factor`, callers should only invoke it after the site
  is explicitly waiting for a code or device approval, and they should include
  the concrete method details (SMS/email/auth app/device tap). The manual CLI
  remains available outside Codex runtime, but run_task sets
  `HUMAN_APPROVAL_GATE_REQUIRE_MCP=1` so shell-side HAG calls are rejected and
  the blocking MCP path is enforced. Each send also writes
  `.human_approval_gate/events.jsonl` and emits a `HAG_EVENT ...` stderr line
  containing challenge type plus attachment filenames and sizes, so
  prod/staging task logs can prove exactly what was sent. Sender resolution
  priority is `--from` > `HUMAN_APPROVAL_FROM` > employee mailbox from employee
  config. When an active Browserbase session exists, the HAG email also includes a
  signed browser handoff link under the configured public service base (for example
  `/service/auth/browser-handoff`) so the human can open the same live Browserbase
  session, finish the blocker in-browser, and then reply in the email thread to
  resume the agent. When Browserbase keeps auxiliary blank tabs around, the
  handoff flow now prefers the most recent non-blank debuggable page instead of
  dropping the user into a session-level `about:blank` inspector. HAG-thread
  replies (`[HAG:...]`) are ignored by normal inbound
  task routing to prevent recursive Email->task loops. The HTML help email keeps
  the live-browser handoff button at the top when available and summarizes the
  blocker using short `Blocked on` / `Help needed` copy so humans can scan it
  quickly.
- Browserbase handoff validation endpoints:
  `/service/browserbase-handoff-demo?run=<run_id>` is a DoWhiz-owned same-tab
  demo page for manual validation. It intentionally starts in a blocked state,
  stores its state in browser localStorage scoped by `run`, and can be completed
  in-place by the human so the resumed agent sees the exact same tab change.
- Recommended staging validation flow for Browserbase/HAG:
  1. Deterministic same-tab demo:
     send a task to `dowhiz@deep-tutor.com` instructing the agent to open
     `/service/browserbase-handoff-demo?run=<unique-id>`, stop at the blocked
     state, and request HAG help. Open the top button from the HAG email, verify
     the live page is the blocked demo tab, click the in-page completion button,
     then reply `done` in the HAG thread and confirm the agent resumes.
  2. Real Google admin staging flow:
     send a task to `dowhiz@deep-tutor.com` instructing the agent to sign into
     Google as `dowhiz@deep-tutor.com` and perform one harmless follow-up action
     after login. Let the agent use `GOOGLE_PASSWORD` if present, rely on HAG for
     any remaining 2FA / device approval / CAPTCHA blocker, complete the blocker
     through the live handoff page, reply `done`, and verify the agent finishes.
- Evidence to capture during Browserbase handoff validation:
  the HAG email showing the top live-browser button, the live handoff page in its
  blocked state, the same page after the human completes the unblock step, and
  the final DoWhiz reply proving the agent resumed.
- ACI run_task sets Playwright/NPM runtime defaults for mounted workspaces:
  `PLAYWRIGHT_MCP_EXECUTABLE_PATH` auto-discovery (`chrome-linux` / `chrome-linux64`),
  `PLAYWRIGHT_BROWSERS_PATH=/app/.cache/ms-playwright`,
  and `NPM_CONFIG_CACHE=/tmp/.npm` to avoid symlink failures from `npx`.

### 4.6 Billing / insufficient-balance notices

- Stripe billing routes are enabled only when both keys are present:
  - `STRIPE_SECRET_KEY`
  - `STRIPE_WEBHOOK_SECRET`
- Optional fixed payment link for insufficient-balance auto notices:
  - `INSUFFICIENT_BALANCE_PAYMENT_LINK` (preferred)
  - fallback order: `BILLING_PAYMENT_LINK` -> `PAYMENT_LINK` -> `${FRONTEND_URL}/auth/index.html` -> `https://www.dowhiz.com/auth/index.html`
- Insufficient-balance notices bypass agent execution and are sent directly by channel adapter (email HTML / other channels plain text).

## 5) Local Run Workflows

### 5.1 Fast path: one worker + one gateway

From repo root:

```bash
# worker
./DoWhiz_service/scripts/run_employee.sh little_bear 9001 --skip-hook --skip-ngrok

# gateway (new terminal)
./DoWhiz_service/scripts/run_gateway_local.sh
```

Optional local public webhook:

```bash
ngrok http 9100
cd DoWhiz_service
cargo run -p scheduler_module --bin set_postmark_inbound_hook -- \
  --hook-url https://YOUR-DOMAIN.ngrok.app/postmark/inbound
```

### 5.2 Manual multi-employee worker setup

```bash
cd DoWhiz_service

EMPLOYEE_ID=little_bear RUST_SERVICE_PORT=9001 \
  cargo run -p scheduler_module --bin rust_service -- --host 0.0.0.0 --port 9001

EMPLOYEE_ID=mini_mouse RUST_SERVICE_PORT=9002 \
  cargo run -p scheduler_module --bin rust_service -- --host 0.0.0.0 --port 9002

EMPLOYEE_ID=sticky_octopus RUST_SERVICE_PORT=9003 \
  cargo run -p scheduler_module --bin rust_service -- --host 0.0.0.0 --port 9003

EMPLOYEE_ID=boiled_egg RUST_SERVICE_PORT=9004 \
  cargo run -p scheduler_module --bin rust_service -- --host 0.0.0.0 --port 9004
```

Then run gateway using configured routes in `gateway.toml`.

### 5.3 Legacy fanout ingress

`inbound_fanout` is still available for legacy fanout testing:

```bash
./DoWhiz_service/scripts/run_fanout_local.sh
```

Preferred ingress path remains `inbound_gateway`.

## 6) Staging / Production Deployment

Deployment branch policy:
- staging VM deploys from `dev` via automatic pushes to `dev`
- production VM deploys from `main`

Runtime env policy:
- VM runtime file is `DoWhiz_service/.env` with unprefixed keys.
- CI/CD merges `ENV_COMMON + ENV_STAGING` (staging) or `ENV_COMMON + ENV_PROD` (production).

Expected config selections:
- staging: `GATEWAY_CONFIG_PATH=gateway.staging.toml`, `EMPLOYEE_CONFIG_PATH=employee.staging.toml`
- production: `GATEWAY_CONFIG_PATH=gateway.toml`, `EMPLOYEE_CONFIG_PATH=employee.toml`
- staging expected worker identity: `boiled_egg`
- production expected worker identity: `little_bear`
- on staging/production VMs, use existing public webhook endpoint (`POSTMARK_INBOUND_HOOK_URL`) and do not run ngrok

Use these runbooks:
- `DoWhiz_service/OPERATIONS.md`
- `DoWhiz_service/docs/staging_production_deploy.md`

## 7) Testing

### 7.1 Core test commands

```bash
cd DoWhiz_service
cargo test -p run_task_module
cargo test -p send_emails_module
cargo test -p scheduler_module
```

Module-targeted examples:

```bash
cargo test -p scheduler_module --test scheduler_basic
cargo test -p scheduler_module --test send_reply_outbound_e2e
cargo test -p scheduler_module --test service_real_email -- --nocapture
```

Notes:
- `cargo test -p scheduler_module --test scheduler_basic` currently opens the Mongo-backed scheduler store, so `MONGODB_URI` must be set. If local Mongo is unavailable, mark it `SKIP` in verification notes with the blocker.

### 7.2 Live E2E

Full email E2E helper script:

```bash
./DoWhiz_service/scripts/run_e2e.sh
```

On staging/production, prefer configured public hook URL via `POSTMARK_TEST_HOOK_URL` or `POSTMARK_INBOUND_HOOK_URL` (or pass `--public-url`) and keep ngrok disabled.

Manual live run example:

```bash
RUN_CODEX_E2E=1 POSTMARK_LIVE_TEST=1 \
  cargo test -p scheduler_module --test service_real_email -- --nocapture
```

Optional live-email overrides:
- `RUST_SERVICE_LIVE_EMAIL_BODY_FILE=/abs/path/prompt.txt` or `RUST_SERVICE_LIVE_EMAIL_BODY_TEXT=...`
  to send a specific inbound email body through the live harness instead of the default short test
  message

Canonical test checklist:
- `reference_documentation/test_plans/DoWhiz_service_tests.md`

## 8) Runtime State and Data Stores

Default runtime root:

```text
$HOME/.dowhiz/DoWhiz/run_task/<employee_id>/
```

Common directories:
- `state/` (scheduler/user/index scope keys and processed IDs)
- `users/<user_id>/memory`
- `users/<user_id>/mail`
- `users/<user_id>/workspaces/<thread_or_message>`

Data store split:
- MongoDB: task scheduler state, user/index data, several operational collections
- Supabase Postgres: account/auth/billing records
- Raw payload: Supabase storage or Azure Blob (by backend config)
- Queue: Service Bus (gateway flow) or Postgres (legacy/optional)

Task debug archival:
- Each `RunTask` execution writes a standardized `.run_task_trace/` directory inside the workspace
  with prompt, stdout/stderr/combined logs, assistant output tail, token usage, and Azure ACI
  metadata/logs when applicable.
- Scheduler finalization snapshots `workspace_before` and `workspace_after`, writes redacted
  manifests/diffs/runtime metadata, zips the bundle, uploads it to Azure Blob when configured, and
  records the lookup row in Mongo collection `task_debug_archives`.
- The archive lookup row records the actual storage account/container/blob path used for successful
  uploads, plus a precise `blob_reference`, so historical task bundles can be found without
  guessing which Azure account accepted the write.
- Sensitive files are not copied into the archive payload. `.env*`, `.secrets/`, `.auth/`,
  credential JSON, and private key-like files are recorded as redacted manifest entries instead.
- The archive record also stores `archive_build_duration_ms` and `upload_duration_ms` so staging
  and production runs can be checked for overhead.
- Operational commands for finding a task, downloading its bundle, and inspecting captured logs are
  documented in `DoWhiz_service/docs/task_debug_archives.md`.

## 9) Troubleshooting

### Gateway exits immediately with backend error

Symptom:
- `inbound gateway requires ... INGESTION_QUEUE_BACKEND=servicebus`

Fix:
- set `INGESTION_QUEUE_BACKEND=servicebus`
- set either `SERVICE_BUS_CONNECTION_STRING`
  or `SERVICE_BUS_NAMESPACE` + `SERVICE_BUS_POLICY_NAME` + `SERVICE_BUS_POLICY_KEY`
- set `SERVICE_BUS_QUEUE_NAME`

### Gateway enqueue works but worker does not process

Check:
- same Service Bus credentials and `SERVICE_BUS_QUEUE_NAME` in worker env
- worker `EMPLOYEE_ID` matches routed employee
- worker logs for `claim_next`/processing errors

### Raw payload store upload/download failures

Check:
- backend selection `RAW_PAYLOAD_STORAGE_BACKEND`
- matching credentials for selected backend
- container/bucket names and permissions

### Local run_task blocked in staging/production target

Symptom:
- local execution forbidden error

Fix:
- use `RUN_TASK_EXECUTION_BACKEND=azure_aci` with required ACI env,
  or run with non-staging/production `DEPLOY_TARGET` for local dev.

### Azure ACI backend fails before execution

Check:
- Azure Files share mounted at `RUN_TASK_AZURE_ACI_HOST_SHARE_ROOT`
- run `scripts/ensure_aci_share_mount.sh`
- verify ACI resource group/image/storage credentials
