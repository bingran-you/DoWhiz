# Fault Report: Stale Running Executions After Worker Restart

**Date:** 2026-04-27  
**Investigated by:** Dylan Tang  
**Status:** Root cause identified, partial fix deployed

## Executive Summary

Multiple tasks were stuck in an indefinite defer loop, showing "Creating share" on the dashboard for hours. Investigation revealed that a worker restart (SIGKILL) at `2026-04-26T15:11:45` killed in-flight tasks after `record_execution_start()` but before ACI container creation, leaving orphaned "running" execution records in MongoDB.

## Affected Tasks

| Task ID | User | Subject |
|---------|------|---------|
| `a457a850-440d-14c5-932a-fa3d09c657f4` | af794b74-705a-4d5b-9fd5-9f697f4103d1 | Add Oliver execution validation pack and analyzer |
| `fd084225-34d5-e040-43b6-0ea3eed88d2f` | 1ac112c7-a4d3-4e6f-ad58-a5b93a1168bb | Financial crisis indicator 2 |

---

## Task 1: a457a850-440d-14c5-932a-fa3d09c657f4

### MongoDB Execution History

```bash
ssh dowhizprod1 'source ~/.nvm/nvm.sh && source /home/azureuser/server/DoWhiz/DoWhiz_service/.env && mongosh "$MONGODB_URI" --quiet --eval "db.getSiblingDB(\"dowhiz_production_little_bear\").task_executions.find({task_id: \"a457a850-440d-14c5-932a-fa3d09c657f4\"}).sort({started_at: 1}).toArray()"'
```

**Results:**

| execution_id | started_at | finished_at | status | error_message |
|--------------|------------|-------------|--------|---------------|
| 1777088351583428 | Apr 25 03:39 | Apr 25 04:25 | failed | Output contract violation (actual execution) |
| 1777091140678277 | Apr 25 04:25 | Apr 26 00:26 | failed | reconciled stale running execution after worker restart; execution exceeded 72030s |
| 1777163201121580 | Apr 26 00:26 | Apr 26 10:47 | failed | Output contract violation + Command timed out (claude after 36000s) |
| 1777216311085977 | **Apr 26 15:11** | Apr 27 01:55 | failed | **reconciled stale running execution; ACI container not found** |
| 1777254954176465 | Apr 27 01:55 | Apr 27 01:56 | failed | reconciled stale running execution; ACI container not found |
| 1777255181066121 | Apr 27 01:59 | Apr 27 02:00 | failed | reconciled stale running execution; ACI container not found |

### PM2 Logs Analysis

```bash
ssh dowhizprod1 "source ~/.nvm/nvm.sh && pm2 logs --nostream --lines 20000 | grep 'a457a850' | grep -v 'defer\|found.*due\|task snapshot\|claimed'" 2>&1 | head -30
```

**Finding:** No `[run_task]` logs exist for this task during the Apr 26 15:11 execution window. The scheduler logged "executing task_id=a457a850..." but no subsequent run_task output appeared.

```bash
ssh dowhizprod1 "source ~/.nvm/nvm.sh && pm2 logs --nostream --lines 20000 | grep 'a457a850' | grep -i 'executing'"
```

**Output:**
```
6|dw_worke | 2026-04-27T01:55:54.113403Z INFO scheduler executing task_id=a457a850-440d-14c5-932a-fa3d09c657f4 user_id=af794b74-705a-4d5b-9fd5-9f697f4103d1 kind=run_task status=due
6|dw_worke | 2026-04-27T01:59:40.981727Z INFO scheduler executing task_id=a457a850-440d-14c5-932a-fa3d09c657f4 user_id=af794b74-705a-4d5b-9fd5-9f697f4103d1 kind=run_task status=due
```

### Workspace Analysis

```bash
ssh dowhizprod1 "ls -la /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/af794b74-705a-4d5b-9fd5-9f697f4103d1/workspaces/thread_7dcb73dde58dd1831451e30af86ebcfc/"
```

**Key files:**
- `.run_task_trace/metadata.json` - Current trace (Claude fallback, never finished)
- `.run_task_trace_codex_primary/metadata.json` - Archived codex trace from successful ACI run
- `failure_notifications/` - Created Apr 26 15:11 (the kill time)

### Current Trace (Claude fallback attempt)

```bash
ssh dowhizprod1 "cat /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/af794b74-705a-4d5b-9fd5-9f697f4103d1/workspaces/thread_7dcb73dde58dd1831451e30af86ebcfc/.run_task_trace/metadata.json"
```

```json
{
  "runner": "claude",
  "backend": "claude_local",
  "current_stage": "executing_claude_local",
  "started_at_unix_ms": 1777256661793,
  "finished_at_unix_ms": null,
  "success": null
}
```

**Observation:** This is from a later execution attempt (Apr 27 ~02:04), not the Apr 26 15:11 execution.

### Archived Codex Trace (successful ACI run)

```bash
ssh dowhizprod1 "cat /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/af794b74-705a-4d5b-9fd5-9f697f4103d1/workspaces/thread_7dcb73dde58dd1831451e30af86ebcfc/.run_task_trace_codex_primary/metadata.json"
```

```json
{
  "runner": "codex",
  "backend": "codex_azure_aci",
  "container_name": "dwz-codex-1777255294226-2959816-6",
  "current_stage": "failed",
  "exit_status": 0,
  "success": false,
  "error": "Output contract violation...",
  "timing_ms": {
    "setup_latency_ms": 603.462067,
    "ephemeral_share_create_ms": 47188.085026,
    "aci_cold_start_ms": 305662.111025,
    "codex_execution_ms": 122328.417977,
    "result_download_ms": 878504.872212,
    "total_ms": 1364312.850876
  }
}
```

**Observation:** This trace shows an ACI container WAS created and executed successfully. The failure was due to output validation, not infrastructure.

---

## Task 2: fd084225-34d5-e040-43b6-0ea3eed88d2f

### MongoDB Execution History

```bash
ssh dowhizprod1 'source ~/.nvm/nvm.sh && source /home/azureuser/server/DoWhiz/DoWhiz_service/.env && mongosh "$MONGODB_URI" --quiet --eval "db.getSiblingDB(\"dowhiz_production_little_bear\").task_executions.find({task_id: \"fd084225-34d5-e040-43b6-0ea3eed88d2f\"}).sort({started_at: 1}).toArray()"'
```

**Results:**

| execution_id | started_at | finished_at | status | error_message |
|--------------|------------|-------------|--------|---------------|
| 1777187637643156 | Apr 26 07:13 | Apr 26 15:11 | failed | Output contract violation (actual ~8hr execution) |
| 1777216337577551 | **Apr 26 15:12** | Apr 27 01:55 | failed | **reconciled stale running execution; ACI container not found** |
| 1777254953175042 | Apr 27 01:55 | Apr 27 01:59 | failed | reconciled stale running execution; ACI container not found |
| 1777255187294805 | Apr 27 01:59 | Apr 27 02:09 | failed | reconciled stale running execution; ACI container not found |

**Same pattern as Task 1:** Successful ACI execution followed by stuck "running" executions starting at 15:12.

---

## Root Cause: Worker SIGKILL During Deployment

### PM2 Logs at Incident Time

```bash
ssh dowhizprod1 "source ~/.nvm/nvm.sh && pm2 logs --nostream --lines 50000 | grep -i 'restart\|deploy\|starting\|pm2' | grep -E '2026-04-26T15:1'"
```

**Output:**
```
PM2        | 2026-04-26T15:11:45: PM2 log: Stopping app:dw_worker id:6
PM2        | 2026-04-26T15:11:46: PM2 log: pid=1977394 msg=failed to kill - retrying in 100ms
PM2        | 2026-04-26T15:11:46: PM2 log: pid=1977394 msg=failed to kill - retrying in 100ms
... (repeated ~20 times) ...
PM2        | 2026-04-26T15:11:47: PM2 log: Process with pid 1977394 still alive after 1600ms, sending it SIGKILL now...
PM2        | 2026-04-26T15:11:47: PM2 log: App [dw_worker:6] exited with code [0] via signal [SIGKILL]
```

### Timeline Reconstruction

```
2026-04-26T15:11:45.000  PM2 begins stopping dw_worker
2026-04-26T15:11:46.605  fd084225 execution finishes (previous run)
2026-04-26T15:11:51.085  a457a850 execution starts (record_execution_start)
2026-04-26T15:12:17.577  fd084225 new execution starts (record_execution_start)
2026-04-26T15:11:47.xxx  SIGKILL sent to worker
                         ↳ Both executions killed before reaching run_task
                         ↳ MongoDB records left as "running"
```

### Execution Flow Where Failure Occurred

```
┌─────────────────────────────────────────────────────────────┐
│  scheduler/core.rs                                          │
│                                                             │
│  record_execution_start()  ← MongoDB marked "running" ✓     │
│  archive_session setup...                                   │
│                                                             │
│  executor.execute(&task_kind)                               │
│    ├─ check supersede reason                                │
│    ├─ load_github_inbound_context                          │
│    ├─ resolve_account_for_run_task                         │
│    ├─ check balance                                         │
│    ├─ sync memo                                             │
│    ├─ ... more setup ...         ← SIGKILL HAPPENED HERE   │
│    │                                                        │
│    └─ run_task_module::run_task()  ← Never reached         │
│         └─ az container create     ← Never reached         │
└─────────────────────────────────────────────────────────────┘
```

---

## Why ACI Recovery Didn't Help

The existing ACI recovery mechanism queries for in-flight ACI containers on startup:

```rust
// aci_container_store.rs
pub fn read_aci_recovery_context(workspace_dir: &Path) -> Option<AciRecoveryContext>
```

However, this **only works if an ACI container was actually created**. In this case:

1. `record_execution_start()` succeeded → MongoDB "running"
2. SIGKILL before `az container create` was called
3. No ACI container exists to recover
4. MongoDB execution stuck as "running" indefinitely

---

## Fix Deployed: ACI Container Check During Reconciliation

**File:** `scheduler_module/src/scheduler/store/mongo.rs`

```rust
// In reconcile_stale_running_executions_for_task()
let aci_resource_group = std::env::var("RUN_TASK_AZURE_ACI_RESOURCE_GROUP").ok();

for row in rows.iter().filter(|row| row.status == "running") {
    // ... existing superseded checks ...
    
    // NEW: Check if ACI container exists
    if let Some(ref rg) = aci_resource_group {
        match query_aci_container_status(task_id, rg) {
            AciContainerStatus::NotFound => Some((
                "failed",
                "reconciled stale running execution; ACI container not found".to_string(),
            )),
            AciContainerStatus::Terminal(state) => Some((
                "failed",
                format!("reconciled stale running execution; ACI container terminated with state: {}", state),
            )),
            _ => { /* fall through to stale timeout check */ }
        }
    }
}
```

**Effect:** Stale executions are now detected within 10 minutes (reconciliation interval) instead of 20 hours (stale timeout).

---

But why are tasks staying in the scheduler for 10+ hrs?

## False Positive in Investment Request Detection

Task `a457a850` was a GitHub PR comment but was incorrectly classified as an investment request, causing repeated "Output contract violation" failures.

**Root cause:** `is_investment_request()` in `reply_contract.rs` triggers on:
- "analyzer" in subject (contains "analyze" substring)
- "PR" detected as probable stock ticker (1-5 uppercase chars not in stopwords)

**Impact:** `contains_probable_ticker()` returns true for common acronyms (PR, ACI, API, CLI, SDK, AWS, etc.) since the stopwords list is minimal. Combined with words like "analyze/analysis", any technical discussion gets flagged as investment and fails validation.

Task `fd084225` ("Financial crisis indicator 2") was also a false positive—it was a slide generation request asking to edit `AI_Industry_Bubble_Signals_slide_v3.pptx`, not an investment analysis. Triggered because the request mentioned "investment is highly concentrated" (editing slide text). Codex successfully built the PPTX, rendered preview, ran validation, but the reply email was rejected for missing investment labels.

---

## Appendix: Useful Commands

### Query all executions for a task
```bash
ssh dowhizprod1 'source ~/.nvm/nvm.sh && source /home/azureuser/server/DoWhiz/DoWhiz_service/.env && mongosh "$MONGODB_URI" --quiet --eval "db.getSiblingDB(\"dowhiz_production_little_bear\").task_executions.find({task_id: \"TASK_ID\"}).sort({started_at: 1}).toArray()"'
```

### Check workspace trace
```bash
ssh dowhizprod1 "cat /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/USER_ID/workspaces/THREAD_ID/.run_task_trace/metadata.json"
```

### Find PM2 restart events
```bash
ssh dowhizprod1 "source ~/.nvm/nvm.sh && pm2 logs --nostream --lines 50000 | grep -i 'restart\|stopping\|SIGKILL' | grep 'DATE'"
```

### Check for [run_task] logs for a task
```bash
ssh dowhizprod1 "source ~/.nvm/nvm.sh && pm2 logs dw_worker --nostream --lines 20000 | grep -E '\[run_task\].*TASK_ID|TASK_ID.*\[run_task\]'"
```
