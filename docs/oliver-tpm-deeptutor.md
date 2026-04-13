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

#### Task Storage (MongoDB)

Extend existing `TaskKind` enum:

```rust
pub enum TaskKind {
    SendReply(SendReplyTask),
    RunTask(RunTaskTask),
    DevTask(DevTask),  // New Task for DeepTutor Development
    Noop,
}

enum Priority { P0, P1, P2, P3 }
enum TaskStatus { Backlog, InProgress, Review, Done, Blocked }
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

### 2. Sentiment Analyzer (User Feedback Pipeline) - Likely Most Difficult

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
└── Done (status = Done, last 2 weeks)
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

## Data Model Additions

### MongoDB Collections

```
deeptutor_tasks          — Task queue and metadata
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
- Add `organizations` table to Supabase
- Add `organization_id` column to `accounts` table
- Update gateway to fetch account's organization and route accordingly
- Frontend: org search + join flow in DoWhiz account settings

### Core Task Queue
- MongoDB collection + CRUD operations ✅ (`dev_task_store.rs`)
- Manual task creation via Oliver
- Assignment notifications
- Notion TPM CLI

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
- `create-task` — Oliver autonomously creates tasks (from user feedback, notetaker, market research)
- `sync-tasks` — Pull status/priority updates that developers made directly in Notion → MongoDB

#### `setup-board` — Create Notion database for an organization

```bash
tpm_cli setup-board \
  --organization deeptutor \
  --parent-page-id <NOTION_PAGE_ID> \
  --workspace-id <WORKSPACE_ID>
```

Creates a Notion database with TPM schema:
- **Name** (title)
- **Status** (select: Backlog, In Progress, Review, Done, Blocked)
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

#### `list-tasks` — List tasks from MongoDB

```bash
# List all tasks
tpm_cli list-tasks --organization deeptutor

# Filter by status
tpm_cli list-tasks --organization deeptutor --status backlog

# Filter by assignee
tpm_cli list-tasks --organization deeptutor --assignee dev@example.com
```

#### `sync-tasks` — Pull developer updates from Notion to MongoDB

```bash
tpm_cli sync-tasks \
  --organization deeptutor \
  --database-id <NOTION_DATABASE_ID> \
  --workspace-id <WORKSPACE_ID>
```

Run before `list-tasks` to pull any status/priority changes developers made directly in Notion. Uses `MongoDB ID` property to match Notion pages to MongoDB documents.

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

## Open Questions
1. **Transcript access** — How to get Otter.ai transcripts? (API, email forward, shared folder)
2. **Sentiment Analysis** - Other than DeepTutor discord and direct @Oliver-DoWhiz pings, how does Oliver get a good idea of user sentiment? What apps does Oliver look at?
3. **Go-to-Market** - How does Oliver structure responses? What apps should Oliver post on? Which additional channels to integrate?

---

## Existing Code to Reuse

| Component | Location | Reuse |
|-----------|----------|-------|
| MongoDB client | `scheduler_module/src/mongo_store.rs` | Connection + CRUD patterns |
| **Dev Task Store** | `scheduler_module/src/dev_task_store.rs` | **NEW** - DevTask CRUD for TPM |
| **TPM CLI** | `scheduler_module/src/bin/tpm_cli.rs` | **NEW** - Task board commands (setup-board, create-task, list-tasks, sync-tasks) |
| Notion CLI | `scheduler_module/src/bin/notion_api_cli.rs` | All page/database operations |
| Notion API Client | `scheduler_module/src/notion_browser/api_client.rs` | `create_database`, `create_database_page`, `query_database` |
| Account lookup | `scheduler_module/src/account_store.rs` | Developer identity + org lookup |
| Queue trait | `scheduler_module/src/ingestion_queue.rs` | Enqueue/claim semantics |
| Task types | `scheduler_module/src/scheduler/types.rs` | TaskKind pattern |
| Supabase accounts | PostgreSQL `accounts` table | Add `organization_id` column |
