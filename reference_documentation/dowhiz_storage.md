# DoWhiz Storage

## Important Mechanisms:

- All `self` references in core.rs refer to the user scheduler
  - Thus, all operations in core.rs, such as `execute_due_task()` are **locally scoped per user**
- After the SQLite to MongoDB migration, there is no actual "user store". The "user store" are just different documents in MongoDB, queried by the legacy user_path.
- Everything is locally scoped in the user's store. The only synchronization is through the lightweight global `task_index`, which just tells the service (more specifically the **global worker loop**) when a task is due
- The global worker loop reads from the `task_index` and schedules the task to a worker thread
- **Dual-write pattern**: For channels with account linking (Discord, Google Workspace, Lark), tasks must be written twice - once with `legacy_user_id` (for worker execution) and once with `account_id` (for frontend visibility). See "Dual-Write Pattern for Linked Accounts" section below.

---

## Summary: MongoDB Collections

| Collection | Scope | What's Stored |
|------------|-------|---------------|
| tasks | Per-user (owner_scope) | Full task definition, schedule, enabled, retry_count |
| task_executions | Per-user (owner_scope) | Execution history: started_at, finished_at, status, error_message (used for frontend tasksync) |
| task_debug_archives | Per-user (owner_scope) | One row per `task_id + execution_id`, mapping execution to Azure Blob zip (or local fallback path), plus actual storage account/container/blob coordinates and size/checksum/overhead metrics |
| task_index | Global | Lightweight index: user_id, task_id, next_run, enabled |

---

## Different document structures

### tasks collection (per-user scope)
```json
{
  "owner_scope": { "kind": "user", "id": "alice" },
  "task_id": "abc-123",
  "kind": "run_task",
  "channel": "wechat",
  "enabled": true,
  "schedule": { "type": "one_shot", "run_at": "2024-01-01T10:00:00Z" },
  "task_json": "{...full serialized task...}",
  "retry_count": 0
}
```

### task_executions
```json
{
  "owner_scope": {
    "kind": "user",
    "id": "alice"
  },
  "execution_id": 12345,
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "started_at": { "$date": "2026-03-10T14:30:00Z" },
  "finished_at": { "$date": "2026-03-10T14:31:15Z" },
  "status": "success",
  "error_message": null
}
```

### task_debug_archives
```json
{
  "owner_scope": {
    "kind": "user",
    "id": "alice"
  },
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "execution_id": 12345,
  "archive_type": "full_debug_bundle",
  "archive_version": 1,
  "status": "uploaded",
  "storage_backend": "azure_blob",
  "storage_account": "archiveacct",
  "blob_container": "task-debug-archives",
  "blob_path": "task_debug_archives/2026/03/10/550e8400-e29b-41d4-a716-446655440000/12345-v1.zip",
  "blob_reference": "azure://archiveacct/task-debug-archives/task_debug_archives/2026/03/10/550e8400-e29b-41d4-a716-446655440000/12345-v1.zip",
  "local_fallback_path": null,
  "sha256": "<zip sha256>",
  "size_bytes": 482190,
  "duration_ms": 75123,
  "archive_build_duration_ms": 1380,
  "upload_duration_ms": 241,
  "workspace_before_file_count": 18,
  "workspace_after_file_count": 31,
  "redacted_file_count": 4,
  "skipped_file_count": 2,
  "has_run_task_trace": true,
  "has_aci_logs": true,
  "error_summary": null,
  "created_at": { "$date": "2026-03-10T14:31:16Z" }
}
```

Archive bundle contents:
- `manifest.json` and task/execution metadata
- `workspace_before/` and `workspace_after/`
- snapshot manifests + diff
- runtime env allowlist / redacted env inventory / git / tool versions
- `.run_task_trace/` artifacts (prompt, stdout/stderr, assistant tail, token usage, Azure ACI logs)

Redaction policy:
- `.env*`, `.secrets/`, `.auth/`, credential JSON, and private-key-like files are recorded in
  manifests as redacted entries and are not copied into the zip payload

### task_index collection (global)
```json
{
  "user_id": "alice",
  "task_id": "abc-123",
  "next_run": "2024-01-01T10:00:00Z",
  "enabled": true
}
```

---

## Dual-Write Pattern for Linked Accounts (Discord, Google Workspace, Lark)

### The Problem: Two Different Owner Scopes

When a message arrives, the system creates a task. But there are **two different IDs** involved:

1. **`legacy_user_id`** - Created by `user_store.get_or_create_user("discord", sender)`. This is used by the **worker** to execute tasks.
2. **`account_id`** - The unified account ID from `account_store`. This is used by the **frontend** to query tasks.

### Why Two Identities? Channel vs Account

**Channel Identity** (e.g., `email_abc123`, `discord_guild_user`)
- Created automatically when a message arrives from any channel
- Always exists - works even if user hasn't created an account
- Stored at `users_root/{channel_identity}/state/tasks.db`
- Worker polls via `task_index` which references this identity

**Account Identity** (e.g., `550e8400-e29b-41d4-a716-446655440000`)
- Only exists if user has explicitly linked their channel to an Anthropic account
- Stored at `users_root/{account_uuid}/state/tasks.db`
- Frontend Task Center queries tasks by this identity

**The core problem:**
- Someone can email/Discord Oliver without having an account → worker must still process their task
- If they DO have a linked account → task should also appear in their Task Center UI
- These are two different scheduler paths, so we need two writes

### When is Dual-Write Required?

The frontend has **two different endpoints** for fetching tasks, and they check different paths:

| Endpoint | What it checks | Dual-write needed? |
|----------|----------------|-------------------|
| `get_account_routines` | Account path + ALL legacy paths (via `load_unified_account_task_paths` → `legacy_routine_lookup_identifiers`) | **No** - cron jobs are found via legacy path lookup |
| `get_account_tasks` | Account path + Slack legacy paths only | **Yes** - one-shot tasks from email/Discord/Lark/etc. need dual-write |

**Summary:**
- **Cron jobs (routines):** Queried by `get_account_routines`, no dual-write needed. `get_account_routines` already checks all verified identifier paths.
- **One-shot tasks:** Dual-write required for channels other than Slack. `get_account_tasks` only checks the account path and Slack legacy paths.

**TPM Note:** TPM mode uses `email_user_id` (not `account_id`) for worker execution to prevent the worker from loading a new scheduler with {account_id}. But, because we are loading with owner-scope `email_user_id`, and the frontend queries by owner-scope `account_id`, one-shot tasks created by `trigger_tpm_sync` require dual-write to be visible on the frontend dashboard.

**What is owner scope?***
The MongoDB `tasks` collection uses `owner_scope.id` to scope queries. The `owner_scope.id` is derived from the file path:

```rust
// store/mongo.rs:474-504
fn resolve_owner_scope(path: &Path) -> (String, String) {
    // If path is: /users/{owner_id}/state/tasks.db
    // Returns: ("user", owner_id)
}
```

So:
- `Scheduler::load(&user_paths.tasks_db_path)` → `owner_scope.id = legacy_user_id`
- `Scheduler::load(&$USERS_ROOT/{account_id}/state/tasks.db)` → `owner_scope.id = account_id`

### The Solution: Dual-Write

For the frontend to see tasks, channels with account linking (Discord, Google Workspace, Lark) must write the task **twice**, once for the legacy user id and once more for the Supabase account_id:

1. **Legacy storage** (for worker execution): `add_one_shot_in()` generates a new `task_id`
2. **Account storage** (for frontend visibility): `add_one_shot_in_with_id()` uses the same `task_id`

```rust
// Example from discord.rs:228-294

// 1. Clone task before consuming
let run_task_for_account = run_task.clone();

// 2. Build user path with legacy UUID, then insert task in MongoDB from deconstructed path (worker will execute from here)
let mut scheduler = Scheduler::load(&user_paths.tasks_db_path, ...)?;
let task_id = scheduler.add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))?;

// 3. Build user path with Supabase account_id, then insert task in MongoDB from deconstructed path (frontend will query from here)
if let Ok(Some(account)) = account_store.get_account_by_identifier("discord", &message.sender) {
    let user_tasks_db_path = config.users_root
        .join(account.id.to_string())
        .join("state")
        .join("tasks.db");
    let mut user_scheduler = Scheduler::load(&user_tasks_db_path, ...)?;
    user_scheduler.add_one_shot_in_with_id(
        task_id,  // Same task_id!
        Duration::from_secs(0),
        TaskKind::RunTask(run_task_for_account),
    )?;
}
```

### Why Same Task ID?

Both entries must have the **same `task_id`** so that `sync_task_status_to_user_storage()` can update the correct task in account storage after execution:

```rust
// core.rs - after task execution
store.record_execution_start(task_id, ...);   // Updates by task_id
store.record_execution_finish(task_id, ...);  // Updates by task_id
```

### `add_one_shot_in` vs `add_one_shot_in_with_id`

| Method | Task ID | `owner_scope.id` | Purpose |
|--------|---------|------------------|---------|
| `add_one_shot_in` | Generates new `Uuid::new_v4()` | `legacy_user_id` | Worker execution |
| `add_one_shot_in_with_id` | Uses provided `Uuid` | `account_id` | Frontend visibility |

**Important:** Neither method syncs to `task_index` automatically. You must call `index_store.sync_user_tasks(user_id, scheduler.tasks())` after adding tasks so the worker can find them.

### Why Do We Still Need the Legacy User ID?

The **worker** uses the legacy user ID to find and execute tasks. The global worker loop does:

```rust
// task_index stores user_id = legacy_user_id
let task_ref = index_store.due_task_refs(...);  // { user_id: "legacy_user_id", task_id: "..." }

// Worker loads scheduler using legacy_user_id
let tasks_db_path = user_store.user_paths(&config.users_root, &task_ref.user_id).tasks_db_path;
let scheduler = Scheduler::load(&tasks_db_path, ...)?;
scheduler.execute_task_by_id(task_id);
```

So:
- **Legacy write** (`owner_scope.id = legacy_user_id`) → worker can find and execute the task
- **Account write** (`owner_scope.id = account_id`) → frontend can display the task

If we only wrote to account storage, the worker wouldn't find the task to execute because `task_index` references the legacy user ID.

**Note:** Slack is special-cased in `get_account_tasks` - it fetches from both account storage AND Slack legacy storage. Other channels (email, Discord, Lark, etc.) require dual-write for one-shot task visibility.

---

## Two Part Process: In inbound, sync to user scheduler store with `add_one_shot_in` and sync to global task_index with `sync_user_tasks`

### Step 1: Inbound message arrives (e.g., WeChat)
- `process_wechat_event()` in service/inbound/wechat.rs

**Example:**
```rust
// wechat.rs:116-125
let task_id = scheduler.add_one_shot_in(
    Duration::from_secs(0),
    TaskKind::RunTask(run_task)
)?; //syncs user's tasks

index_store.sync_user_tasks(&user.user_id, scheduler.tasks())?;
//syncs task_index

//scheduler.tasks contains tasks from user scheduler (there does not
exist a global scheduler)
```

---

## Part 1: Store to User Task Board

### Step 2: `scheduler.add_one_shot_in()`
- core.rs:90-116

```rust
pub fn add_one_shot_in(&mut self, delay: Duration, kind: TaskKind) ->
Result<Uuid, SchedulerError> {
    let run_at = utc_now + chrono_delay; //chrono_deplay - exponential backoff
    let task = ScheduledTask {
        id: Uuid::new_v4(),
        kind,
        schedule: Schedule::OneShot { run_at },
        enabled: true,
        created_at: now,
        last_run: None,
    };
    self.tasks.push(task);
    self.store.insert_task(self.tasks.last().unwrap())?; //user task store
    Ok(self.tasks.last().unwrap().id)
}
```

### Step 3: `store.insert_task()` - Wrapper
- store/mod.rs:27-29

```rust
pub(crate) fn insert_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
    self.mongo.insert_task(task)
}
```

### Step 4: `mongo.insert_task()` -> MongoDB tasks collection
- store/mongo.rs:99-126

```rust
pub(crate) fn insert_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
    let task_json = serde_json::to_string(task)?;
    self.tasks.update_one(
        self.task_filter(&task.id.to_string()),
        doc! { //!doc is a Rust macro that creates a BSON object (MongoDB's schemaless structure uses BSON)
            "$set": { //set + upsert inserts a document if this document exists, ignores otherwise
                "owner_scope": self.owner_scope_doc(), // { kind: "user", id: user_id }
                "task_id": task.id.to_string(),
                "kind": task_kind_label(&task.kind), // "run_task" or "send_reply"
                "channel": task_kind_channel(&task.kind).to_string(),
                "enabled": task.enabled,
                "created_at": BsonDateTime::from_chrono(task.created_at),
                "last_run": task.last_run,
                "schedule": schedule_doc(&task.schedule),
                "task_json": task_json, // Full serialized task
            },
            "$setOnInsert": {
                "retry_count": 0i32,
            },
        },
    ).with_options(upsert: true).run()?;
}
```

**MongoDB Collection: tasks (user-scoped via owner_scope)**

---

## Part 2: Store to Global Storage

### Step 5: `index_store.sync_user_tasks()` - Wrapper
- index_store/mod.rs:49-55

```rust
pub fn sync_user_tasks(&self, user_id: &str, tasks: &[ScheduledTask]) -> Result<(), IndexStoreError> {
    self.mongo.sync_user_tasks(user_id, tasks)
}
```

### Step 6: `mongo.sync_user_tasks()` -> MongoDB task_index collection
- index_store/mod.rs:97-143

```rust
fn sync_user_tasks(&self, user_id: &str, tasks: &[ScheduledTask]) ->
Result<(), IndexStoreError> {
    let task_rows = enabled_task_next_runs(tasks); // Extract (task_id, next_run) pairs from all tasks in user scheduler

    // Delete tasks no longer in the user scheduler list
    self.task_index.delete_many(doc! {
        "user_id": user_id,
        "task_id": { "$nin": task_ids.clone() },
    }).run()?;

    // Upsert each enabled task
    for (task_id, next_run) in task_rows {
        self.task_index.update_one(
            doc! { "task_id": &task_id, "user_id": user_id } //filter param,
            doc! {
                "$set": { //$set overwrites if exists
                    "next_run": BsonDateTime::from_chrono(next_run),
                    "enabled": true,
                },
                "$setOnInsert": { //$setOnInsert doesn't overwrite if it exists
                    "task_id": &task_id,//task_index is lightweight, mainly keeps track of task_id and user_id
                    "user_id": user_id,
                },
            },
        ).with_options(upsert: true).run()?;
    }
}
```

**MongoDB Collection: task_index (global task store for querying for querying due tasks across users)**

---

### Step 7. Global Worker Loop - in scheduler.rs

```rust
let handle = thread::spawn(move || {
    while !scheduler_stop.load(Ordering::Relaxed) {
        let now = Utc::now();

        // Get due tasks from global task_index
        match index_store.due_task_refs(now, query_limit) {
            Ok(task_refs) => {
                // task_refs = [{ user_id: "alice", task_id: "1" }, ...]

                for (idx, task_ref) in task_refs.into_iter().enumerate() {
                    // Concurrency control
                    if !limiter.try_acquire() { break; }

                    // Claim task (prevent double execution)
                    let claim_result = claims.try_claim(&task_ref, ...);
                    if claim_result != Claimed { continue; }

                    // SPAWN WORKER THREAD to execute
                    thread::spawn(move || {
                        execute_due_task(&config, &user_store, &index_store, &task_ref, ...);
                        claims.release(&task_ref);
                        limiter.release();
                    });
                }
            }
        }
        thread::sleep(scheduler_poll_interval);
    }
});
```

---

## Part 2: Task Execution in Worker Thread

### Step 8. Worker Thread executes the due task

```rust
fn execute_due_task(
    config: &ServiceConfig,
    user_store: &UserStore,
    index_store: &IndexStore,
    task_ref: &TaskRef,              // { user_id: "alice", task_id: "1" }
    running_threads: &Arc<Mutex<HashSet<String>>>,
) -> Result<(), BoxError> {
    let task_id = Uuid::parse_str(&task_ref.task_id)?;

    // 1. Build path for this user for user store scoping
    let tasks_db_path = user_store.user_paths(&config.users_root, &task_ref.user_id)
        .tasks_db_path; // "/users/alice/state/tasks.db"

    // 2. LOAD USER'S SCHEDULER (queries `tasks` collection)
    let mut scheduler = Scheduler::load(&tasks_db_path, ModuleExecutor::default())?;

    // 3. Execute the specific task by ID
    let executed = scheduler.execute_task_by_id(task_id);

    // 4. Handle result and sync back to task_index
    match executed {
        Ok(true) => {
            // Success - sync updated tasks to global index
            index_store.sync_user_tasks(&task_ref.user_id, scheduler.tasks())?;
        }
        Ok(false) => {
            // Task wasn't executed (disabled/not due) - sync anyway
            index_store.sync_user_tasks(&task_ref.user_id, scheduler.tasks())?;
        }
        Err(err) => {
            // Error - still sync
            index_store.sync_user_tasks(&task_ref.user_id, scheduler.tasks())?;
            return Err(err);
        }
    }
}
```

### Step 9: `execute_task_by_id`

```rust
pub fn execute_task_by_id(&mut self, task_id: Uuid) -> Result<bool, SchedulerError> {
    let now = Utc::now();

    // 1. Find task index by UUID
    let index = match self.tasks.iter().position(|task| task.id == task_id) {
        Some(index) => index,
        None => return Ok(false), // Task not found
    };

    // 2. Guard: skip if disabled or not due
    if !self.tasks[index].enabled || !self.tasks[index].is_due(now) {
        return Ok(false);
    }

    // 3. Delegate to workhorse
    self.execute_task_at_index(index)?;
    Ok(true)
}
```

### Step 10: `executes_task_at_index`

```rust
fn execute_task_at_index(&mut self, index: usize) -> Result<(), SchedulerError> {
    let started_at = Utc::now();
    let execution_id = self.store.record_execution_start(task_id, started_at)?; // store task_id in executions DB with status: "running
    let result = self.executor.execute(&task_kind);
    let executed_at = Utc::now();

    match result {
        Ok(execution) => {
            /* success path */
        }
        Err(err) => {
            /* failure path */
        }
    }
}
```

### Step 11: `record_execution_start()` -> MongoDB executions collection
- store/mongo.rs:148-167

```rust
pub(crate) fn record_execution_start(&self, task_id: Uuid, started_at: DateTime<Utc>) -> Result<i64, SchedulerError> {
    let execution_id = EXECUTION_SEQ.fetch_add(1, Ordering::Relaxed);
    self.executions.insert_one(doc! {
        "owner_scope": self.owner_scope_doc(),
        "execution_id": execution_id,
        "task_id": task_id.to_string(),
        "started_at": BsonDateTime::from_chrono(started_at),
        "status": "running",
    }).run()?;
    Ok(execution_id)
}
```

---

## Part 3: Success Path

### Step 12a: On success -> Update task + record finish
- core.rs:246-312

```rust
Ok(execution) => {
    // Reset retry count
    self.store.reset_retry_count(&task_id.to_string())?;

    // Record execution finish
    self.store.record_execution_finish(
        task_id, execution_id, executed_at, "success", None
    )?; //updates scheduler's own store

    // Update task schedule (disable if OneShot, calc next_run if Cron)
    self.store.update_task(&updated_task)?;

    // Sync status to user's account storage
    sync_task_status_to_user_storage(task_id, task, executed_at, "success", None);
}
```

### Step 13a: `record_execution_finish()` -> MongoDB executions
- store/mongo.rs:169-196
- **Scope-agnostic:** Uses whatever `owner_scope` the SchedulerStore was initialized with
- Called twice: once in main flow (legacy scope), once in Step 15a (account scope)

```rust
pub(crate) fn record_execution_finish(
    &self, task_id: Uuid, execution_id: i64, finished_at: DateTime<Utc>,
    status: &str, error_message: Option<&str>
) -> Result<(), SchedulerError> {
    self.executions.update_one(
        doc! {
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,  // <- from SchedulerStore's path
            "task_id": task_id.to_string(),
            "execution_id": execution_id,
        },
        doc! {
            "$set": {
                "finished_at": BsonDateTime::from_chrono(finished_at),
                "status": status,
                "error_message": error_message.unwrap_or(Bson::Null),
            }
        },
    ).run()?;
}
```

### Step 14a: `update_task()` -> MongoDB tasks (legacy_user_id scope only)
- store/mongo.rs:128-146
- **Only called in main flow** with `owner_scope.id` = `legacy_user_id`
- Updates task schedule/enabled state for worker's internal tracking

```rust
pub(crate) fn update_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
    let task_json = serde_json::to_string(task)?;
    self.tasks.update_one(
        self.task_filter(&task.id.to_string()),  // <- filter includes legacy_user_id
        doc! {
            "$set": {
                "enabled": task.enabled, // **false if OneShot completed, updates schedule if cron job**
                "last_run": task.last_run,
                "schedule": schedule_doc(&task.schedule),
                "task_json": task_json,
            }
        },
    ).run()?;
}
```

---

## Part 3b: Frontend Visibility (account_id scope)

> **Key distinction:** The main execution flow (Steps 13a/14a) operates on the worker's SchedulerStore
> which uses `legacy_user_id`. Step 15a opens a **separate** SchedulerStore with `account_id` scope
> and calls the same `record_execution_*` functions to sync status for frontend visibility.

### Step 15a: `sync_task_status_to_user_storage()` -> User's account MongoDB (account_id scope)
- core.rs:424-535

**Important:** This function only **updates execution status** - it does NOT create the task. For the frontend to see the task, it must have been created at ingestion time via the dual-write pattern (see "Dual-Write Pattern for Linked Accounts" above).

```rust
fn sync_task_status_to_user_storage(task_id, task, executed_at, status, error_message) {
    // Look up account_id from linked identifier
    let account_id = lookup_account_by_channel(&task.channel, identifier)?;

    // Path: $USERS_ROOT/{account_id}/state/tasks.db
    let user_tasks_db_path = users_root.join(account_id).join("state").join("tasks.db");

    // Open user's scheduler store (connects to MongoDB with owner_scope.id = account_id)
    let store = SchedulerStore::new(user_tasks_db_path)?;

    // Record execution status to user's executions collection
    // NOTE: This updates status for an EXISTING task - if the task wasn't
    // created with dual-write at ingestion, this won't make it visible to frontend
    let execution_id = store.record_execution_start(task_id, executed_at)?;
    store.record_execution_finish(task_id, execution_id, executed_at, status, error_message)?;
}
```

---

## Part 4: Failure Path

### Step 12b: On failure -> Retry or disable
- core.rs:314-377

```rust
Err(err) => {
    let message = err.to_string();

    // Record failure in scheduler's MongoDB
    self.store.record_execution_finish(
        task_id, execution_id, executed_at, "failed", Some(&message)
    )?;

    // Sync failure to user's account storage
    sync_task_status_to_user_storage(task_id, task, executed_at, "failed", Some(&message));

    // Retry logic for OneShot RunTask
    if retry_count < 3 {
        // Reschedule with delay
        task.schedule.run_at = executed_at + delay;
        self.store.update_task(&updated_task)?; // <- MongoDB update
    } else {
        // Disable task
        task.enabled = false;
        self.store.update_task(&updated_task)?; // <- MongoDB update
        notify_run_task_failure(task_id, task, &message)?;
    }
}
```
This report has been compiled and reviewed by: *dtang04*
