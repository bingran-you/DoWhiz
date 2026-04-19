# Local Development

This document focuses on development paths that do not require private deployment secrets.

## Fastest Supported Demo

If you only want a working local demo, start with the frontend route:

```bash
cd website
npm ci
npm run dev
```

Open `http://localhost:5173/demo/workspace`.

This route is the fastest supported open-source demo path and does not require MongoDB, Postgres, Azure, Supabase, or Postmark.

## Frontend Workflow

```bash
cd website
npm ci
npm run lint
npm run build
npm run test:auth-onboarding
```

## Rust Workflow

From `DoWhiz_service/`:

```bash
cargo fmt --all --check
cargo clippy --locked -p send_emails_module --all-targets --no-deps -- -D warnings
cargo clippy --locked -p run_task_module --tests -- -D warnings \
  -A clippy::too_many_arguments \
  -A clippy::io_other_error \
  -A clippy::manual_contains \
  -A clippy::useless_vec \
  -A clippy::unnecessary_sort_by
```

The public contributor path currently uses this scoped clippy baseline instead of
`cargo clippy --workspace --all-targets --all-features -- -D warnings` because the workspace
still contains inherited lint debt outside the warning-clean public path.

For public PR validation, also run the documented build and test steps:

```bash
cargo build --locked -p scheduler_module --bin rust_service --bin inbound_gateway
cargo test --locked -p run_task_module
cargo test --locked -p send_emails_module
cargo test --locked -p scheduler_module --test runtime_skills_manifest
```

Live Postmark tests in `send_emails_module` are ignored by default so public forks can run
`cargo test` without credentials. To run them intentionally:

```bash
POSTMARK_LIVE_TEST=1 cargo test -p send_emails_module -- --ignored --nocapture
```

## Optional Service-Side Demo

The repository also contains service-side demo surfaces, but they are not the fastest path and may require local databases.

For a minimal local service profile:

1. Start local databases:

   ```bash
   docker compose -f docker-compose.local.yml up -d
   ```

2. Copy the local env template:

   ```bash
   cp DoWhiz_service/.env.local.example DoWhiz_service/.env
   ```

3. Start the worker demo route:

   ```bash
   ./DoWhiz_service/scripts/run_local_demo.sh
   ```

4. Open `http://localhost:9001/browserbase-handoff-demo`.

5. Optional: start the local gateway profile in a second terminal:

   ```bash
   ./DoWhiz_service/scripts/run_local_gateway.sh
   ```

Supporting files:

- `DoWhiz_service/.env.local.example`
- `DoWhiz_service/gateway.local.toml`
- `DoWhiz_service/scripts/run_local_demo.sh`
- `DoWhiz_service/scripts/run_local_gateway.sh`
- `docker-compose.local.yml`

These files are intended to reduce coupling to private infrastructure, but they are still less turnkey than the frontend demo path.

## What Not To Expect From The Default Local Path

By default, the open-source local path does not assume:

- private Azure deployment resources
- internal Supabase projects
- internal Postmark mailboxes
- private staging/production secrets

Those paths are documented separately as best-effort or out-of-scope in [../../OPEN_SOURCE_SCOPE.md](../../OPEN_SOURCE_SCOPE.md).
