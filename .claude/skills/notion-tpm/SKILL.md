---
name: notion-tpm
description: Manage Notion task boards as a Technical Program Manager - track tasks, contact assignees, update status, and generate reports
---

# Notion TPM Skill

Act as a Technical Program Manager (TPM) to manage Notion task boards, track progress, contact assignees, and generate reports.

## Overview

This skill enables you to:
1. **Manage task boards** - Create and sync tasks between MongoDB and Notion
2. **Read task boards** - Query Notion databases to see all tasks and their status
3. **Contact assignees** - Look up owner contact info and send follow-ups via Slack/Discord
4. **Update status** - Record progress updates as comments on tasks
5. **Generate reports** - Aggregate task data and send summaries to channels

## Task Board Setup & Management

**Important:**
1. **New organization?** Must run `setup-board` first to create the Notion database
2. **Before `list-tasks` or `create-task`**, run `sync-tasks` to ensure MongoDB is in sync with Notion (developers may have updated status directly in Notion)

### Setup a new task board for an organization

```bash
tpm_cli setup-board \
  --organization deeptutor \
  --parent-page-id <NOTION_PAGE_ID> \
  --workspace-id <WORKSPACE_ID>
```

Returns `database_id` to use in subsequent commands. Store this in `organizations.notion_database_id`.

### Create a task (dual write to MongoDB + Notion)

```bash
tpm_cli create-task \
  --organization deeptutor \
  --database-id <DATABASE_ID> \
  --workspace-id <WORKSPACE_ID> \
  --title "Fix PDF crash on large files" \
  --description "PDFs over 100 pages cause crash" \
  --priority p1 \
  --source user_feedback \
  --tags bug,pdf
```

### List tasks from MongoDB

```bash
# All tasks
tpm_cli list-tasks --organization deeptutor

# Filter by status
tpm_cli list-tasks --organization deeptutor --status backlog

# Filter by assignee
tpm_cli list-tasks --organization deeptutor --assignee dev@example.com
```

### Sync status changes from Notion to MongoDB

```bash
tpm_cli sync-tasks \
  --organization deeptutor \
  --database-id <DATABASE_ID> \
  --workspace-id <WORKSPACE_ID>
```

## Workflow

### Step 1: Read the Task Board

```bash
# Get database schema to understand properties
notion_api_cli get-database --database-id <DATABASE_ID>

# Query all incomplete tasks
notion_api_cli query-database --database-id <DATABASE_ID> \
  --filter '{"property": "Status", "status": {"does_not_equal": "Done"}}'
```

**Common filters:**
```json
// Tasks in specific status
{"property": "Status", "status": {"equals": "In Progress"}}

// Tasks assigned to someone
{"property": "Assignee", "people": {"is_not_empty": true}}

// Overdue tasks (if Due Date property exists)
{"property": "Due Date", "date": {"before": "2024-01-01"}}
```

### Step 2: Get Contact Info for Assignees

For each task assignee (Notion person ID), look up their contact info:

```bash
# Get contact info (Slack/Discord handles)
tpm_cli get-contact --notion-user-id <NOTION_USER_ID>
```

**Response:**
```json
{
  "notion_user_id": "abc123",
  "slack_user_id": "U12345ABC",
  "discord_user_id": "123456789012345678",
  "preferred_channel": "slack"
}
```

### Step 3: Send Follow-up Messages

Based on preferred channel, contact the assignee:

**Via Slack:**
```bash
slack_cli send-dm \
  --user-id U12345ABC \
  --message "Hi! 👋 Checking in on task *API Integration*. What's your current progress? Any blockers?"
```

**Via Discord:**
```bash
discord_cli send-dm \
  --user-id 123456789012345678 \
  --message "Hi! 👋 Checking in on task **API Integration**. What's your current progress? Any blockers?"
```

### Step 4: Log Progress Updates

After receiving updates, log them as comments on the task:

```bash
notion_api_cli create-comment --page-id <TASK_ID> \
  --content "**TPM Update ($(date +%Y-%m-%d))**: Contacted @assignee via Slack. Status: On track, 70% complete. ETA: Friday."
```

Also update task properties if needed:

```bash
notion_api_cli update-page --page-id <TASK_ID> \
  --properties '{"Status": {"status": {"name": "In Progress"}}}'
```

### Step 5: Mark Contact Completed

After successful follow-up, update the last_contacted timestamp:

```bash
tpm_cli update-contacted --contact-id <CONTACT_UUID>
```

## Full TPM Workflow Example

```bash
#!/bin/bash
# TPM Daily Check-in Script

DATABASE_ID="your-database-id"

# 1. Get all in-progress tasks
tasks=$(notion_api_cli query-database --database-id "$DATABASE_ID" \
  --filter '{"property": "Status", "status": {"equals": "In Progress"}}')

# 2. For each task, get assignee and contact them
for task in $(echo "$tasks" | jq -c '.[]'); do
  task_id=$(echo "$task" | jq -r '.id')
  task_title=$(echo "$task" | jq -r '.properties.Name.title[0].plain_text')
  assignee_id=$(echo "$task" | jq -r '.properties.Assignee.people[0].id')

  # Skip if no assignee
  [ -z "$assignee_id" ] && continue

  # Get contact info
  contact=$(tpm_cli get-contact --notion-user-id "$assignee_id")
  preferred=$(echo "$contact" | jq -r '.preferred_channel')

  # Send follow-up based on preferred channel
  message="Hi! Checking in on task *$task_title*. What's your progress?"

  if [ "$preferred" = "slack" ]; then
    slack_id=$(echo "$contact" | jq -r '.slack_user_id')
    slack_cli send-dm --user-id "$slack_id" --message "$message"
  elif [ "$preferred" = "discord" ]; then
    discord_id=$(echo "$contact" | jq -r '.discord_user_id')
    discord_cli send-dm --user-id "$discord_id" --message "$message"
  fi

  # Update contacted timestamp
  contact_uuid=$(echo "$contact" | jq -r '.id')
  tpm_cli update-contacted --contact-id "$contact_uuid"
done
```

## Weekly Report Generation

Generate and send a weekly summary:

```bash
# 1. Query all tasks
all_tasks=$(notion_api_cli query-database --database-id "$DATABASE_ID")

# 2. Count by status
done_count=$(echo "$all_tasks" | jq '[.[] | select(.properties.Status.status.name == "Done")] | length')
progress_count=$(echo "$all_tasks" | jq '[.[] | select(.properties.Status.status.name == "In Progress")] | length')
blocked_count=$(echo "$all_tasks" | jq '[.[] | select(.properties.Status.status.name == "Blocked")] | length')

# 3. Format report
report="📊 **Weekly TPM Report - $(date +%Y-%m-%d)**

✅ **Completed:** $done_count tasks
🔄 **In Progress:** $progress_count tasks
⚠️ **Blocked:** $blocked_count tasks

_Generated by TPM Bot_"

# 4. Send to team channel
slack_cli send-channel --channel-id C12345ABC --message "$report"
```

## CLI Commands Reference

### notion_api_cli (Task Board Operations)
| Command | Purpose |
|---------|---------|
| `get-database` | Get database schema |
| `query-database` | Query tasks with filters |
| `update-page` | Update task properties |
| `create-comment` | Add progress comment |
| `search` | Find pages by keyword |

### tpm_cli (Task Board Management)
| Command | Purpose |
|---------|---------|
| `setup-board` | Create Notion database for an organization |
| `create-task` | Create task in MongoDB + Notion |
| `list-tasks` | List tasks from MongoDB |
| `sync-tasks` | Sync status from Notion to MongoDB |

### tpm_cli (Contact Directory)
| Command | Purpose |
|---------|---------|
| `get-contact` | Look up contact by Notion user ID |
| `list-contacts` | List all configured contacts |
| `update-contacted` | Mark contact as recently contacted |

### slack_cli (Slack Messaging)
| Command | Purpose |
|---------|---------|
| `send-dm` | Send direct message |
| `send-channel` | Post to channel |

### discord_cli (Discord Messaging)
| Command | Purpose |
|---------|---------|
| `send-dm` | Send direct message |
| `send-channel` | Post to channel |

## Best Practices

1. **Check before contacting**: Use `contact_frequency_days` to avoid over-pinging
2. **Log all interactions**: Create comments on tasks for audit trail
3. **Respect preferences**: Use the assignee's `preferred_channel`
4. **Aggregate reports**: Don't send individual task updates, summarize
5. **Handle missing contacts**: If no contact info, add comment requesting setup

## Error Handling

| Situation | Action |
|-----------|--------|
| No contact info for assignee | Comment on task asking them to configure contact |
| DM fails (user blocked) | Try channel message or email |
| Task has no assignee | Skip or flag for team review |
| Notion API rate limited | Wait and retry |

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `EMPLOYEE_ID` | For OAuth token lookup |
| `ACCOUNT_ID` | For contact directory lookup |
| `MONGODB_URI` | Task storage (dev_tasks collection) |
| `MONGODB_DATABASE` | Database name for tasks |
| `SLACK_BOT_TOKEN` | Slack messaging |
| `DISCORD_BOT_TOKEN` | Discord messaging |
| `SUPABASE_DB_URL` | Contact directory database |
