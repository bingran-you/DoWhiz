# Repo Map

This is the current maintainer-oriented map of the DoWhiz repository.

Use this when you need to understand the live code layout quickly, especially for Codex or other
repo-reading agents. It is intentionally more concrete than the public open-source docs and more
current than older internal notes.

## Trust Order

When docs disagree, use this order:

1. Code in `DoWhiz_service/` and `website/`
2. `README.md`, `DoWhiz_service/README.md`, and the docs listed in this file
3. `docs/open-source/*` for supported public workflows
4. `reference_documentation/` and older `docs/*.md` files as historical or secondary context
5. `external/` as reference-only material

## Top-Level Map

| Path | What it is | When to start there |
|---|---|---|
| `README.md` | Repo entry point and product positioning | First orientation |
| `docs/` | Curated docs, including public onboarding and current maintainer maps | Architecture and workflow reading |
| `DoWhiz_service/` | Rust backend: gateway, worker, scheduler, run-task integration, adapters, runtime skills | Any backend/runtime question |
| `website/` | React/Vite frontend and local demo routes | Web product, onboarding, dashboard, workspace UI |
| `reference_documentation/` | Historical notes, test plans, and internal research | Background context after you know the current shape |
| `external/` | Third-party or imported reference material | Read-only lookup; do not modify |

## If You Need To Debug X

| Topic | Start here | Then go deeper into |
|---|---|---|
| Inbound webhooks and routing | `DoWhiz_service/scheduler_module/src/bin/inbound_gateway.rs` | `DoWhiz_service/scheduler_module/src/bin/inbound_gateway/*`, `DoWhiz_service/scheduler_module/src/service/inbound/*` |
| Worker bootstrap and HTTP surface | `DoWhiz_service/scheduler_module/src/bin/rust_service.rs` | `DoWhiz_service/scheduler_module/src/service/server.rs` |
| Queue consumption | `DoWhiz_service/scheduler_module/src/service/ingestion.rs` | `DoWhiz_service/scheduler_module/src/ingestion_queue.rs`, `DoWhiz_service/scheduler_module/src/service_bus_queue.rs` |
| Due-task scheduling | `DoWhiz_service/scheduler_module/src/service/scheduler.rs` | `DoWhiz_service/scheduler_module/src/scheduler/*`, `DoWhiz_service/scheduler_module/src/index_store/mod.rs` |
| Workspace preparation | `DoWhiz_service/scheduler_module/src/service/workspace.rs` | `DoWhiz_service/scheduler_module/src/past_emails.rs`, `DoWhiz_service/scheduler_module/src/service/startup_workspace/*` |
| Prompt construction | `DoWhiz_service/run_task_module/src/run_task/prompt.rs` | `DoWhiz_service/run_task_module/src/run_task/core.rs` |
| Codex execution | `DoWhiz_service/run_task_module/src/run_task/codex.rs` | `DoWhiz_service/run_task_module/src/run_task/workspace.rs`, `DoWhiz_service/run_task_module/src/run_task/scheduled.rs` |
| Claude execution | `DoWhiz_service/run_task_module/src/run_task/claude.rs` | `DoWhiz_service/run_task_module/src/run_task/core.rs` |
| Reply dispatch | `DoWhiz_service/scheduler_module/src/scheduler/outbound.rs` | `DoWhiz_service/send_emails_module/src/send_emails.rs`, channel adapters under `DoWhiz_service/scheduler_module/src/adapters/` |
| Startup workspace product layer | `DoWhiz_service/scheduler_module/src/domain/*` | `DoWhiz_service/scheduler_module/src/service/startup_workspace/*`, `DoWhiz_service/scheduler_module/src/service/auth.rs` |

## Current Runtime Mental Model

The old single-service mental model is no longer accurate enough.

The current runtime is better understood as:

1. `inbound_gateway` receives channel events and webhooks.
2. The gateway normalizes them into ingestion envelopes.
3. The worker (`rust_service`) consumes queued envelopes.
4. Channel-specific inbound handlers update user/thread state and enqueue or synchronize tasks.
5. The scheduler scans due tasks from the task index and executes them with concurrency controls.
6. `run_task_module` prepares a thread workspace, builds the prompt, and invokes Codex or Claude.
7. The runner writes reply artifacts and optional scheduler actions back into the workspace/stdout.
8. The scheduler parses those outputs and sends outbound replies through channel adapters.

## Terminology Migration From The Older Internal Doc

| Older term | Current term / location |
|---|---|
| Single scheduler service receiving `/postmark/inbound` | Gateway/worker split; `/postmark/inbound` lives in `inbound_gateway` |
| SQLite task store | Mongo-backed scheduler/user/index state plus queue backend abstraction |
| Monolithic `service.rs` / `lib.rs` scheduler map | Split across `service/server.rs`, `service/ingestion.rs`, `service/scheduler.rs`, `service/workspace.rs`, and `scheduler/*` |
| Monolithic `run_task.rs` | `run_task/*` (`codex.rs`, `claude.rs`, `prompt.rs`, `core.rs`, etc.) |
| `SendEmail` task kind | `SendReply` in the live model, serialized as `send_email` for backward compatibility |
| Email-only reply contract | Channel-specific reply artifacts (`reply_email_draft.html` or `reply_message.txt`) |

## Recommended Reading Order

1. `README.md`
2. `docs/repo-map.md`
3. `docs/runtime-architecture.md`
4. `DoWhiz_service/README.md`
5. The specific module README under `DoWhiz_service/*/README.md`
6. Code in the file families listed above

## Areas Most Likely To Drift Again

These files still concentrate a lot of behavior and are the most likely places for docs to fall
behind unless they keep being split:

- `DoWhiz_service/run_task_module/src/run_task/codex.rs`
- `DoWhiz_service/run_task_module/src/run_task/prompt.rs`
- `DoWhiz_service/scheduler_module/src/service/scheduler.rs`
- `DoWhiz_service/scheduler_module/src/service/workspace.rs`
