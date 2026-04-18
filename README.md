# DoWhiz

DoWhiz is an open-source foundation for building AI operators that work across email, documents, chat, and repository workflows.

This repository contains:
- `website/`: the public web app and local product demo routes
- `DoWhiz_service/`: Rust services for routing, scheduling, and task execution
- `DoWhiz_service/skills/`: runtime skills copied into task workspaces

## Start Here

If you are new to the repo, use this order:

1. Read [OPEN_SOURCE_SCOPE.md](OPEN_SOURCE_SCOPE.md) to understand what is and is not supported.
2. Run the fastest local demo:
   ```bash
   cd website
   npm ci
   npm run dev
   ```
   Then open `http://localhost:5173/demo/workspace`.
3. Follow [CONTRIBUTING.md](CONTRIBUTING.md) for the public contributor workflow.
4. Use [docs/README.md](docs/README.md) for deeper local-development, self-hosting, integration, and troubleshooting docs.

## Open-Source Scope

DoWhiz is now documented as an open-source project, but not every production path in the repository is packaged as turnkey self-hosting.

| Area | Status |
|---|---|
| Website local demo and contributor workflow | Supported |
| Rust service code, local development, and public CI | Supported |
| Self-hosting with local dependencies and selective integrations | Best effort |
| Internal staging/production deployment workflows and private cloud setup | Out of scope |

The full support matrix lives in [OPEN_SOURCE_SCOPE.md](OPEN_SOURCE_SCOPE.md).

## Repository Map

| Path | Purpose |
|---|---|
| `website/` | React 19 + Vite frontend |
| `DoWhiz_service/` | Rust backend binaries, scheduler, gateway, adapters, runtime skills |
| `docs/open-source/` | Public-facing development and self-hosting docs |
| `reference_documentation/` | Historical architecture notes, API references, and internal research material |
| `external/` | Reference-only third-party material; do not modify |

## Public Docs

- [docs/open-source/local-development.md](docs/open-source/local-development.md)
- [docs/open-source/self-hosting.md](docs/open-source/self-hosting.md)
- [docs/open-source/architecture.md](docs/open-source/architecture.md)
- [docs/open-source/integrations.md](docs/open-source/integrations.md)
- [docs/open-source/skills-and-plugins.md](docs/open-source/skills-and-plugins.md)
- [docs/open-source/troubleshooting.md](docs/open-source/troubleshooting.md)

## Contributing

External contributions are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md).

The public PR path is designed to work without private deployment secrets:
- website lint/build/tests
- Rust formatting, scoped public clippy checks, scheduler build validation, and selected public tests

## Support And Security

- Support expectations: [SUPPORT.md](SUPPORT.md)
- Security reporting: [SECURITY.md](SECURITY.md)
- Release process: [RELEASING.md](RELEASING.md)
- Community expectations: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)

## Internal/Historical Material

This repository still contains operational notes, product-history docs, and deployment workflows that were written for the original team. They are kept for transparency, but they are not the primary onboarding path for external developers.

Use the open-source docs first. Treat `reference_documentation/`, `DoWhiz_service/OPERATIONS.md`, and branch-coupled deploy workflows as secondary/internal context unless a public doc points you there.

Legacy maintainer helpers such as `env.example.least`, `cleanup/`, and `test_google_e2e.py`
remain in the repo for transparency, but they are not part of the supported first-time
onboarding path.
