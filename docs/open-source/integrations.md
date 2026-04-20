# Integrations

DoWhiz includes code for many integrations, but not all of them are equally productized for external users.

## Integration Status

| Integration Area | Status |
|---|---|
| Website local demo | Supported |
| Core frontend contributor workflow | Supported |
| Rust code and adapters at source level | Supported |
| Live provider integrations with your own credentials | Best effort |
| Private DoWhiz production setup | Out of scope |

## Typical Provider Dependencies

Depending on the path you explore, integrations may require:

- OpenAI or Azure OpenAI compatible credentials
- MongoDB
- PostgreSQL
- Postmark
- Slack app credentials
- Discord bot credentials
- Google Workspace OAuth or service-account setup
- Notion OAuth setup
- Twilio or other messaging credentials

## Guidance For Contributors

- If your change does not require a live provider, prefer the local demo path and mocked/unit-tested flows.
- If your change does require a live provider, document the exact prerequisites and keep the validation clearly marked as opt-in.
- Do not make public CI depend on private provider credentials.
