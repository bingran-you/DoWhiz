# Contributing to DoWhiz

Thanks for contributing.

This guide is written for external contributors who do not have access to the private DoWhiz deployment environment.

## Before You Start

Read these first:

1. [OPEN_SOURCE_SCOPE.md](OPEN_SOURCE_SCOPE.md)
2. [docs/open-source/local-development.md](docs/open-source/local-development.md)
3. [SUPPORT.md](SUPPORT.md)

Important repository rules:
- `external/` is reference-only. Do not modify it.
- Keep docs updated when behavior or setup changes.
- Do not assume private secrets, internal Azure resources, or internal mailboxes are available to contributors.

## Quick Contributor Workflow

The public, no-secrets workflow is:

1. Fork and clone the repo.
2. Create a branch from `dev`.
3. Run the local demo or the smallest relevant development path.
4. Make a focused change.
5. Run the relevant public checks.
6. Open a pull request against `dev`.

## Fastest Local Demo

The fastest supported demo path is frontend-only:

```bash
cd website
npm ci
npm run dev
```

Then open `http://localhost:5173/demo/workspace`.

Use this path when you want to understand the product shape or make website/UI changes without private infrastructure.

## Development Setup

### Frontend

```bash
cd website
npm ci
npm run dev
```

Useful commands:

```bash
npm run lint
npm run build
npm run test:auth-onboarding
```

### Rust Services

Rust contributors should start with:

```bash
cd DoWhiz_service
cargo fmt --all --check
cargo clippy --locked -p send_emails_module --all-targets --no-deps -- -D warnings
cargo clippy --locked -p run_task_module --tests -- -D warnings \
  -A clippy::too_many_arguments \
  -A clippy::io_other_error \
  -A clippy::manual_contains \
  -A clippy::useless_vec \
  -A clippy::unnecessary_sort_by
```

The public CI currently uses this narrower Rust lint baseline because `scheduler_module` and
parts of `run_task_module` still carry inherited warning debt. We document that limitation
explicitly instead of pretending full-workspace strict clippy is already green.

Publicly safe test commands are documented in:

- [docs/open-source/local-development.md](docs/open-source/local-development.md)
- [reference_documentation/test_plans/DoWhiz_service_tests.md](reference_documentation/test_plans/DoWhiz_service_tests.md)

If a test requires vendor credentials or live services, treat it as opt-in and call that out in your PR.

## What To Run Before Opening A PR

Run the checks that match your change:

- Website changes: `npm run lint`, `npm run build`, and relevant website tests
- Rust changes: the documented public Rust baseline in `docs/open-source/local-development.md`, plus the most relevant Rust tests for the code you changed
- Docs-only changes: verify links, commands, and file paths manually

If you could not run something important, say exactly why in the PR description.

## Pull Request Expectations

Open small, reviewable PRs.

Each PR should include:
- a short summary of the change
- why the change is needed
- exact validation commands run
- any skipped validation and the reason
- screenshots for UI changes when they help reviewers

Prefer one coherent change over a large mixed PR.

## Branching And Releases

- External contributions should normally target `dev`.
- Maintainers promote release-ready changes to `main`.
- Public release expectations are documented in [RELEASING.md](RELEASING.md).

## Documentation Changes

If you change onboarding, setup, scope, CI, or supported behavior, update the public docs in the same PR.

Use the open-source docs as the primary source of truth:

- `README.md`
- `OPEN_SOURCE_SCOPE.md`
- `docs/open-source/*`

## Reporting Problems

- Security issue: follow [SECURITY.md](SECURITY.md)
- Usage/support question: follow [SUPPORT.md](SUPPORT.md)
- Bug report or feature request: use the GitHub issue templates
