# Skills And Plugins

DoWhiz has two different skill contexts that are easy to confuse.

## Runtime Skills

`DoWhiz_service/skills/` is the runtime source of truth for shared skills that are copied into task workspaces.

See:

- `DoWhiz_service/skills/README.md`
- `DoWhiz_service/skills/manifest.toml`

## Desktop/Authoring Skill Catalogs

The repo also contains desktop-agent skill directories such as `.agents/skills/`.

Those are useful for local authoring workflows, but they are not the same thing as the runtime skill catalog used by the Rust service.

## Plugin Status

Plugin authoring is not yet a stable, first-class public extension surface in this repository.

For now:

- runtime skills are the supported extension point to read and modify
- plugin-style or internal agent catalog structures should be treated as experimental unless documented otherwise

If you want to improve extension surfaces, prefer small changes that make skill boundaries, manifests, and docs clearer.
