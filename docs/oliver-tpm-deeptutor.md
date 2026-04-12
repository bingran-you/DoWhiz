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

#### Routing: Single Pipeline, Different Behavior

Reuse the existing Azure Service Bus inbound gateway. Same Oliver (employee), different behavior based on `tenant_id`:

```toml
# gateway.toml

# DeepTutor dev channel → TPM mode
[[routes]]
channel = "discord"
key = "DEEPTUTOR_DEV_GUILD_ID"
employee_id = "little_bear"
tenant_id = "deeptutor"

# Regular Oliver
[[routes]]
channel = "discord"
key = "*"
employee_id = "little_bear"
tenant_id = "default"
```

**How Oliver differentiates:**
- `tenant_id = "deeptutor"` → TPM mode (manage tasks, assign to humans)
- `tenant_id = "default"` → Regular mode (execute tasks via Codex)

The prompt builder (`prompt.rs`) or workspace setup checks `tenant_id` and injects TPM-specific instructions/skills.

**Why not a separate employee?**
Each `employee_id` is associated with an Azure VM. Would need to create a dedicated VM for the new DeepTutor TPM

**Question:** Should we create a dedicated employee? Should we still use Oliver?

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
    title: String,
    description: String,
    priority: Priority,
    status: TaskStatus,
    assignee: Option<String>,
    source: TaskSource,
    tags: Vec<String>,
    notion_page_id: Option<String>,
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

### Core Task Queue
- MongoDB collection + CRUD operations
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

### Notion CLI

**Already supported:**
- `query-database` — Filter tasks by status, assignee, priority
- `update-page` — Change task status, reassign, update properties
- `create-page` — Create new task (database items are pages in Notion)
- `create-comment` / `reply` — Oliver comments on tasks
- `search` — Find tasks by keyword
- `get-database` — Get board schema/properties

**Potential additions:**
- `create-database-item` — Convenience wrapper for creating tasks with typed properties (priority, status, assignee)
- `get-stale-items` — Query items with no updates in N days

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
| Notion CLI | `scheduler_module/src/bin/notion_api_cli.rs` | All page/database operations |
| Account lookup | `scheduler_module/src/account_store.rs` | Developer identity |
| Queue trait | `scheduler_module/src/ingestion_queue.rs` | Enqueue/claim semantics |
| Task types | `scheduler_module/src/scheduler/types.rs` | TaskKind pattern |
| Channel routing | `gateway.toml` | Multi-channel notifications |
