# Open-Source Scope

This document defines the public boundary for the open-source DoWhiz repository.

## What Is Open-Sourced

The repository publicly includes:
- the React frontend in `website/`
- the Rust services in `DoWhiz_service/`
- runtime skill definitions in `DoWhiz_service/skills/`
- public contributor docs, CI, and development scripts

The source is available under the repository license, but source availability does not mean every operational path is packaged as a turnkey product.

## Support Tiers

We use the following terms in this repo:

- `Supported`: documented, intended for external use, and validated by the public contributor path or local demo path.
- `Best effort`: available in source, but may require vendor accounts, manual setup, or architectural context that is still evolving.
- `Out of scope`: present for historical or internal-operational reasons, but not part of the maintained open-source contract.

## Self-Hosting Support Matrix

| Area | What You Can Expect | Third-Party Requirements | Support Tier |
|---|---|---|---|
| Website local demo | Run the public frontend locally and use the curated demo routes | Node.js only | Supported |
| Website development | Edit, lint, build, and test the frontend without private secrets | Node.js only | Supported |
| Rust code compilation and test execution | Build services, run selected public tests, and contribute fixes | Rust toolchain | Supported |
| Minimal service-side local demo routes | Run selected local/demo service paths that do not depend on private SaaS credentials | Local databases may be required for some routes | Best effort |
| Full autonomous task execution | Run the complete worker pipeline against real models and delivery channels | LLM credentials and channel credentials | Best effort |
| Email/webhook ingress and outbound delivery | Use Postmark-backed or similar live integrations | Postmark or equivalent provider access | Best effort |
| Slack, Discord, Google Workspace, Notion, Lark, WeChat, WhatsApp, SMS | Use integration adapters already in the codebase | Per-provider app credentials | Best effort |
| Staging/production Azure deployment flow | Reproduce the current internal branch-coupled deployment setup | Azure, service credentials, private environment knowledge | Out of scope |
| Internal billing/auth/account flows as currently operated | Mirror the production account stack exactly as run by the team | Supabase-compatible/Postgres-compatible setup plus team-specific config | Best effort |
| Historical product and operations notes | Inspect them for context | None | Out of scope for support |

## Third-Party Dependencies

The open-source repo includes integrations with third-party systems. Depending on what you want to run, you may need some or all of:

- MongoDB
- PostgreSQL or a Supabase-compatible Postgres deployment
- LLM credentials for task execution or routing
- Postmark for email ingress/egress
- Slack, Discord, Google Workspace, Notion, Lark, WeChat, WhatsApp, Twilio, or other provider credentials
- Azure services for the current production-like deployment path

We do not treat those vendor accounts as part of the public open-source contract.

## Out Of Scope

The following are intentionally out of scope for maintainer support:

- reproducing the internal staging or production environment exactly
- access to private secrets, internal mailboxes, internal cloud resources, or private datasets
- guaranteed support for every integration in every provider mode
- private troubleshooting over direct messages
- one-off custom deployment work for downstream users

## Best-Effort Areas

Some important parts of the codebase are useful but not yet fully productized for strangers:

- provider-specific auth flows
- branch-coupled deploy automation
- full self-hosted parity with the private production stack
- advanced task-execution backends and vendor fallbacks

When these areas block public onboarding, we prefer to document the limitation explicitly and narrow it over time instead of implying support that does not exist.

## What External Contributors Should Rely On

For a first contribution, rely on:

- the local frontend demo path
- public contributor docs
- public PR CI
- the docs under `docs/open-source/`

Treat anything else as opt-in exploration unless a public doc says it is supported.
