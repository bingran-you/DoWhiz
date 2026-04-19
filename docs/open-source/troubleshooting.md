# Troubleshooting

## The frontend demo does not load

Check:

- you ran `npm ci` inside `website/`
- you are opening `http://localhost:5173/demo/workspace`
- your Node version is compatible with the repo

## Lint or build fails in `website/`

Try:

```bash
cd website
rm -rf node_modules
npm ci
npm run lint
npm run build
```

## Rust lint or tests fail

Start with:

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
cargo test --locked -p run_task_module
cargo test --locked -p send_emails_module
cargo test --locked -p scheduler_module --test runtime_skills_manifest
```

If `cargo build --locked -p scheduler_module --bin rust_service --bin inbound_gateway`
fails locally, check free disk space before assuming the code is broken. That build pulls in a
large dependency graph.

## I hit a path that wants Azure, Supabase, or Postmark

That usually means you have moved beyond the supported minimal open-source path.

Check:

1. [../../OPEN_SOURCE_SCOPE.md](../../OPEN_SOURCE_SCOPE.md)
2. [local-development.md](local-development.md)
3. [self-hosting.md](self-hosting.md)

If the dependency is unnecessary for the path you are trying to use, file a bug. If it is part of a best-effort integration path, document the requirement explicitly in your PR or issue.
