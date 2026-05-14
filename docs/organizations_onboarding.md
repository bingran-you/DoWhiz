# Organizations Onboarding Guide

This guide walks through setting up an organization in DoWhiz, configuring team roles, and enabling TPM (Task/Project Management) features.

## Overview

Organizations allow teams to:
- Share Oliver's TPM capabilities across team members
- Centralize Notion task board management
- Run automated syncs via cron jobs (leader-only)
- Connect community channels (Discord/Slack) for bug tracking

## Prerequisites

Before creating or joining an organization:
1. Sign up for a DoWhiz account at [dowhiz.com/auth](https://dowhiz.com/auth)
2. Connect your Notion workspace (Settings > Connections > Notion)
3. Note your **Account ID** (displayed in the dashboard sidebar)

## Creating an Organization

1. Navigate to the **Organizations section** in the DoWhiz account panel
2. Click **Create Organization**
3. Enter your organization name
4. Click **Create**

* You are automatically the acting leader of organizations you create.
* If you would like to manually configure a leader of the organization, see **Setting the Organization Leader** section

## Joining an Organization

1. Get the **Organization ID** from your team leader
2. Navigate to **Settings > Organization**
3. Enter the Organization ID in the "Join Organization" field
4. Click **Join**


## Organization Roles

### Leader
- Can configure TPM settings (Notion database ID, cron schedules)
- Their Notion credentials are used for all TPM operations
- Can set up community bug tracking channels

### Member
- Can trigger manual TPM syncs
- Can view organization settings (read-only for TPM config)
- Cannot modify cron schedules or leader settings

## Setting the Organization Leader

The leader's Notion account is used for all TPM API calls. To set a leader:

1. Get the leader's **DoWhiz Account ID** (they can find this in their dashboard sidebar)
2. Navigate to **Settings > Organization > Advanced**
3. Enter the Account ID in "Leader's DoWhiz Account ID"
4. Click **Set Leader**

If no leader is set, any member can configure TPM settings (acting leader mode).

## Configuring TPM (Task/Project Management)

### Step 1: Connect Notion

The organization leader must have Notion connected:
1. Go to **Settings > Connections**
2. Click **Connect Notion**
3. Authorize DoWhiz to access your Notion workspace

### Step 2: Set the Task Board Database

1. Create a Notion database for your task board (or use an existing one)
2. Copy the database ID from the Notion URL:
   - URL format: `notion.so/{workspace}/{database_id}?v=...`
   - The database ID is the 32-character string before the `?`
3. Navigate to **Settings > Organization > Advanced**
4. Paste the database ID in "Notion Task Board Database ID"
5. Click **Set Database**


### Step 3: Enable Cron Sync (Leader Only)

Automated syncs run on a schedule to keep the task board updated:

1. Navigate to **Settings > Organization > Advanced**
2. In the TPM Cron section, configure:
   - **Frequency**: How often to sync (e.g., every 6 hours)
   - **Sync type**: Full sync or incremental
3. Click **Enable Cron**

Only the organization leader can see and configure cron settings.

## Community Bug Tracking

Connect community channels to automatically scan for bug reports during TPM syncs.

### Discord Setup

1. Navigate to **Settings > Organization > Advanced > Community Bug Tracking**
2. Enter your Discord **Server ID**
   - Enable Developer Mode in Discord (Settings > Advanced)
   - Right-click your server > Copy Server ID
3. Click **Set Discord Server**

### Slack Setup

1. Navigate to **Settings > Organization > Advanced > Community Bug Tracking**
2. Enter your Slack **Workspace ID**
   - Found in your Slack workspace URL or admin settings
3. Click **Set Slack Workspace**

## Oliver's Self-Assignment

Oliver can be assigned tasks by adding the "oliver" tag to a task's Tags property. Oliver automatically queries for tasks with this tag during TPM operations.

To assign a task to Oliver:
1. Open the task in Notion
2. Add "oliver" to the Tags multi-select property
3. Oliver will pick up the task on the next sync

Tasks with no assignee are also considered Oliver's responsibility.

## Troubleshooting

### "Notion not connected" Error

The organization leader's Notion account is not connected:
1. Have the leader log into DoWhiz
2. Navigate to **Settings > Connections > Notion**
3. Re-authorize the connection

### TPM Cron Section Not Visible

Only the organization leader can see cron settings:
1. Verify you are set as the leader (check "Leader's DoWhiz Account ID" matches your Account ID)
2. If no leader is set, any member should see the section

### Database ID Not Saving

1. Ensure the database ID is a valid 32-character Notion ID
2. Verify the leader's Notion account has access to the database
3. Try disconnecting and reconnecting Notion

### Manual Sync Fails

1. Check that Notion is connected (Settings > Connections)
2. Verify the Notion database ID is set correctly
3. Check advanced settings: verify Discord Server, Slack Workspace IDs are correct
4. Verify the leader's DoWhiz account ID matches

## Related Documentation

- [Oliver TPM Deep Dive, Update Log](oliver-tpm-deeptutor.md)
