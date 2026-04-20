# Security Policy

## Reporting A Vulnerability

Please do not open public GitHub issues for suspected security vulnerabilities.

Use one of these channels instead:

1. GitHub private vulnerability reporting, if it is enabled for the repository.
2. Email `admin@dowhiz.com` with the subject line `DoWhiz security report`.

Please include:
- a clear description of the issue
- affected files, routes, or components
- reproduction steps or proof of concept
- impact assessment
- any suggested mitigation, if you have one

We will acknowledge receipt as quickly as we can and coordinate next steps privately.

## Supported Versions

Because the open-source release process is still being formalized, support currently follows this policy:

| Version Line | Security Support |
|---|---|
| Latest `main` branch state | Supported |
| Latest tagged release | Supported once public releases are cut |
| `dev` branch | Best effort only |
| Older untagged commits and private deployment snapshots | Not supported |

## Scope Notes

This repo contains integrations with third-party providers and historical/internal deployment material.

When reporting an issue, please be explicit about whether the problem affects:
- the public website/demo path
- the Rust services locally
- a third-party integration
- an internal deployment path that is not part of the supported open-source contract
