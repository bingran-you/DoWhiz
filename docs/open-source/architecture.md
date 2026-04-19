# Architecture

This page separates the public architecture story from the older internal operational notes.

## Repository Architecture

The repo has two primary product surfaces:

- `website/`: public web experience and local demo routes
- `DoWhiz_service/`: routing, scheduling, task execution, adapters, and runtime skills

## High-Level Runtime Shape

In the full product path, the codebase is organized around:

1. ingress
2. scheduling and task orchestration
3. task execution
4. outbound delivery

At a high level:

```text
Inbound event
  -> gateway / webhook route
  -> ingestion queue
  -> worker / scheduler
  -> task execution
  -> outbound adapter
```

## Open-Source-Friendly View

For public onboarding, you do not need to start from the full production flow.

The recommended order is:

1. frontend demo route
2. frontend contributor workflow
3. selective Rust development
4. deeper integration or self-hosting work only when needed

## Internal Production Notes

The repo still contains internal deployment and operational documentation, including Azure-specific flows and branch-coupled deployment workflows.

Those materials are still useful context, but they are not the primary architecture docs for open-source users.

If you need them, look at:

- `DoWhiz_service/OPERATIONS.md`
- `DoWhiz_service/docs/`
- `reference_documentation/`
