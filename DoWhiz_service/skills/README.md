# Runtime Skills Catalog

`DoWhiz_service/skills` is the scheduler/runtime source of truth for shared skills copied into task workspaces.

Current behavior:
- Built-in employees in `DoWhiz_service/employee.toml` currently use `skills_dir = "skills"`.
- `scheduler_module/src/service/workspace.rs` copies only subdirectories from this root into `<workspace>/.agents/skills/`.
- Root-level metadata files in this directory are not copied into task workspaces.
- Repo-root `.agents/skills` and `.claude/skills` are local authoring/runtime catalogs for desktop agents; they are not the source used by `ensure_thread_workspace`.

Use `manifest.toml` in this folder as the maintained index of runtime-copyable skill directories.

Notes:
- A directory counts as a runtime skill only when it contains `SKILL.md`.
- `browserbase-handoff/` is currently a helper-script directory, not a runtime skill.
