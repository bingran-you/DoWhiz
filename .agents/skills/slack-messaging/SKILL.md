---
name: slack-messaging
description: Send proactive messages to Slack users and channels using slack_cli
allowed-tools: Bash(slack_cli:*)
---

# Slack Messaging Skill

Send messages to Slack users and channels for TPM follow-ups, notifications, and reports.

## Quick Reference

```bash
# Send DM to user
slack_cli send-dm --user-id U12345ABC --message "Hi! Checking in on task X"

# Send message to channel
slack_cli send-channel --channel-id C12345ABC --message "Weekly status update..."

# Reply in thread
slack_cli send-channel --channel-id C12345ABC --thread-ts "1234567890.123456" --message "Thanks!"

# Auto-detect (user ID creates DM, channel ID posts to channel)
slack_cli send --to U12345ABC --message "Hello!"
```

## Commands

### send-dm
Send a direct message to a Slack user.

```bash
slack_cli send-dm --user-id <USER_ID> --message "<TEXT>"
```

**Parameters:**
- `--user-id`: Slack user ID (starts with `U`, e.g., `U12345ABC`)
- `--message`: Message content (supports Slack mrkdwn formatting)

**Example:**
```bash
slack_cli send-dm \
  --user-id U0123456789 \
  --message "Hi! This is Oliver from DoWhiz. Can you provide an update on the authentication task?"
```

### send-channel
Send a message to a Slack channel.

```bash
slack_cli send-channel --channel-id <CHANNEL_ID> --message "<TEXT>" [--thread-ts <TS>]
```

**Parameters:**
- `--channel-id`: Slack channel ID (starts with `C`, e.g., `C12345ABC`)
- `--message`: Message content
- `--thread-ts`: (Optional) Message timestamp to reply in thread

**Example:**
```bash
slack_cli send-channel \
  --channel-id C0123456789 \
  --message ":memo: Weekly TPM Report\n\n*Completed:* 5 tasks\n*In Progress:* 3 tasks\n*Blocked:* 1 task"
```

### send
Auto-detect destination and send message.

```bash
slack_cli send --to <ID> --message "<TEXT>" [--thread-ts <TS>]
```

If `--to` starts with `U`, creates a DM. Otherwise, sends to channel.

## Message Formatting

Slack supports mrkdwn formatting:

| Format | Syntax | Example |
|--------|--------|---------|
| Bold | `*text*` | `*important*` |
| Italic | `_text_` | `_emphasis_` |
| Strikethrough | `~text~` | `~deleted~` |
| Code | `` `code` `` | `` `variable` `` |
| Code block | ` ```code``` ` | Multi-line code |
| Link | `<URL|text>` | `<https://example.com|Click here>` |
| User mention | `<@USER_ID>` | `<@U12345>` |
| Channel link | `<#CHANNEL_ID>` | `<#C12345>` |
| Emoji | `:emoji_name:` | `:white_check_mark:` |

## TPM Use Cases

### 1. Task Follow-up
```bash
slack_cli send-dm \
  --user-id U0123456789 \
  --message "Hi! :wave: Checking in on task *API Integration*. What's your current progress?"
```

### 2. Weekly Report to Channel
```bash
slack_cli send-channel \
  --channel-id C0123456789 \
  --message ":chart_with_upwards_trend: *Weekly TPM Report - $(date +%Y-%m-%d)*

*Completed This Week:*
• Task A - Done
• Task B - Done

*In Progress:*
• Task C - 70% complete
• Task D - Starting

*Blockers:*
• Waiting on API documentation

_Reply in thread for questions._"
```

### 3. Reminder DM
```bash
slack_cli send-dm \
  --user-id U0123456789 \
  --message ":alarm_clock: Reminder: Task *Database Migration* is due tomorrow. Please update the status in Notion."
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `SLACK_BOT_TOKEN` | Yes | Bot OAuth token (`xoxb-...`) |
| `SLACK_API_BASE_URL` | No | Override API base (default: `https://slack.com/api`) |

## Error Handling

| Error | Cause | Solution |
|-------|-------|----------|
| `channel_not_found` | Invalid channel ID | Verify channel ID and bot membership |
| `not_in_channel` | Bot not in channel | Invite bot to the channel |
| `user_not_found` | Invalid user ID | Verify user ID format |
| `cannot_dm_bot` | Trying to DM a bot | Use channel instead |
| `no_permission` | Missing scopes | Check bot OAuth scopes |

## Required Bot Scopes

- `chat:write` - Send messages
- `channels:read` - List channels
- `users:read` - Look up users
- `im:write` - Send DMs
- `groups:write` - Post to private channels

## Tips

1. **Always confirm user IDs** before sending DMs to avoid contacting wrong person
2. **Use threads** for follow-up messages to keep channels organized
3. **Include context** in messages (task name, deadline, etc.)
4. **Rate limits**: Slack limits ~1 message per second per channel
