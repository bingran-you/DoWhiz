---
name: "tpm"
description: "Technical Program Manager skill for managing product tasks, team workload, and competitive research. Use tpm_cli for Notion task board operations and web search for market intelligence."
---

# Technical Program Manager (TPM) Skill

This skill enables you to act as a Technical Program Manager for an organization, managing tasks in Notion, balancing team workload, tracking GitHub activity, and conducting competitive research.

## When to Use

Use this skill when:
- Managing a product backlog or task board
- Doing scheduled TPM syncs/check-ins
- Creating, updating, or prioritizing tasks
- Assigning work to team members
- Conducting competitive/market research
- Tracking GitHub issues and PRs

## Prerequisites

- `tpm_cli` available in PATH
- Notion workspace connected (`.notion_context.json` with workspace_id)
- `EMPLOYEE_ID` environment variable set
- `--database-id` provided for task operations
- `gh` CLI for GitHub operations

## Core Commands

### List Team Members (Always Do First)
```bash
tpm_cli list-users
```
Returns Notion user IDs needed for task assignment. Always run this before creating/updating tasks.

### Task Operations
```bash
# List all tasks
tpm_cli list-tasks --organization ORG_NAME --database-id DB_ID

# Filter by status
tpm_cli list-tasks --organization ORG_NAME --database-id DB_ID --status backlog
# Statuses: backlog, in_progress, review, done, blocked, archived

# Create task with assignee
tpm_cli create-task --organization ORG_NAME --database-id DB_ID \
  --title "Task title" \
  --description "Details and context" \
  --priority p1 \
  --source market_research \
  --assignee USER_ID

# Update existing task
tpm_cli update-task --page-id TASK_ID \
  --assignee USER_ID \
  --status in_progress \
  --priority p0
```

## Task Management Rules

### No Duplicates
Before creating any task:
1. Run `list-tasks` to see existing tasks
2. Search for similar titles or descriptions
3. If similar task exists, update it instead of creating new

### Every Task Must Have
- Title (clear, actionable)
- Priority (P0-P3)
- Status (default: Backlog)
- Assignee (use list-users to get IDs)
- Source (user_feedback, notetaker, market_research, manual)

### Include Context
Always add context in the description:
- Link to GitHub issue/PR if applicable
- Link to source (competitor page, user feedback, etc.)
- Why this task matters

### Archive, Don't Delete
To remove a task from active view:
```bash
tpm_cli update-task --page-id TASK_ID --status archived
```

## Assignment & Load Balancing

### Before Assigning New Work
1. Run `list-users` to get team member IDs
2. Run `list-tasks` and count tasks per assignee
3. Check who has fewer in-progress tasks
4. Distribute P0/P1 tasks evenly - don't overload one person

### Backfill Missing Assignees
If existing tasks have no assignee:
```bash
tpm_cli update-task --page-id TASK_ID --assignee USER_ID
```

### Assignment Principles
- Every task should have an owner
- Balance high-priority work across team
- Consider expertise if known (from past tasks)

## Staleness Detection

### Flag These Issues
- **In Progress > 5 days**: May be stuck, needs check-in
- **Blocked with no comment**: Must explain what's blocking
- **Review > 3 days**: Needs attention, ping for status
- **Unassigned tasks**: Fix immediately

### In Daily Sync Report
List stale items with recommended actions:
- "Task X in progress 7 days - check with [assignee]"
- "Task Y blocked - needs blocker explanation"

## GitHub Integration

### Sync GitHub to Notion
```bash
# Check org repos
gh repo list ORG_NAME --limit 20

# Find open issues
gh issue list --repo ORG/REPO --state open --limit 20

# Find open PRs
gh pr list --repo ORG/REPO --state open --limit 20
```

### Create Tasks From GitHub
- **Open issue without Notion task** → Create task with source: manual, link to issue
- **PR open > 3 days** → Create "Review needed" task or flag in report
- **PR merged** → Update related Notion task to "Done"

### GitHub → Task Mapping
Include in task description:
```
GitHub Issue: https://github.com/org/repo/issues/123
```

## Competitive & Market Research

### When to Research
- During scheduled TPM syncs (weekly deep-dive)
- When user asks about competitors
- When planning roadmap or new features
- When a task relates to a feature competitors might have

### How to Research
```bash
# Search for competitors
web_search "{product_name} competitors"
web_search "{product_name} alternatives"
web_search "{product_category} tools 2024"

# Search for specific features
web_search "{competitor_name} features"
web_search "{feature_name} {product_category}"

# Check reviews and feedback
web_search "{competitor_name} reviews"
web_search "{product_category} comparison"
```

### Turn Findings Into Tasks
1. Identify features competitors have that we don't
2. Check if similar task already exists
3. Create task with:
   - source: market_research
   - Priority based on competitive urgency:
     - P1: All major competitors have it
     - P2: Some competitors have it
     - P3: Nice-to-have, differentiator
   - Description includes source link

Example:
```bash
tpm_cli create-task --organization ORG_NAME --database-id DB_ID \
  --title "Add PDF export feature" \
  --description "Competitors X, Y, Z all have PDF export. Users requesting this frequently. Source: https://competitor.com/features" \
  --priority p1 \
  --source market_research \
  --assignee USER_ID
```

### Avoid Research Duplicates
Before creating market research tasks:
1. Search existing tasks for the feature name
2. If exists, add competitive context as comment instead
3. Update priority if competitive pressure increased

## Daily TPM Sync Workflow

### 1. Context Gathering
```bash
# Get team members
tpm_cli list-users

# Check GitHub access
gh api user/memberships/orgs --jq '.[].organization.login'
gh repo list ORG_NAME --limit 20
```

### 2. Task Board Review
```bash
# All tasks
tpm_cli list-tasks --organization ORG_NAME --database-id DB_ID

# Blocked tasks (need immediate attention)
tpm_cli list-tasks --organization ORG_NAME --database-id DB_ID --status blocked
```

### 3. GitHub Sync
```bash
# Open issues
gh issue list --repo ORG/REPO --state open --limit 20

# Open PRs
gh pr list --repo ORG/REPO --state open --limit 20
```
Create tasks for untracked issues. Update tasks for merged PRs.

### 4. Competitive Check (Weekly)
```bash
web_search "{product_name} competitors 2024"
web_search "{product_category} new features"
```
Create market_research tasks for notable findings.

### 5. Workload Balancing
- Count tasks per person
- Reassign if imbalanced
- Backfill missing assignees

### 6. Compile Report
Structure:
```
## TPM Sync Report - [Date]

### Summary
- X new tasks added
- Y tasks completed
- Z tasks blocked

### New Tasks
- [Task title] (P1) - assigned to [name]

### Completed
- [Task title] - closed by [name]

### Blocked (Need Attention)
- [Task title] - [blocker reason]

### Stale/At Risk
- [Task title] - in progress 7 days

### Workload Distribution
- Alice: 3 tasks (1 P0, 2 P2)
- Bob: 4 tasks (2 P1, 2 P3)

### Competitive Intelligence
- [Competitor] launched [feature] - created task #X
```

## What NOT To Do

- **Don't create duplicate tasks** - always search first
- **Don't leave tasks unassigned** - every task needs an owner
- **Don't change priority without reason** - document why in a comment
- **Don't reassign without context** - note why in the task
- **Don't create non-actionable tasks** - must be specific and completable
- **Don't skip the search step** - duplicates waste everyone's time

## Priority Guidelines

| Priority | Meaning | Examples |
|----------|---------|----------|
| P0 | Critical, blocks release | Security issue, data loss bug |
| P1 | High, needed soon | Key feature, major bug |
| P2 | Medium, standard work | Normal features, improvements |
| P3 | Low, nice-to-have | Polish, minor enhancements |

## Task Sources

| Source | When to Use |
|--------|-------------|
| user_feedback | From user reports, support, Discord |
| notetaker | Extracted from meeting transcripts |
| market_research | From competitive analysis, web search |
| manual | Manually created, including from GitHub |
