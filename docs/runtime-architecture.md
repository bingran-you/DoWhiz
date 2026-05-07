# Runtime Architecture

This document is the current engineering view of how DoWhiz actually runs.

It is intentionally grounded in the live repository structure rather than the older internal doc
that described a smaller SQLite-based single-service version.

## End-To-End Flow

```text
Inbound channel event
  -> inbound_gateway
  -> ingestion envelope
  -> ingestion queue
  -> rust_service worker
  -> channel-specific inbound handler
  -> task index / scheduler state update
  -> due-task scheduler
  -> run_task_module (Codex / Claude)
  -> reply artifact + scheduler actions
  -> outbound adapter
```

## Concrete Source Of Truth By Layer

| Layer | Primary files |
|---|---|
| Gateway entrypoints | `DoWhiz_service/scheduler_module/src/bin/inbound_gateway.rs`, `DoWhiz_service/scheduler_module/src/bin/inbound_gateway/*` |
| Queue abstraction | `DoWhiz_service/scheduler_module/src/ingestion_queue.rs`, `DoWhiz_service/scheduler_module/src/service_bus_queue.rs` |
| Worker bootstrap | `DoWhiz_service/scheduler_module/src/bin/rust_service.rs`, `DoWhiz_service/scheduler_module/src/service/server.rs` |
| Queue consumer | `DoWhiz_service/scheduler_module/src/service/ingestion.rs` |
| Inbound channel handling | `DoWhiz_service/scheduler_module/src/service/inbound/*`, `DoWhiz_service/scheduler_module/src/service/email.rs` |
| Scheduler core | `DoWhiz_service/scheduler_module/src/service/scheduler.rs`, `DoWhiz_service/scheduler_module/src/scheduler/*` |
| Task/index persistence | `DoWhiz_service/scheduler_module/src/scheduler/store/mongo.rs`, `DoWhiz_service/scheduler_module/src/index_store/mod.rs`, `DoWhiz_service/scheduler_module/src/user_store/mod.rs` |
| Workspace preparation | `DoWhiz_service/scheduler_module/src/service/workspace.rs` |
| Prompt composition | `DoWhiz_service/run_task_module/src/run_task/prompt.rs` |
| Runner orchestration | `DoWhiz_service/run_task_module/src/run_task/core.rs` |
| Codex runner | `DoWhiz_service/run_task_module/src/run_task/codex.rs` |
| Claude runner | `DoWhiz_service/run_task_module/src/run_task/claude.rs` |
| Outbound replies | `DoWhiz_service/scheduler_module/src/scheduler/outbound.rs`, `DoWhiz_service/send_emails_module/src/send_emails.rs`, channel adapters |
| Startup workspace product layer | `DoWhiz_service/scheduler_module/src/domain/*`, `DoWhiz_service/scheduler_module/src/service/startup_workspace/*` |

## Gateway vs Worker

The most important architectural correction is that inbound handling is no longer centered inside
the worker.

- `inbound_gateway` owns channel webhook routes such as `/postmark/inbound`, Slack, Discord,
  WhatsApp, Notion, Google Drive, and Zoom ingress.
- `rust_service` is the worker. It consumes queued envelopes, runs the due-task scheduler, exposes
  auth/product routes, and executes task work.
- The worker logs this explicitly: inbound webhooks are handled by the gateway and the worker only
  consumes queued messages.

## Queue And State Backends

There are multiple storage layers now, and they matter:

| Concern | Current backend |
|---|---|
| Ingestion queue | Backend abstraction with `postgres` default resolver; Service Bus is the typical production gateway path |
| Scheduler store | Mongo |
| Task index | Mongo |
| User store | Mongo-backed APIs plus filesystem workspace materialization |
| Account/auth/billing data | Supabase-backed account store paths |
| Raw payload storage | Supabase by default; Azure Blob recommended for gateway production |

This means the old “tasks are stored in SQLite and polled directly” description is materially wrong
for current operations.

## Scheduler Model

The worker runs two related loops:

1. Ingestion consumer
   - Claims queued channel envelopes from the ingestion queue.
   - Dispatches to channel-specific handlers.
   - Marks the envelope done or failed.

2. Due-task scheduler
   - Polls the task index for due task refs.
   - Applies global and per-user concurrency limits.
   - Spawns execution threads for claimed tasks.
   - Reconciles stale executions and keeps task execution status in Mongo.

The queue consumer and the due-task scheduler are separate concerns now.

## Thread Workspace Contract

Before a runner executes, the worker prepares a per-thread workspace. The exact content varies by
channel, but the core contract includes:

- `incoming_email/` or equivalent inbound context
- `incoming_attachments/`
- `memory/`
- `references/`
- `.agents/skills/`
- `SOUL.md`
- `AGENTS.md`
- optional `CLAUDE.md`
- optional `startup_workspace/` bootstrap files
- reply output paths such as `reply_email_draft.html`, `reply_message.txt`,
  `reply_email_attachments/`, or `reply_attachments/`

Key facts:

- Workspaces are thread-scoped, not one-off task temp dirs.
- Past emails are hydrated into `references/past_emails/`.
- Skills are copied into `.agents/skills/`.
- Startup workspace artifacts are written under `startup_workspace/` by the scheduler layer, not by
  the frontend.

## Prompt Assembly

Prompt construction now does more than “SOUL + AGENTS + email”.

The prompt builder in `run_task_module/src/run_task/prompt.rs` composes:

- guidance blocks from `SOUL.md`, `AGENTS.md`, and optional runner-specific files
- memory context loaded from `memory/*.md`
- channel-specific reply instructions
- user identity and cross-channel routing context
- chat-history capabilities
- web auth and human approval gate guidance
- filesystem security limits
- investment-monitoring and recovery-mode guidance when relevant

That richer prompt contract is one reason a bare “email in, HTML reply out” mental model is no
longer sufficient.

## Reply Contract

Reply artifacts are channel dependent:

- email uses `reply_email_draft.html` plus `reply_email_attachments/`
- Slack/Discord/Telegram and others use `reply_message.txt`
- some channels may require `reply_routing.json` to route to a linked surface
- scheduler actions can still be emitted via structured stdout blocks

So “the runner always writes `reply_email_draft.html`” is no longer correct.

## Startup Workspace Product Boundary

The startup-workspace modeling layer lives in `scheduler_module`, not `run_task_module`.

That boundary is important:

- `scheduler_module/src/domain/*` defines the canonical blueprint/resource/task/artifact models
- `scheduler_module/src/service/startup_workspace/*` turns intake into bootstrap plans and provider
  state
- `scheduler_module/src/service/workspace.rs` persists the resulting `startup_workspace/*.json`
  artifacts
- `run_task_module` is execution/runtime infrastructure, not product modeling

## Current Hotspots

These files are still large enough to remain architecture and documentation risk:

| File | Approx. size |
|---|---|
| `DoWhiz_service/run_task_module/src/run_task/codex.rs` | 7.2k lines |
| `DoWhiz_service/run_task_module/src/run_task/prompt.rs` | 2.6k lines |
| `DoWhiz_service/scheduler_module/src/service/scheduler.rs` | 1.7k lines |
| `DoWhiz_service/scheduler_module/src/service/workspace.rs` | 1.0k lines |

Those files are still the main places where future design drift is likely to start.
