# Releasing DoWhiz

This document describes the public release process for the open-source repository.

## Current Policy

DoWhiz is still pre-1.0 from an open-source packaging perspective.

Public contributors should assume:
- `dev` is the integration branch for active work
- `main` is the release branch
- tagged releases are the supported public checkpoints once cut

## Release Criteria

Before cutting a public release:

1. Public PR CI is green on the release commit.
2. Public docs match the shipped behavior.
3. Any scope/support changes are reflected in `README.md`, `OPEN_SOURCE_SCOPE.md`, and `SUPPORT.md`.
4. Release notes summarize user-visible changes and known limitations.

## Release Steps

1. Merge release-ready work into `dev`.
2. Promote the release candidate to `main`.
3. Verify public CI on `main`.
4. Create an annotated tag using semantic versioning, for example `v0.3.0`.
5. Publish GitHub release notes.

## Versioning

We intend to use semantic versioning for public tags:

- `MAJOR`: breaking public contract changes
- `MINOR`: backward-compatible features
- `PATCH`: backward-compatible fixes and doc corrections

Until stable releases are frequent, branch names are not a substitute for public versions.

## Internal Deployment Workflows

The repository still contains internal staging/production deployment workflows tied to branch policy and private infrastructure.

Those workflows are not the public release mechanism for open-source users. They may continue to exist for the core team, but public releases should be understandable without access to private infrastructure.
