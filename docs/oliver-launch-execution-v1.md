# Oliver Launch Execution V1

- Status: implemented
- Last updated: 2026-04-24
- Audience: product, design, frontend, backend

## Summary

Oliver v1 is now a narrow launch execution copilot instead of a broad AI TPM surface.

The shipped promise is:

- Turn one messy launch thread into an accountable execution plan.
- Keep that plan refreshable with pasted updates.
- Produce a launch-readiness brief backed by visible evidence.

## Scope shipped

### Input

V1 supports one starting path only:

- pasted thread or pasted message bundle

This is intentional. We did not try to make Google Docs, email references, or broad multi-surface ingestion equally complete in this first slice.

### Output

The analyzer returns a `LaunchExecutionPlan` with:

- title
- objective
- source context summary
- target date or launch window
- milestones
- owners
- dependencies
- critical path candidates
- risks
- decisions
- evidence
- readiness status
- readiness reason
- follow-up items

The response also returns:

- tracker rows for owner/date/risk scanning
- a readiness brief with blockers, at-risk dependencies, open decisions, missing owner updates, and change summary

## Current product surface

### Frontend

- New route: `website/src/pages/OliverLaunchExecutionPage.jsx`
- New API client: `website/src/domain/launchExecutionApi.js`
- Demo thread seed: `website/src/data/launchExecutionDemo.js`
- New styling: `website/src/styles/launch-execution.css`

### Backend

- New analyzer module: `DoWhiz_service/scheduler_module/src/service/launch_execution.rs`
- New endpoint: `POST /api/launch-execution/analyze`
- Router wiring: `DoWhiz_service/scheduler_module/src/service/auth.rs`

## Readiness model

The readiness model is deliberately explicit in code.

### Green

- Evidence is present
- no critical blockers are open
- no launch-blocking decision is unresolved
- no critical-path item lacks ownership
- dates and follow-through are credible enough to support readiness

### Yellow

- Evidence exists, but there are still material follow-ups
- common causes include missing dates, stale updates, at-risk dependencies, or unresolved non-blocking issues

### Red

- No credible evidence was extracted
- or a high/critical blocker is still open
- or a launch-blocking decision is unresolved
- or a critical-path item lacks a clear owner

## Follow-through loop

We intentionally shipped the lightest real loop:

- user pastes initial launch context
- Oliver extracts a plan and brief
- user pastes the newest update
- Oliver refreshes the same plan and highlights what changed

The backend currently generates follow-up prompts for:

- missing owner
- missing date
- stale update
- unresolved blocker
- unresolved decision

We did not ship autonomous background outreach in v1.

## Why this matches the product thesis

This version keeps the strongest parts of Oliver:

- thread-native start
- continuity across updates
- follow-through into a structured execution layer

And it cuts the weak parts:

- no generic “works anywhere” positioning
- no broad task automation platform
- no attempt to become full PM software

## Intentional non-goals

Not built in this iteration:

- broad channel ingestion parity
- persistent launch-plan storage
- autonomous owner messaging
- generic workflow abstractions
- dashboards beyond the launch execution slice
- broad GitHub or engineering execution flows

## Next iteration

If this v1 proves useful, the most natural next steps are:

1. Persist `LaunchExecutionPlan` by thread so refreshes do not depend on browser state.
2. Add one more ingestion path, most likely email thread or Google Docs note.
3. Add approved follow-up send actions instead of draft-only messages.
4. Add evidence lineage to exact message timestamps or channel references.
5. Add a lightweight launch archive/history so “what changed” survives across sessions.
