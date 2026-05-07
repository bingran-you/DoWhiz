# Documentation Drift Review (2026-05-07)

This review compares the requested Google Doc summary against the live repository.

The goal is not to preserve the older wording. The goal is to identify what is still true, what is
now stale, and what remains risky for maintainers or agents reading the repo.

## Summary

The Google Doc captures the rough product idea correctly: DoWhiz prepares a workspace, invokes an
AI CLI, parses outputs, and sends a reply.

But the implementation has outgrown that document in several important ways:

- the runtime is now gateway plus worker, not one scheduler service
- task/index state is Mongo-backed, not SQLite-backed
- queue handling is abstracted and commonly Service Bus in production
- the prompt contract is channel-aware and much larger than “email + memory + SOUL/AGENTS”
- the backend has been partially modularized, but a few large files still concentrate too much
  behavior

## Findings

### 1. High: The imported source doc is no longer a safe architecture map

The older doc sends readers to the wrong files and the wrong runtime model.

| Source doc claim | Current reality |
|---|---|
| Scheduler receives `/postmark/inbound` directly | `/postmark/inbound` lives in `inbound_gateway` |
| Tasks live in SQLite with `next_run` polling | scheduler/user/index state is Mongo-backed |
| `service.rs` and `lib.rs` are the runtime centers | the live flow is split across `service/server.rs`, `service/ingestion.rs`, `service/scheduler.rs`, `service/workspace.rs`, and `scheduler/*` |
| `run_task.rs` is the runner map | the runner is split across `run_task/*` |
| reply contract is effectively email-only | replies are channel-specific and may route across channels |

Impact:

- a maintainer following the older doc will search the wrong boundaries first
- an agent using that doc as primary context will misunderstand ingress, persistence, and reply
  behavior

Action taken:

- added `docs/repo-map.md`
- added `docs/runtime-architecture.md`
- linked them from `README.md` and `docs/README.md`

### 2. Medium: A live repo doc claimed the gateway enforces Service Bus, but the code still accepts `postgres`

`DoWhiz_service/README.md` previously said the gateway enforces
`INGESTION_QUEUE_BACKEND=servicebus`.

The code in `scheduler_module/src/bin/inbound_gateway.rs` currently accepts:

- `servicebus`
- `service_bus`
- `postgres`

Impact:

- the README overstated production policy as an absolute runtime restriction
- local readers could assume `postgres` is unsupported when the binary still allows it

Action taken:

- corrected the backend wording in `DoWhiz_service/README.md`

### 3. Medium: Worker naming in code still said “email service” although the worker is multi-channel

The worker bootstrap/help text still used “Rust email service” language even though the worker now
handles multi-channel ingestion consumption, scheduling, auth/product routes, and outbound work.

Impact:

- small but real design drift in developer-facing logs and CLI help
- reinforces the outdated email-only mental model

Action taken:

- renamed those strings to “DoWhiz worker service”

### 4. Medium: The codebase is more modular than the old doc implies, but several hot files still violate the repo’s own maintainability preference

The repo guidance says to keep files modular and split large files.

Current hotspots:

- `DoWhiz_service/run_task_module/src/run_task/codex.rs` (~7.2k)
- `DoWhiz_service/run_task_module/src/run_task/prompt.rs` (~2.6k)
- `DoWhiz_service/scheduler_module/src/service/scheduler.rs` (~1.7k)
- `DoWhiz_service/scheduler_module/src/service/workspace.rs` (~1.0k)

Impact:

- these files are the biggest sources of future doc drift
- they make it harder for Codex or human maintainers to isolate responsibilities cleanly

Action taken:

- documented these hotspots explicitly so future refactors have a starting point

Remaining recommendation:

- split `codex.rs` by execution path / recovery / artifact handling / output contract
- split `prompt.rs` by channel guidance, capability sections, and identity/security injection
- split `service/scheduler.rs` by claim logic, execution dispatch, and stale reconciliation

## What The Older Doc Still Got Right

- workspace-based execution remains the correct core abstraction
- prompt guidance still pulls in `SOUL.md`, `AGENTS.md`, memory, and inbound context
- the system still relies on reply artifacts plus structured stdout actions
- Codex and Claude remain subprocess-backed runners

## Recommended Next Refactor Order

1. `run_task_module/src/run_task/codex.rs`
2. `run_task_module/src/run_task/prompt.rs`
3. `scheduler_module/src/service/scheduler.rs`
4. `scheduler_module/src/service/workspace.rs`
