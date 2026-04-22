# DoWhiz

DoWhiz - Oliver, a trusted AI operator for work and life. 🧸

<p align="center"><strong>Product Shorts</strong></p>

<table align="center">
  <tr>
    <td align="center">
      <a href="https://www.youtube.com/shorts/SI9mxW_Top0">
        <img src="assets/readme-shorts/short-1.gif" alt="DoWhiz short 1 preview" width="170" />
      </a>
    </td>
    <td align="center">
      <a href="https://www.youtube.com/shorts/PSsJ7WBk71w">
        <img src="assets/readme-shorts/short-2.gif" alt="DoWhiz short 2 preview" width="170" />
      </a>
    </td>
    <td align="center">
      <a href="https://www.youtube.com/shorts/5H9g3LOGkMc">
        <img src="assets/readme-shorts/short-3.gif" alt="DoWhiz short 3 preview" width="170" />
      </a>
    </td>
  </tr>
  <tr>
    <td align="center"><sub><a href="https://www.youtube.com/shorts/SI9mxW_Top0">DoWhiz employee in Notion</a></sub></td>
    <td align="center"><sub><a href="https://www.youtube.com/shorts/PSsJ7WBk71w">DoWhiz employee in Email</a></sub></td>
    <td align="center"><sub><a href="https://www.youtube.com/shorts/5H9g3LOGkMc">DoWhiz employee in Discord</a></sub></td>
  </tr>
</table>

<p align="center"><sub>Tap any preview to watch the full Shorts video.</sub></p>

## Product Snapshot

DoWhiz is built around Oliver, a trusted AI operator that works in existing tools and brings back finished work.

Current product model:
- Oliver-first landing and onboarding for consumer adoption
- Personal setup dashboard for connected apps, tasks, memory, and settings
- Multi-channel execution across email, Slack/Discord, GitHub, Google Docs, and related surfaces
- Legacy startup/workspace routes remain available for backward compatibility, but they are no longer the primary product journey

Primary web routes:
- `/`: public landing
- `/auth/index.html`: Oliver setup dashboard
- `/demo/workspace`: supported open-source no-cloud demo
- `/start` and `/workspace`: legacy routes retained in the repo, but not the primary OSS onboarding path

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

Legacy maintainer helpers such as `env.example.least` and `test_google_e2e.py`
remain in the repo for transparency, but they are not part of the supported first-time
onboarding path.
