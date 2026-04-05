---
name: discord-messaging
description: Send proactive messages to Discord users and channels using discord_cli
allowed-tools: Bash(discord_cli:*)
---

# Discord Messaging Skill

Send messages to Discord users and channels for TPM follow-ups, notifications, and reports.

## Quick Reference

```bash
# Send DM to user
discord_cli send-dm --user-id 123456789012345678 --message "Hi! Checking in on task X"

# Send message to channel
discord_cli send-channel --channel-id 123456789012345678 --message "Weekly status update..."

# Reply to message
discord_cli send-channel --channel-id 123456789012345678 --reply-to 123456789012345678 --message "Thanks!"

# Auto-detect (prefix dm: for DM)
discord_cli send --to dm:123456789012345678 --message "Hello!"
```

## Commands

### send-dm
Send a direct message to a Discord user.

```bash
discord_cli send-dm --user-id <USER_ID> --message "<TEXT>"
```

**Parameters:**
- `--user-id`: Discord user ID (snowflake, 18+ digits)
- `--message`: Message content (max 2000 characters)

**Example:**
```bash
discord_cli send-dm \
  --user-id 123456789012345678 \
  --message "Hi! This is Oliver from DoWhiz. Can you provide an update on the authentication task?"
```

### send-channel
Send a message to a Discord channel.

```bash
discord_cli send-channel --channel-id <CHANNEL_ID> --message "<TEXT>" [--reply-to <MSG_ID>]
```

**Parameters:**
- `--channel-id`: Discord channel ID (snowflake)
- `--message`: Message content
- `--reply-to`: (Optional) Message ID to reply to

**Example:**
```bash
discord_cli send-channel \
  --channel-id 123456789012345678 \
  --message "📋 **Weekly TPM Report**\n\n**Completed:** 5 tasks\n**In Progress:** 3 tasks\n**Blocked:** 1 task"
```

### send
Auto-detect destination and send message.

```bash
discord_cli send --to <ID> --message "<TEXT>" [--reply-to <MSG_ID>]
```

Use `dm:USER_ID` prefix to force DM creation.

## Message Formatting

Discord supports markdown formatting:

| Format | Syntax | Example |
|--------|--------|---------|
| Bold | `**text**` | `**important**` |
| Italic | `*text*` or `_text_` | `*emphasis*` |
| Underline | `__text__` | `__underlined__` |
| Strikethrough | `~~text~~` | `~~deleted~~` |
| Code | `` `code` `` | `` `variable` `` |
| Code block | ` ```language\ncode\n``` ` | Multi-line code |
| Quote | `> text` | Block quote |
| Spoiler | `||text||` | Hidden text |
| User mention | `<@USER_ID>` | `<@123456789>` |
| Channel link | `<#CHANNEL_ID>` | `<#123456789>` |
| Role mention | `<@&ROLE_ID>` | `<@&123456789>` |

## TPM Use Cases

### 1. Task Follow-up DM
```bash
discord_cli send-dm \
  --user-id 123456789012345678 \
  --message "👋 Hi! Checking in on task **API Integration**. What's your current progress?"
```

### 2. Weekly Report to Channel
```bash
discord_cli send-channel \
  --channel-id 123456789012345678 \
  --message "📊 **Weekly TPM Report - $(date +%Y-%m-%d)**

**Completed This Week:**
• Task A - ✅ Done
• Task B - ✅ Done

**In Progress:**
• Task C - 70% complete
• Task D - Starting

**Blockers:**
• ⚠️ Waiting on API documentation

_Reply to this message for questions._"
```

### 3. Deadline Reminder
```bash
discord_cli send-dm \
  --user-id 123456789012345678 \
  --message "⏰ **Reminder:** Task **Database Migration** is due tomorrow. Please update the status in Notion."
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `DISCORD_BOT_TOKEN` | Yes | Bot token from Discord Developer Portal |
| `DISCORD_API_BASE_URL` | No | Override API base (default: `https://discord.com/api/v10`) |

## Error Handling

| Error | Cause | Solution |
|-------|-------|----------|
| `Unknown Channel` | Invalid channel ID | Verify channel ID |
| `Cannot send messages to this user` | User has DMs disabled or bot blocked | Use channel instead |
| `Missing Permissions` | Bot lacks permissions | Check bot role permissions |
| `50007` | Cannot DM user | User must share a server with bot |

## Required Bot Permissions

For the bot to work properly, it needs these Discord permissions:

- `Send Messages` - Send messages in channels
- `Read Message History` - Read messages for replies
- `Create Private Threads` (optional) - For threaded discussions

## Character Limits

- **Message content**: 2000 characters
- **Embeds**: 6000 characters total
- **Messages per channel**: ~5/second rate limit

Messages exceeding 2000 characters will be automatically truncated with `...`

## Tips

1. **DM limitations**: Users must share a server with the bot and have DMs enabled
2. **Use channel for reliability**: Channel messages work even if user has DMs disabled
3. **Include context**: Always mention task name, deadline, etc.
4. **Rate limits**: Discord limits ~5 messages/second per channel
5. **Snowflake IDs**: Discord IDs are 18+ digit numbers

## Integration with TPM

When contacting task owners from Notion:

```bash
# 1. Get contact info from TPM directory
contact=$(tpm_cli get-contact --notion-user-id abc123)
discord_id=$(echo "$contact" | jq -r '.discord_user_id')

# 2. Send follow-up if Discord is preferred
if [ -n "$discord_id" ]; then
  discord_cli send-dm \
    --user-id "$discord_id" \
    --message "Hi! Checking in on your assigned task..."
fi
```
