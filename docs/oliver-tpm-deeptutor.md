# Oliver as TPM for DeepTutor

## Overview

Pivot Oliver from a task executor to a **Technical Project Manager (TPM)** for DeepTutor development. Oliver will manage tasks, assign work to developers, and track progress, without performing the implementation work itself.

## DoWhiz (Existing Infrastructure)
- **MongoDB** for task scheduling, user storage, credentials
- **PostgreSQL** for accounts, payments, analytics
- **Notion CLI** (`notion_api_cli`) to work with pages, databases, comments
- **IngestionQueue** trait with enqueue/claim/complete semantics
- **Multi-channel integration** (email, Slack, Discord, Notion, etc.)
- **AccountStore** for unified identity across channels

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Oliver (TPM Mode)                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────────┐   │
│  │ Task Queue   │   │ Sentiment    │   │ Notetaker Reader     │   │
│  │ (MongoDB)    │   │ Analyzer     │   │ (Otter.ai, etc.)     │   │
│  └──────┬───────┘   └──────┬───────┘   └──────────┬───────────┘   │
│         │                  │                       │               │
│         └──────────────────┼───────────────────────┘               │
│                            │                                        │
│                    ┌───────▼───────┐                               │
│                    │ Task Router   │                               │
│                    │ (Prioritize,  │                               │
│                    │  Assign)      │                               │
│                    └───────┬───────┘                               │
│                            │                                        │
├────────────────────────────┼────────────────────────────────────────┤
│                            ▼                                        │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    Notion Task Board                         │   │
│  │  (Backlog | In Progress | Review | Done)                    │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                Developer Notifications                       │   │
│  │  (Discord/Slack/Email assignment notifications)             │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Components

### 1. Routing & Task Queue

#### Routing: Organization-Based Resolution

Reuse the existing Azure Service Bus inbound gateway. Same Oliver (employee), different behavior based on the **sender's organization**.

* **Organizations table** (need to implement organization joining on DoWhiz integration panel)
    - For MVP, suffices to just have one organization (DeepTutor)
* **Organizations column in accounts table** - given an account, can identify the organization they're a part of
* Use organization name to determine behavior and query appropriate task store (currently only DeepTutor setup with `DevTaskStore`)

**Schema (Supabase PostgreSQL):**

```sql
-- New table for organizations
CREATE TABLE organizations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT UNIQUE NOT NULL,           -- "deeptutor", "acme-corp", etc.
    notion_database_id TEXT,             -- Notion task board ID (populate from tpm_cli setup-board command)
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Add organization reference to accounts
ALTER TABLE accounts ADD COLUMN organization_id UUID REFERENCES organizations(id);

-- Seed DeepTutor org
INSERT INTO organizations (name) VALUES ('deeptutor');
```

**Routing Flow:**

```
1. User pings Oliver (any channel: Discord, email, Slack, etc.)
                    ↓
2. Gateway identifies sender (email, discord user id, etc.)
                    ↓
3. Look up sender's DoWhiz account in Supabase
                    ↓
4. Fetch account.organization_id → organizations.name
                    ↓
5. organization == "deeptutor" → TPM mode
   organization == NULL/other  → Regular mode
```

**How Oliver differentiates:**
- `organization = "deeptutor"` → TPM mode (manage tasks, assign to humans, use `DevTaskStore`)
- `organization = NULL` or other → Regular mode (execute tasks via Codex)

The prompt builder (`prompt.rs`) or workspace setup checks the organization and injects TPM-specific instructions/skills.

**ACI Container Access:**

Oliver runs inside an ACI container. For TPM mode, it needs access to `DevTaskStore` (MongoDB `dev_tasks` collection) to:
- Query pending tasks
- Update task status
- Assign tasks to developers
- Link tasks to Notion pages

The ACI container already has `MONGODB_URI` for other operations. For TPM mode, Oliver imports `DevTaskStore` from `scheduler_module` and uses it directly:

```rust
// In TPM mode, Oliver creates a store scoped to the user's organization:
let store = DevTaskStore::new("deeptutor")?;
let backlog = store.list_tasks_by_status(TaskStatus::Backlog)?;
store.update_status(&task_id, TaskStatus::InProgress)?;
store.update_assignee(&task_id, Some("dev@example.com"))?;
```

**Multi-tenant design:** All organizations share the same `dev_tasks` collection. Each document has an `organization` field, and all queries filter by it. No cross-org data leakage.

```
dev_tasks collection
├── { organization: "deeptutor", title: "Fix PDF crash", ... }
├── { organization: "deeptutor", title: "Add dark mode", ... }
├── { organization: "acme-corp", title: "Update API", ... }
└── { organization: "acme-corp", title: "Fix login", ... }
```

**Frontend Flow (DoWhiz account settings):**
1. User searches for organization: `SELECT * FROM organizations WHERE name ILIKE '%query%'`
2. User clicks to join: `UPDATE accounts SET organization_id = X WHERE id = Y`

#### Task Storage (MongoDB) — LEGACY

> **Note:** As of 4/20/26, we no longer use MongoDB for task storage. Tasks are stored directly in Notion. The DevTaskStore and bidirectional sync have been removed. This section is kept for historical reference.

Extend existing `TaskKind` enum:

```rust
pub enum TaskKind {
    SendReply(SendReplyTask),
    RunTask(RunTaskTask),
    DevTask(DevTask),  // New Task for DeepTutor Development
    Noop,
}

enum Priority { P0, P1, P2, P3 }
enum TaskStatus { Backlog, InProgress, Review, Done, Blocked, Archived }
enum TaskSource { UserFeedback, Notetaker, MarketResearch, Manual }

struct DevTask {
    organization: String,           // Multi-tenant: "deeptutor", "acme-corp", etc.
    title: String,
    description: String,
    priority: Priority,
    status: TaskStatus,
    assignee: Option<String>,
    source: TaskSource,
    tags: Vec<String>,
    notion_page_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

**Operations:**
- `enqueue_task(task)` — Add new task to backlog
- `pop_completed()` — Remove task when developer confirms completion
- `get_tasks_by_status(status)` — Query for board views
- `assign_task(task_id, assignee)` — Assign to developer
- `update_checkpoint(task_id, checkpoint_id, completed)` — Track progress

### 2. Passive User Feedback Collection (User Feedback Pipeline) - Likely Most Difficult

Oliver monitors DeepTutor user sentiment to discover tasks:

**Sources:**
- Official DeepTutor Discord
- Support emails forwarded to Oliver/DeepTutor
- In-app feedback submissions (if implemented)

**Pipeline:**
1. **Ingest** — Collect feedback from sources (scheduled job or webhook)
2. **Classify** — Bug report, feature request, praise, confusion, churn risk
3. **Extract** — Pull actionable items (what specifically is broken/requested)
4. **Deduplicate** — Match against existing tasks
5. **Enqueue** — Create task with `source: UserFeedback`

**Example Output:**
```
Source: App Store Review (3 stars)
Text: "Love the AI summaries but it crashes when I open large PDFs over 100 pages"
→ Task: "Fix crash on large PDFs (100+ pages)"
   Priority: P1
   Tags: [bug, pdf-reader, stability]
```

* Review the feature to make sure its aligned with roadmap (bugs, feature requests)

### 3. Notetaker Integration

Read transcripts from daily standups to extract tasks/blockers:

**Supported Sources:**
- Otter.ai (primary)
- Fireflies.ai
- Google Meet transcripts
- Zoom transcripts

**Pipeline:**
1. **Detect new transcript** — Poll or webhook when meeting ends
2. **Parse** — Extract action items, blockers, decisions
3. **Match assignees** — Map names to developer accounts
4. **Enqueue** — Create tasks with `source: Notetaker`, link to transcript

**Extraction Patterns:**
- "ACTION: [person] will [task]"
- "[person] is blocked on [issue]"
- "We decided to [decision]"
- "TODO: [task]"
- "Next steps: [list]"

### 4. Notion Board Sync

Reuse existing `notion_api_cli` to maintain a Kanban board:

**Board Structure:**
```
DeepTutor Development Board
├── Backlog (database view: status = Backlog, sorted by priority)
├── In Progress (status = InProgress)
├── Review (status = Review)
├── Done (status = Done, last 2 weeks)
├── Blocked (status = Blocked)
└── Archived (status = Archived, soft-deleted tasks)
```

**Sync Operations:**
- `create_task_page(task)` — Create Notion page, store `notion_page_id`
- `update_task_status(task_id, status)` — Move card on board
- `sync_from_notion()` — Pull updates developers made directly in Notion
- `add_comment(task_id, message)` — Oliver comments on progress/blockers

### 5. Developer Assignment & Notifications

**Assignment Logic:**
1. Match task tags to developer expertise
2. Check current workload (tasks in progress)
3. Round-robin for equal-priority tasks

**Outbound Task Assignment:**
- Primary: Notion @mention on task page + Email
- Secondary: Discord/Slack DM for P0/P1 + Email

---

## Oliver's TPM Behaviors
* Instead of being reactive to an incoming task, Oliver becomes a proactive agent.

### Daily Operations (Cron job)
1. **Morning sync** — Check notetaker for yesterday's standup, enqueue new tasks
2. **Sentiment scan** — Review new user feedback, create tasks
3. **Board hygiene** — Ping stale tasks, update priorities based on new info
4. **EOD summary** — Post daily progress to team channel

### When Developer @Oliver-DoWhiz with updates
1. Move task to Review (if has reviewer) or Done
2. Pop from active queue
3. Notify stakeholders
4. Update Notion board
5. Check if this unblocks other tasks

### When User Feedback Arrives
1. Classify sentiment and urgency
2. Check for duplicate/related tasks
3. If new: create task, assign priority
4. If duplicate: add comment to existing task, potentially bump priority
5. Acknowledge feedback to user (if reply channel available)

### When Market Research Suggests Feature
1. Create task with `source: MarketResearch`
2. Tag as `feature` with relevant area
3. Add competitive context to description
4. Default to P2 unless strategic priority

---

## Data Model Additions — LEGACY

> **Note:** MongoDB collections for tasks are no longer used. Tasks are stored in Notion only.

### MongoDB Collections

```
deeptutor_tasks          — Task queue and metadata (LEGACY)
deeptutor_checkpoints    — Granular progress tracking (optional, can embed)
deeptutor_feedback       — Raw user feedback before processing
deeptutor_transcripts    — Notetaker transcript references
```

### Developer Profile Extension

Add to existing `accounts` or new collection:

```rust
struct DeveloperProfile {
    account_id: String,
    expertise: Vec<String>,        // ["backend", "pdf", "ai", "ui"]
    max_concurrent_tasks: u32,     // Default: 3
    notification_preferences: NotificationPrefs,
    focus_hours: Option<(u8, u8)>, // No pings during focus time
}
```

---

## Implementation

### Organization-Based Routing
- ✅ Add `organizations` table to Supabase
- ✅ Add `organization_id` column to `accounts` table
- ✅ Update gateway to fetch account's organization and route accordingly
- Frontend: org search + join flow in DoWhiz account settings

### Core Task Queue
- ✅ MongoDB collection + CRUD operations (`dev_task_store.rs`)
- ✅ TPM CLI commands (`tpm_cli.rs`): setup-board, create-task, list-tasks, sync-tasks
- ✅ Cron job initialization (via `setup_tpm_cron` function with synthetic trigger)
- Manual task creation via Oliver
- Assignment notifications

### Notetaker Integration
- Otter.ai transcript reader
- Action item extraction (upstream LLM-based)
- Auto-enqueue from standups

### Sentiment Pipeline
- Feedback ingestion (start with forwarded emails)
- Classification + extraction
- Deduplication logic
- Auto-enqueue with source tracking

### Smart Assignment
- Developer profiles + expertise matching
- Workload balancing
- Priority-based routing

### TPM CLI (`tpm_cli`)

* These commands will be exposed to Codex inside the ACI container

Task board commands for managing DevTasks across MongoDB and Notion:

**Important:**
1. **New organization?** Must run `setup-board` first to create the Notion database
2. **Before `list-tasks`**, run `sync-tasks` to pull any status changes developers made directly in Notion

**Command purposes:**
- ✅ `setup-board` — Create Notion database for an organization
- ✅ `create-task` — Oliver autonomously creates tasks (from user feedback, notetaker, market research)
- ✅ `update-task` — Update existing task's assignee, status, or priority
- ✅ `list-tasks` — List tasks from Notion with filters
- ✅ `list-users` — List Notion workspace users (for task assignment)
- ⚠️ `sync-tasks` — **LEGACY** (was for Notion ↔ MongoDB sync, no longer used)

#### `setup-board` — Create Notion database for an organization

```bash
tpm_cli setup-board \
  --organization deeptutor \
  --parent-page-id <NOTION_PAGE_ID> \
  --workspace-id <WORKSPACE_ID>
```

Creates a Notion database with TPM schema:
- **Name** (title)
- **Status** (select: Backlog, In Progress, Review, Done, Blocked, Archived)
- **Priority** (select: P0, P1, P2, P3)
- **Assignee** (people)
- **Tags** (multi-select)
- **Source** (select: User Feedback, Notetaker, Market Research, Manual)
- **MongoDB ID** (rich_text — links to `dev_tasks` collection)

Returns `database_id` to store in `organizations.notion_database_id`.

[TODO] Link notion_database_id with organization in Supabase Postgres

#### `create-task` — Oliver autonomously creates tasks

```bash
tpm_cli create-task \
  --organization deeptutor \
  --database-id <NOTION_DATABASE_ID> \
  --workspace-id <WORKSPACE_ID> \
  --title "Fix PDF crash on large files" \
  --description "PDFs over 100 pages cause app crash" \
  --priority p1 \
  --source user_feedback \
  --tags bug,pdf,stability \
  --assignee dev@example.com
```

Use when Oliver identifies a task from:
- User feedback (Discord, email, in-app)
- Meeting transcripts (notetaker)
- Market research

Flow:
1. Insert `DevTask` into MongoDB `dev_tasks` collection
2. Create page in Notion database with `MongoDB ID` property
3. Link `notion_page_id` back to MongoDB document with DevTaskStore's `link_notion_page`

#### `list-tasks` — List tasks from Notion

```bash
# List all tasks
tpm_cli list-tasks --organization deeptutor --database-id <DB_ID>

# Filter by status (backlog, in_progress, review, done, blocked, archived)
tpm_cli list-tasks --organization deeptutor --database-id <DB_ID> --status backlog
```

#### `update-task` — Update existing task

```bash
# Assign task to a user (use list-users to get user IDs)
tpm_cli update-task --page-id <TASK_PAGE_ID> --assignee <NOTION_USER_ID>

# Change status
tpm_cli update-task --page-id <TASK_PAGE_ID> --status in_progress

# Change priority
tpm_cli update-task --page-id <TASK_PAGE_ID> --priority p0

# Update multiple fields
tpm_cli update-task --page-id <TASK_PAGE_ID> --assignee <USER_ID> --status review --priority p1

# Soft-delete (archive) a task
tpm_cli update-task --page-id <TASK_PAGE_ID> --status archived
```

#### `list-users` — List Notion workspace users

```bash
tpm_cli list-users
```

Returns JSON with user IDs, names, and emails. Use these IDs for `--assignee` flags.

**Output:**
```json
{
  "success": true,
  "count": 3,
  "users": [
    { "id": "abc-123", "name": "Alice", "email": "alice@example.com" },
    { "id": "def-456", "name": "Bob", "email": "bob@example.com" }
  ]
}
```

#### `sync-tasks` — Bidirectional sync between Notion and MongoDB — LEGACY

> **Note:** This command is no longer used. Tasks are now stored directly in Notion only.

```bash
tpm_cli sync-tasks \
  --organization deeptutor \
  --database-id <NOTION_DATABASE_ID> \
  --workspace-id <WORKSPACE_ID>
```

**Sync operations:**
1. **Notion → MongoDB (existing tasks)**: Sync status/priority updates for tasks linked to both
2. **Notion → MongoDB (new tasks)**: Create MongoDB entry for Notion pages (tasks) without MongoDB ID, link back
3. **Orphan cleanup**: Delete MongoDB tasks whose Notion page was deleted

**Data model (bidirectional linking):**
```
Notion Page                    MongoDB DevTask
┌──────────────────┐          ┌──────────────────┐
│ id: "abc-123"    │◄──────── │ notion_page_id:  │
│                  │          │   "abc-123"      │
│ MongoDB ID:      │─────────►│                  │
│   "507f1f77..."  │          │ _id: 507f1f77... │
└──────────────────┘          └──────────────────┘
```

**Output:**
```json
{
  "success": true,
  "synced": 3,
  "created": 2,
  "skipped": 0,
  "orphans_deleted": 1,
  "errors": []
}
```

#### `setup_tpm_cron` — Set up daily TPM sync cron job for a user

Called directly via `POST /api/tpm/setup-cron` endpoint.

```rust
// scheduler_module/src/tpm_cron.rs
pub fn setup_tpm_cron(
    account_store: &AccountStore,
    user_id: Uuid,
    organization: &str,
    cron_expr: Option<&str>,  // Default: "0 0 9 * * MON-FRI"
) -> Result<SetupTpmCronResult, TpmCronError>
```

Sets up a recurring cron job that triggers Oliver in TPM mode for a user. This directly upserts a `RunTask` into the account's `tasks.db`, bypassing the email pipeline.

**Parameters:**
- `user_id` (required) — User's account UUID (must belong to the organization)
- `organization` (required) — Organization name
- `cron_expr` (optional) — Cron expression (default: `"0 0 9 * * MON-FRI"` = 9 AM UTC weekdays)

**Flow:**
1. Validate user belongs to organization via `AccountStore`
2. Derive user email from verified identifiers in their account
3. Create workspace with synthetic `postmark_payload.json` (subject: "TPM Sync")
4. Build `RunTask` with workspace pointing to TPM mode
5. Upsert into account-level `tasks.db` with cron schedule

**Why synthetic trigger?** When cron fires, Codex reads `postmark_payload.json` and sees subject "TPM Sync", triggering the daily sync workflow per the TPM prompt instructions.

**Why direct upsert?** There is no designated sender or receiver for this cron job, no inbound webhook.

**Cron format:** 6-field expression (second minute hour day-of-month month day-of-week)
- `"0 0 9 * * MON-FRI"` — 9:00 AM UTC, Monday through Friday
- `"0 30 14 * * *"` — 2:30 PM UTC daily
- `"0 0 8 1 * *"` — 8:00 AM UTC on the 1st of each month

### Notion CLI (`notion_api_cli`)

**Already supported:**
- `query-database` — Filter tasks by status, assignee, priority
- `update-page` — Change task status, reassign, update properties
- `create-page` — Create new task (database items are pages in Notion)
- `create-database` — Create a new database with custom schema
- `create-comment` / `reply` — Oliver comments on tasks
- `search` — Find tasks by keyword
- `get-database` — Get board schema/properties

---

## Questions
1. **Transcript access** — How do we get Otter.ai transcripts? (API, email forward, shared folder)
2. **Sentiment Analysis** - Other than DeepTutor discord and direct @Oliver-DoWhiz pings, how does Oliver get a good idea of user sentiment? What apps does Oliver look at?
3. **Go-to-Market** - How does Oliver structure responses? What apps should Oliver post on? Which additional channels to integrate?
4. **Regular Task vs. Organization Task** - Given that an account belongs to an organization, how do we classify tasks (populate into Notion + MongoDB) and regular tasks?

---

## Existing Code to Reuse

| Component | Location | Reuse |
|-----------|----------|-------|
| MongoDB client | `scheduler_module/src/mongo_store.rs` | Connection + CRUD patterns |
| **Dev Task Store** | `scheduler_module/src/dev_task_store.rs` | **LEGACY** - No longer used (was MongoDB CRUD for TPM) |
| **TPM CLI** | `scheduler_module/src/bin/tpm_cli.rs` | **NEW** - Task board commands (setup-board, create-task, list-tasks, sync-tasks) |
| **TPM Cron** | `scheduler_module/src/tpm_cron.rs` | **NEW** - `setup_tpm_cron` function for cron job setup |
| **TPM Skill** | `skills/tpm/SKILL.md` | **NEW** - Detailed workflows for task mgmt, assignment, competitive research |
| Notion CLI | `scheduler_module/src/bin/notion_api_cli.rs` | All page/database operations |
| Notion API Client | `scheduler_module/src/notion_browser/api_client.rs` | `create_database`, `create_database_page`, `query_database` |
| Account lookup | `scheduler_module/src/account_store.rs` | Developer identity + org lookup |
| Queue trait | `scheduler_module/src/ingestion_queue.rs` | Enqueue/claim semantics |
| Task types | `scheduler_module/src/scheduler/types.rs` | TaskKind pattern |
| Supabase accounts | PostgreSQL `accounts` table | Add `organization_id` column |

---

## Progress Log
### 4/21/26
**Completed:**
- ✅ Added `list-users` command — Lists all Notion workspace users (for task assignment)
- ✅ Added `update-task` command — Update existing task's assignee, status, or priority
- ✅ Added "Archived" status option — Soft-delete tasks by setting status to archived
- ✅ Fixed `--assignee` flag — Now uses Notion `people` type (was incorrectly using `rich_text`)
- ✅ Created TPM skill file (`skills/tpm/SKILL.md`) — Comprehensive guide covering:
  - Task management rules (no duplicates, required fields, archive don't delete)
  - Assignment & load balancing workflows
  - Competitive research via web search
  - GitHub → Notion sync patterns
  - Daily TPM sync workflow with report template
- ✅ Added skill reference in `prompt.rs` — Oliver reads `.agents/skills/tpm/SKILL.md` for detailed workflows
- ✅ Fixed `.notion_context.json` and `.notion_env` injection — `tpm_cron.rs` now writes workspace_id and NOTION_API_TOKEN to workspace

###  4/20/26
**Completed:**
- ✅ Completed E2E debugging of manual trigger
- ✅ Fixed incorrect `model` in RunTaskTask, and empty `reply_to` by reading from employee config.
- ✅ Refactor TPM CLIs to only use notion API (no mongoDB bidirectional sync, which can get messy with many corner cases)
- ✅ Pass in organization's `notion_database_id` via `UserIdentities` struct, upsert in TPM prompt in `prompt.rs`

### 4/17/26
**Completed:**
- ✅ Added `sync_user_tasks` to `setup_tpm_cron` — cron tasks now sync to `task_index` immediately so the worker can discover them (previously cron tasks were only in `tasks` collection and would never fire unless another sync happened for the user)

### 4/16/26
**Completed:**
- ✅ Fixed TPM cron user ID mismatch bug — `tpm_cron.rs` now uses `UserStore` to resolve `email_user_id` instead of `account_uuid` for workspace paths and index sync (matches email handler pattern)
- ✅ Cleaned up 170 stale tasks from `task_index` and 141 from `tasks` collection caused by the mismatch

### 4/14/26
**Completed:**
- ✅ Organization-based routing — Supabase `organizations` table, `organization_id` on accounts, gateway routing
- ✅ DevTaskStore (`dev_task_store.rs`) — MongoDB CRUD for DevTask with multi-tenant organization scoping
- ✅ TPM CLI (`tpm_cli.rs`) — Task board commands implemented:
  - `setup-board` — Create Notion database with TPM schema
  - `create-task` — Create task in MongoDB + Notion
  - `list-tasks` — Query tasks with status/assignee filters
  - `sync-tasks` — Pull Notion updates back to MongoDB
- ✅ TPM Cron module (`tpm_cron.rs`) — `setup_tpm_cron` function for daily cron job setup
- ✅ TPM system prompt injection (`prompt.rs`) — Organization-based TPM mode activation
- ✅ Cron job infrastructure — Uses proper Scheduler API (`add_cron_task`) with account-level `tasks.db` storage; synthetic `postmark_payload.json` persists across cron runs (workspace is reused, not recreated)
- ✅ Automatic cron setup — `setup_tpm_cron()` function called via `POST /api/tpm/setup-cron` when user joins organization
- ✅ Organization API endpoints — `POST /auth/organization` (create), `GET /auth/organizations?search=` (list with search), `GET /auth/organization/:name/member-count`
- ✅ Account response includes organization — `GET /auth/account` returns `organization_id` and `organization_name`
- ✅ Frontend organization UI (`website/public/auth/index.html`) — Search, select, join, leave organization flow
- ✅ TPM cron trigger endpoint (`POST /api/tpm/setup-cron`) — Calls `setup_tpm_cron()` function directly when first member joins org

**Remaining:**
- Organization creation UI (frontend)
- Transcript parsing and initial ingestion
- Proactive search for user feedback

---

## Frontend Organization Flow

User joins an organization via the DoWhiz dashboard (`website/public/auth/index.html`).

**UI Components:**
- Current organization display (when joined) with Leave button
- Search input with debounced API calls
- Dropdown showing matching organizations
- Join button (disabled until selection)

**Join Flow:**
```
1. User types in search box
                ↓
2. Debounced (300ms) GET /auth/organizations?search=query
                ↓
3. Dropdown shows results, user clicks one
                ↓
4. Selection highlighted, Join button enabled
                ↓
5. User clicks Join → PUT /auth/account/organization
                ↓
6. GET /auth/organization/:name/member-count
                ↓
7. If member_count === 1:
   → POST /api/tpm/setup-cron { organization_name } to trigger setup_tpm_cron()
   → Show "TPM mode will be set up" message
                ↓
8. UI updates to show current organization
```

**Leave Flow:**
```
1. User clicks Leave → confirmation prompt
                ↓
2. DELETE /auth/account/organization
                ↓
3. UI resets to search mode
```

---

## Notion Token Flow

### Interactive Requests (setup-board, create-task)
User sends first TPM request after connecting organization → uses **user's Notion OAuth token** → database created in **user's workspace** → user owns it.

1. User sends task to Oliver
2. `executor.rs` calls `load_notion_access_token_for_account(account_id)` 
3. `codex.rs` passes token to ACI via `NOTION_API_TOKEN` env var
4. `tpm_cli` reads env var, creates database in user's Notion
5. User shares database with team + Oliver (manual step via Notion UI)

### Cron Job (setup_tpm_cron)
Cron stores **setup user's account_id** → uses **their Notion token** for scheduled syncs.

1. User joins organization → `POST /api/tpm/setup-cron` calls `setup_tpm_cron(account_id, organization)`
2. Task stored in account-level `tasks.db` with `account_id: <UUID>` (the setup user)
3. Cron fires → `resolve_account_for_run_task` returns stored `account_id` via `scheduler.add_cron_task(&cron_expr, RunTaskTask)`
4. `load_notion_access_token_for_account(account_id)` loads setup user's token
5. User's token has access to their own database → sync works

**Note:** No separate "Oliver Notion token" needed for cron. The setup user's token is used since they own the database.

---

## Manual Steps:

1. The first person who joined the organization must share the task board with others; no sharing command is available via Notion API for Oliver to call within the ACI

---

## Error Debugging

### Issue 1: Task Index Sync User ID Mismatch

#### Problem Summary

On 4/16/26, clicking "trigger-sync" caused a 429 rate limit error from CosmosDB due to an explosion of ~170 stale tasks being synced at once.

#### Root Cause: Dual Identity System Mismatch

DoWhiz uses two identity systems:

| Identity | Source | Example | Used By |
|----------|--------|---------|---------|
| `account_id` | AccountStore (Supabase) | `123e4567-e89b-12d3-a456-426614174000` | Auth, billing, organization membership |
| `channel_user_id` | UserStore (MongoDB) | `email_abc123...` | Task scheduling, workspace paths, index sync |

The **email handler** uses `channel_user_id` (email user ID) for:
- Workspace path: `/tmp/users/{email_user_id}/state/tasks.db`
- Index sync: `index_store.sync_user_tasks(&email_user_id, tasks)`
- Tasks collection: `owner_scope.id = email_user_id` (extracted from path)

The **original TPM cron** mistakenly used `account_id` (account UUID) for:
- Workspace path: `/tmp/users/{account_uuid}/state/tasks.db`
- Index sync: `index_store.sync_user_tasks(&account_uuid, tasks)`
- Tasks collection: `owner_scope.id = account_uuid` (extracted from path)

#### Why Tasks Accumulated

1. **setup_tpm_cron** added cron tasks to `{account_uuid}/state/tasks.db` but never synced to `task_index`
2. Each cron fire added more tasks to SQLite (under `account_uuid` path)
3. **trigger_tpm_sync** was first to call `index_store.sync_user_tasks(&account_uuid, tasks)`

#### What Happens When Index Sync Uses a "New" User ID

`sync_user_tasks(user_id, tasks)` performs:
```rust
// 1. Delete all existing tasks for this user_id
DELETE FROM task_index WHERE user_id = ?

// 2. Insert all tasks from the SQLite scheduler
INSERT INTO task_index (user_id, task_id, ...) VALUES ...
```

When `user_id = account_uuid`:
- **Delete phase**: Finds nothing (no prior tasks under this "new" user_id)
- **Insert phase**: Inserts ALL accumulated tasks from the SQLite file

Since the account_uuid path had 170+ accumulated tasks from cron runs, all were synced at once → CosmosDB 429 rate limit.

#### Why Frontend Showed the Tasks

The `tasks` collection uses `owner_scope.id` derived from the path via `resolve_owner_scope()`:

```rust
fn resolve_owner_scope(path: &Path) -> (String, String) {
    // Extracts component after "users/" in the path
    // /tmp/users/{user_id}/state/tasks.db → owner_scope.id = user_id
}
```

Tasks written under `/tmp/users/{account_uuid}/...` had `owner_scope.id = account_uuid`. The frontend queries by `owner_scope.id`, so it found and displayed these tasks.

#### The Fix

Updated `tpm_cron.rs` to use `UserStore` for path resolution (same as email handler):

```rust
// Get email user from UserStore (same pattern as email handler)
let email_user = user_store.get_or_create_user("email", &email)?;

// Use UserStore paths (matches email handler exactly)
let user_paths = user_store.user_paths(&users_root_path, &email_user.user_id);

// Sync using email_user_id
index_store.sync_user_tasks(&email_user.user_id, scheduler.tasks())?;
```

This ensures:
1. `task_index` sync uses `email_user_id` (same as email handler)
2. `tasks` collection's `owner_scope.id` is `email_user_id` (extracted from path)
3. No "new" user_id accumulation → sync cleans up properly
4. Choose email as the user_id source since the channel in RunTaskTask was set to "email".

### Issue 2: Credentials not used correctly by TPM Commands in ACI Container

```
Oliver's response from manual trigger:
What I checked: 

I confirmed the inbound request was a manual TPM sync trigger.
I tried to read the deeptutor task board through tpm_cli.
I tried the Notion CLI and then a direct Notion API fallback using the token already present in the workspace.

Blockers

tpm_cli failed with: mongo config error: MONGODB_URI must be set
notion_api_cli failed with: EMPLOYEE_ID environment variable is required
The direct Notion API fallback did respond, but the linked integration did not have access to any deeptutor task database. The only visible page was the example page The Notion Basics.


What could not be completed

Sync task updates from Notion into the TPM store
List blocked tasks from the synced deeptutor board
Identify stale tasks with no updates in 3+ days
Post the TPM summary to a team Discord/Slack channel
```

### Issue 3: `.notion_context.json` missing workspace_id, `NOTION_API_TOKEN` missing

```
2026-04-21 manual TPM sync reproduced the same runtime issue. In this run-task workspace, tpm_cli could only list the deeptutor board after manually adding .notion_context.json with workspace_id 104000d2-5179-81f2-88c1-0003e11173a9 and exporting NOTION_API_TOKEN from .notion_env. The board itself is reachable directly at database 348000d2-5179-81ee-b159-c1172ddabe29.
```