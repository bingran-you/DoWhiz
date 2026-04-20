# Self-Hosting

DoWhiz can be self-hosted, but self-hosting support is intentionally scoped.

## Supported Starting Point

The supported public starting point is:

- local frontend demo
- local contributor workflow
- source-level access to the Rust services and integrations

## Best-Effort Self-Hosting

You can explore self-hosting beyond the demo path if you are comfortable supplying and maintaining your own:

- MongoDB
- PostgreSQL
- provider credentials
- model credentials
- storage/queue infrastructure where applicable

## Not A Turnkey Production Appliance

This repo is not yet a fully packaged, zero-context self-hosted product.

In particular:

- internal Azure deployment flows are not the public reference architecture
- several integrations assume vendor-specific setup
- some production behavior still reflects internal branch and environment policy

## Practical Recommendation

If you want to evaluate the project as an external user:

1. Start with the frontend demo path.
2. Move to the local service profile only if you need to work on Rust services.
3. Treat live channel integrations and production-like deployment as opt-in, best-effort territory.

For the detailed scope matrix, see [../../OPEN_SOURCE_SCOPE.md](../../OPEN_SOURCE_SCOPE.md).
