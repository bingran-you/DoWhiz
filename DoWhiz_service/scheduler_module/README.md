# scheduler_module

Core orchestration module for DoWhiz backend.

Responsibilities:
- task scheduling (`cron` + `one-shot`)
- queue-consumer execution path in `rust_service`
- ingress path in `inbound_gateway`
- outbound delivery (`SendReply`) across channels
- user/task index integration with Mongo-backed stores
- startup workspace product-layer bootstrap planning (blueprint validation, resource mapping, starter agents/tasks/artifacts, provisioning snapshot)

## Task Model

`TaskKind`:
- `SendReply` (serialized as `send_email` for backward compatibility)
- `RunTask`
- `Noop`

Schedules:
- `Cron` (6 fields: `sec min hour day month weekday`, UTC)
  - Prefer named weekdays like `MON-FRI` for weekday schedules. Numeric weekday ranges are
    parser-ambiguous here; for example, `1-5` can behave like Sunday-Thursday.
- `OneShot` (`run_at` timestamp)

## Channels

Supported channel enum values:
- `email`, `slack`, `discord`, `sms`, `telegram`, `whatsapp`, `wechat`
- `google_docs`, `google_sheets`, `google_slides`
- `bluebubbles`

## Key Entry Points

- `src/bin/inbound_gateway.rs`
- `src/bin/rust_service.rs`
- `src/service/*`
- `src/scheduler/*`
- `src/ingestion_queue.rs`
- `src/domain/workspace_blueprint.rs`
- `src/domain/resource_model.rs`
- `src/domain/agent_roster.rs`
- `src/domain/starter_tasks.rs`
- `src/domain/artifact_queue.rs`
- `src/service/startup_workspace/*`
- `src/service/workspace.rs` (bootstrap artifact persistence under `startup_workspace/`)
- `src/service/auth.rs` (`GET /api/workspace/provider-state`)

## Startup Workspace Layer

The startup workspace layer is intentionally separated from run-task runner concerns.

Key boundaries:
- Product modeling and bootstrap policy are in `scheduler_module`:
  - `domain/*`: canonical blueprint/resource/task/roster/artifact schemas
  - `service/startup_workspace/*`: intake normalization + bootstrap orchestration + runtime provider-state snapshots
- Execution/runtime concerns remain in `run_task_module` (for example Codex/Claude execution and filesystem prep).

Bootstrap output artifacts are persisted into each workspace under:
- `startup_workspace/blueprint.json`
- `startup_workspace/resources.json`
- `startup_workspace/agent_roster.json`
- `startup_workspace/starter_tasks.json`
- `startup_workspace/artifact_queue.json`
- `startup_workspace/provisioning.json`
- `startup_workspace/workspace_home_snapshot.json`

## Test Commands

```bash
cd DoWhiz_service
cargo test -p scheduler_module
cargo test -p scheduler_module --test scheduler_basic
cargo test -p scheduler_module --test send_reply_outbound_e2e
cargo test -p scheduler_module startup_workspace::
```

Notes:
- `scheduler_basic` currently exercises the Mongo-backed scheduler store, so it requires `MONGODB_URI` in the local environment.
- If Mongo is unavailable, treat `scheduler_basic` as `SKIP` with blocker details in the verification report rather than assuming it is a pure no-infra smoke test.

Live tests and manual scripts are listed in:
- `reference_documentation/test_plans/DoWhiz_service_tests.md`

## Deployment Notes

For gateway/worker runtime and env policy, use:
- `DoWhiz_service/README.md`
- `DoWhiz_service/OPERATIONS.md`

## Install Onboarding

Slack/Discord install onboarding is a V1 activation flow that runs from bot-install success, not from generic account linking.
The auth dashboard separately shows a deterministic in-product setup card after generic connect success so users still see the next safe step without duplicating install tasks in Next Steps.
The current auth dashboard still uses a browser-local completion marker for the install CTA/badge until provider-state grows a durable Slack/Discord install snapshot.

Runtime flags:
- `OLIVER_SLACK_INSTALL_ONBOARDING_ENABLED`
- `OLIVER_DISCORD_INSTALL_ONBOARDING_ENABLED`

These flags are optional kill switches. If they are unset, install onboarding is enabled by default; set either flag to `0` / `false` to disable that platform quickly.

Operational behavior:
- onboarding state is stored per `account_id + platform + workspace_id` in the account database
- reconnecting Slack or Discord after a prior install will replay onboarding for the known installed workspace/server entries on that platform
- repeated installs/reconnects are allowed immediately; dedupe only suppresses re-processing of the same callback event
- unlinking Slack or Discord clears the browser-local install completion marker in the auth dashboard so reconnect returns to a fresh add-bot state
- at most one public onboarding post and one DM are attempted per callback event per workspace
- public and DM delivery outcomes are logged through analytics events such as `install_onboarding_public_sent`, `install_onboarding_dm_failed`, and `install_onboarding_skipped`

Manual resend path:
- authenticated API endpoint: `POST /api/channel-install-onboarding/resend`
- request body:

```json
{
  "platform": "slack",
  "workspace_id": "T123456",
  "force": false
}
```

- `force=false` respects normal same-event dedupe rules
- `force=true` intentionally bypasses normal same-event dedupe for support or QA
- resend requires an existing onboarding state row for that account/workspace

Known V1 limitations:
- Slack installer identity is not recovered directly from the bot-install callback; DM falls back to the linked account owner's verified Slack identifier when available
- Slack DM delivery uses a DM-open plus `chat.postMessage` path and safely fails when the workspace install cannot open or write that conversation
- Discord installer identity is not recovered directly from the bot-install callback; DM falls back to the linked account owner's verified Discord identifier when available
- if no reliable direct recipient exists, DM is skipped rather than guessed
