# Docs

This directory now has two different roles:

1. public open-source onboarding
2. current maintainer / Codex-oriented repo maps

## Start Here By Reader Type

### Public contributor

- [open-source/local-development.md](open-source/local-development.md)
- [open-source/self-hosting.md](open-source/self-hosting.md)
- [open-source/architecture.md](open-source/architecture.md)
- [open-source/integrations.md](open-source/integrations.md)
- [open-source/skills-and-plugins.md](open-source/skills-and-plugins.md)
- [open-source/troubleshooting.md](open-source/troubleshooting.md)
- [Architecture diagrams (Mermaid)](../reference_documentation/architecture/)

### Maintainer / Codex

- [repo-map.md](repo-map.md)
- [runtime-architecture.md](runtime-architecture.md)
- [documentation-drift-review-2026-05.md](documentation-drift-review-2026-05.md)
- [../DoWhiz_service/README.md](../DoWhiz_service/README.md)
- [Fault logs](../reference_documentation/fault_report.md)

## How To Read The Rest Of The Repo

Not all existing documentation was written at the same time or for the same audience.

Use this rough order:

1. code
2. `README.md`, `DoWhiz_service/README.md`, and the curated docs listed above
3. `docs/open-source/*` for supported public workflows
4. `reference_documentation/` and older `docs/*.md` files as more specific context.

Also note the following:

- `reference_documentation/`: specific notes on a particular part of the pipeline, such as `ephemeral_fileshares.md`, `aci_recovery.md`
- `DoWhiz_service/OPERATIONS.md`: internal operational runbook
- older product or experiment notes under `docs/*.md`
