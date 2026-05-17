use chrono::{Duration as ChronoDuration, Utc};
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::{FindOneOptions, FindOptions, UpdateOptions};
use mongodb::sync::{Client, Collection};
use mongodb::IndexModel;
use run_task_module::{
    find_aci_container_by_workspace, query_aci_container_status, AciContainerStatus,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

use crate::account_store::is_global_account_id;
use crate::mongo_store::{
    create_client_from_env, database_from_env, ensure_index_compatible, get_shared_client,
    retry_mongo_read, retry_mongo_write,
};
use crate::thread_state::{current_thread_epoch, default_thread_state_path};

use super::super::task_view::{
    default_routine_name, derive_request_summary, deserialize_task_document,
};
use super::super::types::{Schedule, ScheduledTask, SchedulerError, TaskKind};
use super::super::utils::{task_kind_channel, task_kind_label};
use super::super::{is_user_visible_routine_task, maybe_repair_legacy_weekday_cron_task};
use super::{
    ExecutionReconciliationSummary, ExecutionRecordHandle, RoutineSummary, TaskDebugArchiveRecord,
    TaskExecutionSummary, TaskStatusSummary,
};

static EXECUTION_SEQ: AtomicI64 = AtomicI64::new(0);
const LONG_RUNNING_WARNING_SECS: i64 = 3600;
const DEFAULT_ACI_REGISTRATION_GRACE_SECS: i64 = 15 * 60;
const DEFAULT_ACI_TRACE_ACTIVITY_GRACE_SECS: i64 = 15 * 60;
const DEFAULT_LOCAL_FALLBACK_TIMEOUT_SECS: i64 = 15 * 60;
const CURRENT_TRACE_START_MATCH_TOLERANCE_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone)]
struct ExecutionRow {
    doc_id: Bson,
    task_id: String,
    execution_id: i64,
    started_at: chrono::DateTime<Utc>,
    finished_at: Option<chrono::DateTime<Utc>>,
    status: String,
    error_message: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct RunTaskTraceMetadata {
    backend: String,
    current_stage: String,
    started_at_unix_ms: Option<i64>,
    finished_at_unix_ms: Option<i64>,
    stage_updated_at_unix_ms: Option<i64>,
    success: Option<bool>,
}

#[derive(Debug, Clone, Default)]
struct RunTaskReconciliationContext {
    workspace_dir: Option<String>,
    thread_epoch: Option<u64>,
    thread_state_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UnfinishedFallbackReconciliationAction {
    KeepRunning,
    Fail {
        disable_reason: &'static str,
        error_reason: String,
    },
}

fn next_execution_id(started_at: chrono::DateTime<Utc>) -> i64 {
    let base = started_at.timestamp_micros();
    let mut current = EXECUTION_SEQ.load(Ordering::Relaxed);
    loop {
        let next = base.max(current.saturating_add(1));
        match EXECUTION_SEQ.compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return next,
            Err(observed) => current = observed,
        }
    }
}

#[derive(Debug)]
pub(crate) struct MongoSchedulerStore {
    tasks: Collection<Document>,
    executions: Collection<Document>,
    debug_archives: Collection<Document>,
    owner_kind: String,
    owner_id: String,
}

impl MongoSchedulerStore {
    pub(crate) fn new(tasks_db_path: &Path) -> Result<Self, SchedulerError> {
        let client = create_client_from_env().map_err(mongo_config_err)?;
        Self::with_client(&client, tasks_db_path)
    }

    /// Create a store using the shared MongoDB client singleton.
    /// Use this for hot paths like API request handlers to avoid connection pool exhaustion.
    pub(crate) fn with_shared_client(tasks_db_path: &Path) -> Result<Self, SchedulerError> {
        let client = get_shared_client();
        Self::with_client(client, tasks_db_path)
    }

    fn with_client(client: &Client, tasks_db_path: &Path) -> Result<Self, SchedulerError> {
        let db = database_from_env(client);
        let (owner_kind, owner_id) = resolve_owner_scope(tasks_db_path);
        let tasks = db.collection::<Document>("tasks");
        ensure_index_compatible(
            &tasks,
            IndexModel::builder()
                .keys(doc! { "owner_scope.kind": 1, "owner_scope.id": 1, "task_id": 1 })
                .build(),
        )
        .map_err(mongo_err)?;
        ensure_index_compatible(
            &tasks,
            IndexModel::builder()
                .keys(doc! { "owner_scope.kind": 1, "owner_scope.id": 1, "created_at": 1 })
                .build(),
        )
        .map_err(mongo_err)?;
        let executions = db.collection::<Document>("task_executions");
        ensure_index_compatible(
            &executions,
            IndexModel::builder()
                .keys(doc! {
                    "owner_scope.kind": 1,
                    "owner_scope.id": 1,
                    "task_id": 1,
                    "started_at": -1
                })
                .build(),
        )
        .map_err(mongo_err)?;
        ensure_index_compatible(
            &executions,
            IndexModel::builder()
                .keys(doc! {
                    "owner_scope.kind": 1,
                    "status": 1,
                    "owner_scope.id": 1,
                    "started_at": -1
                })
                .build(),
        )
        .map_err(mongo_err)?;
        let debug_archives = db.collection::<Document>("task_debug_archives");
        ensure_index_compatible(
            &debug_archives,
            IndexModel::builder()
                .keys(doc! {
                    "owner_scope.kind": 1,
                    "owner_scope.id": 1,
                    "task_id": 1,
                    "execution_id": 1
                })
                .options(
                    mongodb::options::IndexOptions::builder()
                        .unique(Some(true))
                        .build(),
                )
                .build(),
        )
        .map_err(mongo_err)?;
        ensure_index_compatible(
            &debug_archives,
            IndexModel::builder()
                .keys(doc! {
                    "owner_scope.kind": 1,
                    "owner_scope.id": 1,
                    "task_id": 1,
                    "created_at": -1
                })
                .build(),
        )
        .map_err(mongo_err)?;
        Ok(Self {
            tasks,
            executions,
            debug_archives,
            owner_kind,
            owner_id,
        })
    }

    pub(crate) fn load_tasks(&self) -> Result<Vec<ScheduledTask>, SchedulerError> {
        let cursor = self
            .tasks
            .find(
                self.owner_filter(),
                FindOptions::builder()
                    .sort(doc! { "created_at": -1 })
                    .build(),
            )
            .map_err(mongo_err)?;
        let mut seen_task_ids = HashSet::new();
        let mut tasks = Vec::new();
        let now = Utc::now();
        for row in cursor {
            let document = row.map_err(mongo_err)?;
            if let Ok(task_id) = document.get_str("task_id") {
                if !seen_task_ids.insert(task_id.to_string()) {
                    continue;
                }
            }
            let mut task = deserialize_task_document(&document)?;
            apply_persisted_task_fields(&document, &mut task)?;
            if maybe_repair_legacy_weekday_cron_task(&mut task, now)? {
                self.update_task(&task)?;
            }
            tasks.push(task);
        }
        tasks.sort_by_key(|task| task.created_at);
        Ok(tasks)
    }

    pub(crate) fn load_task_by_id(
        &self,
        task_id: &str,
    ) -> Result<Option<ScheduledTask>, SchedulerError> {
        let document = self
            .tasks
            .find_one(self.task_filter(task_id), None)
            .map_err(mongo_err)?;
        let Some(document) = document else {
            return Ok(None);
        };

        let mut task = deserialize_task_document(&document)?;
        apply_persisted_task_fields(&document, &mut task)?;
        if maybe_repair_legacy_weekday_cron_task(&mut task, Utc::now())? {
            self.update_task(&task)?;
        }
        Ok(Some(task))
    }

    pub(crate) fn insert_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
        let task_json = serde_json::to_string(task)
            .map_err(|err| SchedulerError::Storage(format!("serialize task failed: {err}")))?;
        let filter = self.task_filter(&task.id.to_string());
        let update = doc! {
            "$set": {
                "owner_scope": self.owner_scope_doc(),
                "task_id": task.id.to_string(),
                "kind": task_kind_label(&task.kind),
                "channel": task_kind_channel(&task.kind).to_string(),
                "enabled": task.enabled,
                "created_at": BsonDateTime::from_chrono(task.created_at),
                "last_run": task.last_run.map(BsonDateTime::from_chrono).map(Bson::DateTime).unwrap_or(Bson::Null),
                "schedule": schedule_doc(&task.schedule),
                "task_json": task_json,
            },
            "$setOnInsert": {
                "retry_count": 0i32,
            },
        };
        let options = UpdateOptions::builder().upsert(Some(true)).build();
        let result = retry_mongo_write("tasks.insert_task", || {
            self.tasks
                .update_one(filter.clone(), update.clone(), options.clone())
        })
        .map_err(mongo_err)?;

        tracing::info!(
            "insert_task: task_id={} owner_scope=({}, {}) upserted={} matched={}",
            task.id,
            self.owner_kind,
            self.owner_id,
            result.upserted_id.is_some(),
            result.matched_count
        );
        Ok(())
    }

    pub(crate) fn update_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
        self.update_task_internal(task, false)
    }

    pub(crate) fn replace_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
        self.update_task_internal(task, true)
    }

    fn update_task_internal(
        &self,
        task: &ScheduledTask,
        allow_reenable_and_clear_auto_disable: bool,
    ) -> Result<(), SchedulerError> {
        let task_json = serde_json::to_string(task)
            .map_err(|err| SchedulerError::Storage(format!("serialize task failed: {err}")))?;
        let mut filter = doc! { "task_id": task.id.to_string() };
        if task.enabled && !allow_reenable_and_clear_auto_disable {
            filter.insert(
                "$or",
                Bson::Array(vec![
                    Bson::Document(doc! { "enabled": { "$ne": false } }),
                    Bson::Document(doc! { "auto_disabled_reason": { "$exists": false } }),
                    Bson::Document(doc! { "auto_disabled_reason": Bson::Null }),
                    Bson::Document(doc! { "auto_disabled_reason": "" }),
                ]),
            );
        }
        let mut update = doc! {
            "$set": {
                "enabled": task.enabled,
                "last_run": task.last_run.map(BsonDateTime::from_chrono).map(Bson::DateTime).unwrap_or(Bson::Null),
                "schedule": schedule_doc(&task.schedule),
                "task_json": task_json,
            },
        };
        if allow_reenable_and_clear_auto_disable {
            update.insert(
                "$unset",
                Bson::Document(doc! {
                    "auto_disabled_reason": "",
                    "auto_disabled_at": "",
                }),
            );
        }
        let result = retry_mongo_write("tasks.update_task", || {
            self.tasks.update_many(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;

        // Task documents are mirrored across owner scopes by shared task_id.
        // Updating all matching copies keeps account/user summaries consistent.
        if result.matched_count == 0 {
            tracing::warn!(
                "update_task matched 0 documents! task_id={} owner_scope=({}, {}) filter={:?}",
                task.id,
                self.owner_kind,
                self.owner_id,
                filter
            );
        } else {
            tracing::debug!(
                "update_task succeeded: task_id={} matched={} modified={} enabled={} force_replace={}",
                task.id,
                result.matched_count,
                result.modified_count,
                task.enabled,
                allow_reenable_and_clear_auto_disable
            );
        }
        Ok(())
    }

    /// Disable a task by ID to prevent further executions.
    ///
    /// Used by reconciliation to break infinite retry loops when a task
    /// repeatedly fails before ACI container creation.
    pub(crate) fn disable_task_by_id(
        &self,
        task_id: &str,
        reason: &str,
    ) -> Result<(), SchedulerError> {
        let filter = doc! { "task_id": task_id };
        let update = doc! {
            "$set": {
                "enabled": false,
                "auto_disabled_reason": reason,
                "auto_disabled_at": BsonDateTime::from_chrono(Utc::now()),
            }
        };
        let result = retry_mongo_write("tasks.disable_task_by_id", || {
            self.tasks.update_many(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;

        if result.matched_count == 0 {
            let message = format!(
                "disable_task_by_id matched 0 documents: task_id={} owner_scope=({}, {})",
                task_id, self.owner_kind, self.owner_id
            );
            tracing::warn!("{message}");
            return Err(SchedulerError::Storage(message));
        }
        tracing::info!(
            "disable_task_by_id succeeded: task_id={} reason={} matched={}",
            task_id,
            reason,
            result.matched_count
        );
        Ok(())
    }

    /// Check if there's already a running execution for this task.
    ///
    /// This prevents duplicate executions when the worker process restarts
    /// and loses its in-memory claims state.
    pub(crate) fn has_running_execution(&self, task_id: &str) -> Result<bool, SchedulerError> {
        let count = retry_mongo_read("task_executions.has_running_execution", || {
            self.executions.count_documents(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": task_id,
                    "status": "running",
                },
                None,
            )
        })
        .map_err(mongo_err)?;
        Ok(count > 0)
    }

    pub(crate) fn record_execution_start(
        &self,
        task_id: Uuid,
        started_at: chrono::DateTime<Utc>,
    ) -> Result<ExecutionRecordHandle, SchedulerError> {
        let execution = ExecutionRecordHandle {
            execution_id: next_execution_id(started_at),
            started_at,
        };
        let document = doc! {
            "owner_scope": self.owner_scope_doc(),
            "execution_id": execution.execution_id,
            "task_id": task_id.to_string(),
            "started_at": BsonDateTime::from_chrono(started_at),
            "finished_at": Bson::Null,
            "status": "running",
            "error_message": Bson::Null,
        };
        retry_mongo_write("task_executions.record_execution_start", || {
            self.executions.insert_one(document.clone(), None)
        })
        .map_err(mongo_err)?;
        Ok(execution)
    }

    pub(crate) fn record_execution_finish(
        &self,
        task_id: Uuid,
        execution: ExecutionRecordHandle,
        finished_at: chrono::DateTime<Utc>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        let filter = doc! {
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
            "task_id": task_id.to_string(),
            "execution_id": execution.execution_id,
            "started_at": BsonDateTime::from_chrono(execution.started_at),
            "status": "running",
        };
        let update = doc! {
            "$set": {
                "finished_at": BsonDateTime::from_chrono(finished_at),
                "status": status,
                "error_message": error_message.map(Bson::from).unwrap_or(Bson::Null),
            }
        };
        let result = retry_mongo_write("task_executions.record_execution_finish", || {
            self.executions
                .update_one(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;
        if result.matched_count == 0 {
            let existing = self.find_execution_row(
                &task_id.to_string(),
                execution.execution_id,
                execution.started_at,
            )?;
            return match existing {
                Some(row) if row.status != "running" => {
                    if should_replace_stale_reconciliation_terminal_row(&row, status, error_message)
                    {
                        self.overwrite_terminal_execution_row(
                            &row.doc_id,
                            finished_at,
                            status,
                            error_message,
                        )?;
                        if should_clear_auto_disabled_reason_after_stale_replacement(
                            status,
                            error_message,
                        ) {
                            self.clear_auto_disabled_reason(task_id)?;
                        }
                        tracing::info!(
                            "record_execution_finish replaced stale terminal row for task {} execution_id={} old_status={} new_status={}",
                            task_id,
                            execution.execution_id,
                            row.status,
                            status
                        );
                        return Ok(());
                    }
                    tracing::warn!(
                        "record_execution_finish observed terminal row already written for task {} execution_id={} status={}",
                        task_id,
                        execution.execution_id,
                        row.status
                    );
                    Ok(())
                }
                _ => Err(SchedulerError::Storage(format!(
                    "missing running execution row for task {} execution_id={} started_at={}",
                    task_id,
                    execution.execution_id,
                    execution.started_at.to_rfc3339()
                ))),
            };
        }
        Ok(())
    }

    fn overwrite_terminal_execution_row(
        &self,
        doc_id: &Bson,
        finished_at: chrono::DateTime<Utc>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        let filter = doc! {
            "_id": doc_id.clone(),
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
        };
        let update = doc! {
            "$set": {
                "finished_at": BsonDateTime::from_chrono(finished_at),
                "status": status,
                "error_message": error_message.map(Bson::from).unwrap_or(Bson::Null),
            }
        };
        retry_mongo_write("task_executions.overwrite_terminal_execution_row", || {
            self.executions
                .update_one(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;
        Ok(())
    }

    fn clear_auto_disabled_reason(&self, task_id: Uuid) -> Result<(), SchedulerError> {
        let filter = doc! { "task_id": task_id.to_string() };
        let update = doc! {
            "$unset": {
                "auto_disabled_reason": "",
                "auto_disabled_at": "",
            }
        };
        retry_mongo_write("tasks.clear_auto_disabled_reason", || {
            self.tasks.update_many(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;
        Ok(())
    }

    pub(crate) fn upsert_terminal_execution(
        &self,
        task_id: Uuid,
        execution: ExecutionRecordHandle,
        finished_at: chrono::DateTime<Utc>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        let filter = doc! {
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
            "task_id": task_id.to_string(),
            "execution_id": execution.execution_id,
        };
        let update = doc! {
            "$set": {
                "owner_scope": self.owner_scope_doc(),
                "execution_id": execution.execution_id,
                "task_id": task_id.to_string(),
                "started_at": BsonDateTime::from_chrono(execution.started_at),
                "finished_at": BsonDateTime::from_chrono(finished_at),
                "status": status,
                "error_message": error_message.map(Bson::from).unwrap_or(Bson::Null),
            }
        };
        let options = UpdateOptions::builder().upsert(Some(true)).build();
        retry_mongo_write("task_executions.upsert_terminal_execution", || {
            self.executions
                .update_one(filter.clone(), update.clone(), options.clone())
        })
        .map_err(mongo_err)?;
        Ok(())
    }

    /// Get the run_task reconciliation context from a task's task_json field.
    /// Returns None if task not found or task_json cannot be parsed.
    fn get_task_run_task_context(&self, task_id: &str) -> Option<RunTaskReconciliationContext> {
        let filter = doc! {
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
            "task_id": task_id,
        };
        let doc = retry_mongo_read("tasks.find_one_for_workspace", || {
            self.tasks.find_one(filter.clone(), None)
        })
        .ok()??;

        let task_json = doc.get_str("task_json").ok()?;
        let parsed: serde_json::Value = serde_json::from_str(task_json).ok()?;
        let kind = parsed.get("kind")?;
        Some(RunTaskReconciliationContext {
            workspace_dir: kind
                .get("workspace_dir")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string()),
            thread_epoch: kind.get("thread_epoch").and_then(|value| value.as_u64()),
            thread_state_path: kind
                .get("thread_state_path")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string()),
        })
    }

    fn find_task_ids_for_workspace(
        &self,
        workspace_dir: &str,
    ) -> Result<Vec<String>, SchedulerError> {
        let filter = doc! {
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
        };
        let cursor = retry_mongo_read("tasks.find_for_workspace_lookup", || {
            self.tasks.find(filter.clone(), None)
        })
        .map_err(mongo_err)?;

        let mut matching_task_ids = Vec::new();
        for doc_result in cursor {
            let doc = doc_result.map_err(mongo_err)?;
            let Ok(task_json) = doc.get_str("task_json") else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(task_json) else {
                continue;
            };
            let Some(ws) = parsed
                .get("kind")
                .and_then(|k| k.get("workspace_dir"))
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            if ws == workspace_dir {
                if let Ok(task_id) = doc.get_str("task_id") {
                    matching_task_ids.push(task_id.to_string());
                }
            }
        }
        Ok(matching_task_ids)
    }

    pub(crate) fn reconcile_stale_running_executions(
        &self,
        now: chrono::DateTime<Utc>,
        stale_after: ChronoDuration,
    ) -> Result<ExecutionReconciliationSummary, SchedulerError> {
        let task_ids = retry_mongo_read("task_executions.distinct_running_task_ids", || {
            self.executions.distinct(
                "task_id",
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "status": "running",
                },
                None,
            )
        })
        .map_err(mongo_err)?;

        let mut summary = ExecutionReconciliationSummary::default();
        for task_id in task_ids {
            let Some(task_id) = task_id.as_str() else {
                continue;
            };
            summary.merge(self.reconcile_stale_running_executions_for_task(
                task_id,
                now,
                stale_after,
            )?);
        }
        Ok(summary)
    }

    pub(crate) fn reconcile_stale_running_executions_for_task(
        &self,
        task_id: &str,
        now: chrono::DateTime<Utc>,
        stale_after: ChronoDuration,
    ) -> Result<ExecutionReconciliationSummary, SchedulerError> {
        let rows = self.load_execution_rows_for_task(task_id)?;
        if rows.iter().all(|row| row.status != "running") {
            return Ok(ExecutionReconciliationSummary::default());
        }

        let latest_started_at = rows.first().map(|row| row.started_at);
        let latest_terminal_finished_at = rows
            .iter()
            .filter(|row| row.status != "running")
            .filter_map(|row| row.finished_at)
            .max();
        let stale_before = now - stale_after;
        let stale_timeout_secs = stale_after.num_seconds();
        let aci_resource_group = std::env::var("RUN_TASK_AZURE_ACI_RESOURCE_GROUP").ok();
        let run_task_context = self.get_task_run_task_context(task_id);
        let workspace_dir = run_task_context
            .as_ref()
            .and_then(|context| context.workspace_dir.clone());
        let workspace_path = workspace_dir.as_deref().map(Path::new);
        let thread_supersede_reason = run_task_context
            .as_ref()
            .and_then(current_thread_supersede_reason);
        let workspace_peer_rows = if let Some(workspace_dir) = workspace_dir.as_deref() {
            let task_ids = self.find_task_ids_for_workspace(workspace_dir)?;
            let task_refs: Vec<&str> = task_ids.iter().map(String::as_str).collect();
            self.load_execution_rows_for_tasks(&task_refs)?
        } else {
            HashMap::new()
        };

        let mut summary = ExecutionReconciliationSummary::default();
        for row in rows.iter().filter(|row| row.status == "running") {
            let unfinished_fallback_action = unfinished_fallback_reconciliation_action(
                workspace_path,
                row.started_at,
                now,
                stale_before,
            );
            let inflight_aci_result_handling = workspace_path
                .map(|workspace| {
                    workspace_suggests_recent_inflight_aci_result_handling(
                        workspace,
                        row.started_at,
                        now,
                    )
                })
                .unwrap_or(false);
            let action = if latest_terminal_finished_at
                .map(|finished_at| row.started_at <= finished_at)
                .unwrap_or(false)
            {
                Some((
                    "superseded",
                    format!(
                        "reconciled stale running execution after a later execution completed at {}",
                        latest_terminal_finished_at
                            .expect("checked above")
                            .to_rfc3339()
                    ),
                ))
            } else if latest_started_at
                .map(|started_at| row.started_at < started_at)
                .unwrap_or(false)
            {
                Some((
                    "superseded",
                    format!(
                        "reconciled stale running execution after a newer execution started at {}",
                        latest_started_at.expect("checked above").to_rfc3339()
                    ),
                ))
            } else if let Some(reason) = thread_supersede_reason.as_ref() {
                self.auto_disable_terminal_action(
                    task_id,
                    "superseded",
                    "auto-disabled: task was superseded by a newer follow-up",
                    format!("reconciled stale running execution because {}", reason),
                )
            } else if let Some(reason) = workspace_peer_supersede_reason(
                task_id,
                &workspace_peer_rows,
                workspace_path,
                row.started_at,
            ) {
                self.auto_disable_terminal_action(
                    task_id,
                    "superseded",
                    "auto-disabled: duplicate task was superseded by another run in the same thread workspace",
                    format!("reconciled stale running execution because {}", reason),
                )
            } else if let Some(ref rg) = aci_resource_group {
                let container_record = workspace_dir
                    .as_ref()
                    .and_then(|ws| find_aci_container_by_workspace(ws));

                match container_record {
                    Some(record) => {
                        // Container found in registry, check its actual Azure status
                        match query_aci_container_status(&record.container_name, rg) {
                            AciContainerStatus::NotFound => {
                                match unfinished_fallback_action.as_ref() {
                                    Some(UnfinishedFallbackReconciliationAction::KeepRunning) => {
                                        tracing::info!(
                                            "leaving task {} running because primary ACI is gone but local fallback is still active",
                                            task_id
                                        );
                                        None
                                    }
                                    None if inflight_aci_result_handling => {
                                        tracing::info!(
                                            "leaving task {} running because local ACI result handling is still active after Azure no longer reported the container",
                                            task_id
                                        );
                                        None
                                    }
                                    Some(UnfinishedFallbackReconciliationAction::Fail {
                                        disable_reason,
                                        error_reason,
                                    }) => self.auto_disable_failed_action(
                                        task_id,
                                        disable_reason,
                                        error_reason.clone(),
                                    ),
                                    None => self.auto_disable_failed_action(
                                        task_id,
                                        "auto-disabled: ACI container was registered but no longer exists in Azure",
                                        "reconciled stale running execution; ACI container was registered but no longer exists in Azure".to_string(),
                                    ),
                                }
                            }
                            AciContainerStatus::Terminal(state) => {
                                if state.eq_ignore_ascii_case("Succeeded") {
                                    // Container succeeded - let ACI recovery handle it
                                    // ACI recovery will: download output, send outbound, mark success
                                    tracing::info!(
                                        "ACI container for task {} succeeded, deferring to ACI recovery",
                                        task_id
                                    );
                                    None
                                } else {
                                    match unfinished_fallback_action.as_ref() {
                                        Some(UnfinishedFallbackReconciliationAction::KeepRunning) => {
                                            tracing::info!(
                                                "leaving task {} running because primary ACI terminated with state={} but local fallback is still active",
                                                task_id,
                                                state
                                            );
                                            None
                                        }
                                        None if inflight_aci_result_handling => {
                                            tracing::info!(
                                                "leaving task {} running because primary ACI terminated with state={} while local result handling is still active",
                                                task_id,
                                                state
                                            );
                                            None
                                        }
                                        Some(UnfinishedFallbackReconciliationAction::Fail {
                                            disable_reason,
                                            error_reason,
                                        }) => self.auto_disable_failed_action(
                                            task_id,
                                            disable_reason,
                                            error_reason.clone(),
                                        ),
                                        None => self.auto_disable_failed_action(
                                            task_id,
                                            &format!(
                                                "auto-disabled: ACI container terminated with state: {}",
                                                state
                                            ),
                                            format!(
                                                "reconciled stale running execution; ACI container terminated with state: {}",
                                                state
                                            ),
                                        ),
                                    }
                                }
                            }
                            AciContainerStatus::Running => {
                                tracing::debug!(
                                    "ACI container for task {} still running; leaving execution row as running",
                                    task_id
                                );
                                None
                            }
                            AciContainerStatus::Error(err) => {
                                if matches!(
                                    unfinished_fallback_action,
                                    Some(UnfinishedFallbackReconciliationAction::KeepRunning)
                                ) {
                                    tracing::info!(
                                        "ignoring ACI status query error for task {} because local fallback is still active: {}",
                                            task_id,
                                            err
                                        );
                                    None
                                } else if inflight_aci_result_handling {
                                    tracing::info!(
                                        "ignoring ACI status query error for task {} because local result handling is still active: {}",
                                        task_id,
                                        err
                                    );
                                    None
                                } else if row.started_at <= stale_before {
                                    Some((
                                        "failed",
                                        format!(
                                            "reconciled stale running execution after worker restart; ACI status query kept failing for {}s: {}",
                                            stale_timeout_secs,
                                            err
                                        ),
                                    ))
                                } else {
                                    None
                                }
                            }
                        }
                    }
                    None => {
                        // No container registered for this workspace
                        // Either: never created (danger zone) or already deregistered (completed)
                        let aci_grace_period = resolve_aci_registration_grace_period();
                        let execution_age = now - row.started_at;

                        match unfinished_fallback_action.as_ref() {
                            Some(UnfinishedFallbackReconciliationAction::KeepRunning) => {
                                tracing::info!(
                                    "leaving task {} running because ACI registry is gone but local fallback is still active",
                                    task_id
                                );
                                None
                            }
                            None if inflight_aci_result_handling => {
                                tracing::info!(
                                    "leaving task {} running because ACI registry is gone while local result handling is still active",
                                    task_id
                                );
                                None
                            }
                            Some(UnfinishedFallbackReconciliationAction::Fail {
                                disable_reason,
                                error_reason,
                            }) => self.auto_disable_failed_action(
                                task_id,
                                disable_reason,
                                error_reason.clone(),
                            ),
                            None if execution_age > aci_grace_period => {
                                let (disable_reason, error_reason) =
                                    missing_aci_registry_reconciliation_reason(
                                        workspace_path,
                                        row.started_at,
                                        now,
                                    );
                                self.auto_disable_failed_action(
                                    task_id,
                                    disable_reason,
                                    error_reason,
                                )
                            }
                            None => {
                                // Within grace period - might still be uploading ephemeral share
                                tracing::debug!(
                                    "ACI container not found for task {} but within grace period ({} < {}), skipping",
                                    task_id,
                                    execution_age,
                                    aci_grace_period
                                );
                                None
                            }
                        }
                    }
                }
            } else if row.started_at <= stale_before {
                Some((
                    "failed",
                    format!(
                        "reconciled stale running execution after worker restart; execution exceeded {}s without a terminal status",
                        stale_timeout_secs
                    ),
                ))
            } else {
                None
            };

            let Some((status, reason)) = action else {
                continue;
            };

            self.finish_execution_row(&row.doc_id, now, status, Some(&reason))?;
            match status {
                "superseded" => summary.superseded_count += 1,
                "failed" => summary.failed_count += 1,
                _ => {}
            }
        }
        Ok(summary)
    }

    fn auto_disable_failed_action(
        &self,
        task_id: &str,
        disable_reason: &str,
        error_reason: String,
    ) -> Option<(&'static str, String)> {
        self.auto_disable_terminal_action(task_id, "failed", disable_reason, error_reason)
    }

    fn auto_disable_terminal_action(
        &self,
        task_id: &str,
        terminal_status: &'static str,
        disable_reason: &str,
        terminal_note: String,
    ) -> Option<(&'static str, String)> {
        if let Err(err) = self.disable_task_by_id(task_id, disable_reason) {
            tracing::error!(
                "failed to disable task {} during stale reconciliation: {}",
                task_id,
                err
            );
            None
        } else {
            Some((terminal_status, terminal_note))
        }
    }

    pub(crate) fn record_task_debug_archive(
        &self,
        archive: &TaskDebugArchiveRecord,
    ) -> Result<(), SchedulerError> {
        let filter = doc! {
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
            "task_id": &archive.task_id,
            "execution_id": archive.execution_id,
        };
        let update = doc! {
            "$set": {
                "owner_scope": self.owner_scope_doc(),
                "task_id": &archive.task_id,
                "execution_id": archive.execution_id,
                "archive_type": &archive.archive_type,
                "archive_version": archive.archive_version,
                "status": &archive.status,
                "storage_backend": &archive.storage_backend,
                "storage_account": archive.storage_account.as_deref().map(Bson::from).unwrap_or(Bson::Null),
                "blob_container": archive.blob_container.as_deref().map(Bson::from).unwrap_or(Bson::Null),
                "blob_path": archive.blob_path.as_deref().map(Bson::from).unwrap_or(Bson::Null),
                "blob_reference": archive.blob_reference.as_deref().map(Bson::from).unwrap_or(Bson::Null),
                "local_fallback_path": archive.local_fallback_path.as_deref().map(Bson::from).unwrap_or(Bson::Null),
                "sha256": &archive.sha256,
                "size_bytes": archive.size_bytes,
                "runner": &archive.runner,
                "model": &archive.model,
                "deploy_target": &archive.deploy_target,
                "started_at": BsonDateTime::from_chrono(archive.started_at),
                "finished_at": BsonDateTime::from_chrono(archive.finished_at),
                "duration_ms": archive.duration_ms,
                "archive_build_duration_ms": archive.archive_build_duration_ms,
                "upload_duration_ms": archive.upload_duration_ms,
                "workspace_before_file_count": archive.workspace_before_file_count,
                "workspace_after_file_count": archive.workspace_after_file_count,
                "redacted_file_count": archive.redacted_file_count,
                "skipped_file_count": archive.skipped_file_count,
                "has_workspace_before": archive.has_workspace_before,
                "has_workspace_after": archive.has_workspace_after,
                "has_run_task_trace": archive.has_run_task_trace,
                "has_aci_logs": archive.has_aci_logs,
                "error_summary": archive.error_summary.as_deref().map(Bson::from).unwrap_or(Bson::Null),
                "created_at": BsonDateTime::from_chrono(archive.created_at),
            }
        };
        let options = UpdateOptions::builder().upsert(Some(true)).build();
        retry_mongo_write("task_debug_archives.record_task_debug_archive", || {
            self.debug_archives
                .update_one(filter.clone(), update.clone(), options.clone())
        })
        .map_err(mongo_err)?;
        Ok(())
    }

    pub(crate) fn append_execution_event(
        &self,
        task_id: &str,
        started_at: chrono::DateTime<Utc>,
        finished_at: Option<chrono::DateTime<Utc>>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        self.executions
            .insert_one(
                doc! {
                    "owner_scope": self.owner_scope_doc(),
                    "execution_id": next_execution_id(started_at),
                    "task_id": task_id,
                    "started_at": BsonDateTime::from_chrono(started_at),
                    "finished_at": finished_at.map(BsonDateTime::from_chrono).map(Bson::DateTime).unwrap_or(Bson::Null),
                    "status": status,
                    "error_message": error_message.map(Bson::from).unwrap_or(Bson::Null),
                },
                None,
            )
            .map_err(mongo_err)?;
        Ok(())
    }

    fn load_execution_rows_for_task(
        &self,
        task_id: &str,
    ) -> Result<Vec<ExecutionRow>, SchedulerError> {
        let cursor = self
            .executions
            .find(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": task_id,
                },
                FindOptions::builder()
                    .sort(doc! { "started_at": -1 })
                    .build(),
            )
            .map_err(mongo_err)?;

        let mut rows = Vec::new();
        for row in cursor {
            rows.push(parse_execution_row(row.map_err(mongo_err)?)?);
        }
        Ok(rows)
    }

    /// Batch fetch executions for multiple task_ids in one query.
    /// Returns a HashMap keyed by task_id for O(1) lookup.
    fn load_execution_rows_for_tasks(
        &self,
        task_ids: &[&str],
    ) -> Result<HashMap<String, Vec<ExecutionRow>>, SchedulerError> {
        if task_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let cursor = self
            .executions
            .find(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": { "$in": task_ids },
                },
                FindOptions::builder()
                    .sort(doc! { "started_at": -1 })
                    .build(),
            )
            .map_err(mongo_err)?;

        let mut map: HashMap<String, Vec<ExecutionRow>> = HashMap::new();
        for row in cursor {
            let exec_row = parse_execution_row(row.map_err(mongo_err)?)?;
            map.entry(exec_row.task_id.clone())
                .or_default()
                .push(exec_row);
        }
        Ok(map)
    }

    fn finish_execution_row(
        &self,
        doc_id: &Bson,
        finished_at: chrono::DateTime<Utc>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        let filter = doc! {
            "_id": doc_id.clone(),
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
            "status": "running",
        };
        let update = doc! {
            "$set": {
                "finished_at": BsonDateTime::from_chrono(finished_at),
                "status": status,
                "error_message": error_message.map(Bson::from).unwrap_or(Bson::Null),
            }
        };
        let result = retry_mongo_write("task_executions.finish_execution_row", || {
            self.executions
                .update_one(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;
        if result.matched_count == 0 {
            let document = retry_mongo_read("task_executions.finish_execution_row.lookup", || {
                self.executions.find_one(
                    doc! {
                        "_id": doc_id.clone(),
                        "owner_scope.kind": &self.owner_kind,
                        "owner_scope.id": &self.owner_id,
                    },
                    None,
                )
            })
            .map_err(mongo_err)?;
            if let Some(document) = document {
                let row = parse_execution_row(document)?;
                if row.status != "running" {
                    tracing::warn!(
                        "finish_execution_row observed terminal row already written for task {} execution_id={} status={}",
                        row.task_id,
                        row.execution_id,
                        row.status
                    );
                    return Ok(());
                }
            }
            return Err(SchedulerError::Storage(
                "missing running execution row while reconciling stale execution".to_string(),
            ));
        }
        Ok(())
    }

    fn find_execution_row(
        &self,
        task_id: &str,
        execution_id: i64,
        started_at: chrono::DateTime<Utc>,
    ) -> Result<Option<ExecutionRow>, SchedulerError> {
        let document = retry_mongo_read("task_executions.find_execution_row", || {
            self.executions.find_one(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": task_id,
                    "execution_id": execution_id,
                    "started_at": BsonDateTime::from_chrono(started_at),
                },
                None,
            )
        })
        .map_err(mongo_err)?;
        document.map(parse_execution_row).transpose()
    }

    pub(crate) fn get_retry_count(&self, task_id: &str) -> Result<u32, SchedulerError> {
        let document = self
            .tasks
            .find_one(self.task_filter(task_id), None)
            .map_err(mongo_err)?;
        Ok(document
            .as_ref()
            .and_then(|doc| numeric_field_to_u32(doc, "retry_count"))
            .unwrap_or(0))
    }

    pub(crate) fn increment_retry_count(&self, task_id: &str) -> Result<u32, SchedulerError> {
        let filter = doc! { "task_id": task_id };
        let update = doc! { "$inc": { "retry_count": 1i32 } };
        retry_mongo_write("tasks.increment_retry_count", || {
            self.tasks.update_many(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;
        self.get_retry_count(task_id)
    }

    pub(crate) fn reset_retry_count(&self, task_id: &str) -> Result<(), SchedulerError> {
        let filter = doc! { "task_id": task_id };
        let update = doc! { "$set": { "retry_count": 0i32 } };
        retry_mongo_write("tasks.reset_retry_count", || {
            self.tasks.update_many(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;
        Ok(())
    }

    pub(crate) fn list_tasks_with_status(&self) -> Result<Vec<TaskStatusSummary>, SchedulerError> {
        let created_after = BsonDateTime::from_chrono(Utc::now() - ChronoDuration::hours(24));
        let cursor = self
            .tasks
            .find(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "created_at": { "$gte": created_after },
                },
                FindOptions::builder()
                    .sort(doc! { "created_at": -1 })
                    .build(),
            )
            .map_err(mongo_err)?;

        // First pass: collect task docs and deduplicate task_ids
        let mut task_docs = Vec::new();
        let mut seen_task_ids = HashSet::new();
        let now = Utc::now();
        for row in cursor {
            let task_doc = row.map_err(mongo_err)?;
            let task_id = task_doc
                .get_str("task_id")
                .map_err(|err| SchedulerError::Storage(format!("missing task_id: {err}")))?;
            if !seen_task_ids.insert(task_id.to_string()) {
                continue;
            }
            task_docs.push(task_doc);
        }

        // Batch fetch all executions for these task_ids in one query
        let task_ids: Vec<&str> = seen_task_ids.iter().map(|s| s.as_str()).collect();
        let executions_map = self.load_execution_rows_for_tasks(&task_ids)?;

        // Second pass: build summaries using the pre-fetched executions
        let mut summaries = Vec::new();
        for task_doc in task_docs {
            let task_id = task_doc.get_str("task_id").unwrap();
            let mut task = deserialize_task_document(&task_doc)?;
            if maybe_repair_legacy_weekday_cron_task(&mut task, now)? {
                self.update_task(&task)?;
            }
            let request_summary = derive_request_summary(&task_doc);
            let executions = executions_map
                .get(task_id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let retry_count = numeric_field_to_u32(&task_doc, "retry_count").unwrap_or(0);
            let auto_disabled_reason = task_doc
                .get_str("auto_disabled_reason")
                .ok()
                .map(|value| value.to_string());
            let auto_disabled_at = task_doc
                .get_datetime("auto_disabled_at")
                .ok()
                .map(|value| value.to_chrono().to_rfc3339());
            let (schedule_type, next_run, run_at) = match &task.schedule {
                Schedule::Cron { next_run, .. } => {
                    ("cron".to_string(), Some(next_run.to_rfc3339()), None)
                }
                Schedule::OneShot { run_at } => {
                    ("one_shot".to_string(), None, Some(run_at.to_rfc3339()))
                }
            };
            summaries.push(build_task_status_summary(
                task_id,
                task_doc.get_str("kind").unwrap_or("unknown"),
                task_doc.get_str("channel").unwrap_or("email"),
                request_summary,
                &task,
                schedule_type,
                next_run,
                run_at,
                executions,
                retry_count,
                auto_disabled_reason,
                auto_disabled_at,
                self.owner_accepts_worker_pickup(),
                now,
            ));
        }
        Ok(summaries)
    }

    pub fn load_task_with_status(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskStatusSummary>, SchedulerError> {
        let Some(task_doc) = self
            .tasks
            .find_one(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": task_id,
                },
                None,
            )
            .map_err(mongo_err)?
        else {
            return Ok(None);
        };

        let now = Utc::now();
        let mut task = deserialize_task_document(&task_doc)?;
        if maybe_repair_legacy_weekday_cron_task(&mut task, now)? {
            self.update_task(&task)?;
        }
        let request_summary = derive_request_summary(&task_doc);
        let executions = self.load_execution_rows_for_task(task_id)?;
        let retry_count = numeric_field_to_u32(&task_doc, "retry_count").unwrap_or(0);
        let auto_disabled_reason = task_doc
            .get_str("auto_disabled_reason")
            .ok()
            .map(|value| value.to_string());
        let auto_disabled_at = task_doc
            .get_datetime("auto_disabled_at")
            .ok()
            .map(|value| value.to_chrono().to_rfc3339());
        let (schedule_type, next_run, run_at) = match &task.schedule {
            Schedule::Cron { next_run, .. } => {
                ("cron".to_string(), Some(next_run.to_rfc3339()), None)
            }
            Schedule::OneShot { run_at } => {
                ("one_shot".to_string(), None, Some(run_at.to_rfc3339()))
            }
        };

        Ok(Some(build_task_status_summary(
            task_id,
            task_doc.get_str("kind").unwrap_or("unknown"),
            task_doc.get_str("channel").unwrap_or("email"),
            request_summary,
            &task,
            schedule_type,
            next_run,
            run_at,
            &executions,
            retry_count,
            auto_disabled_reason,
            auto_disabled_at,
            self.owner_accepts_worker_pickup(),
            now,
        )))
    }

    pub fn list_task_executions(
        &self,
        task_id: &str,
    ) -> Result<Vec<TaskExecutionSummary>, SchedulerError> {
        let rows = self.load_execution_rows_for_task(task_id)?;
        Ok(rows
            .into_iter()
            .map(|row| TaskExecutionSummary {
                execution_id: row.execution_id,
                status: row.status,
                started_at: row.started_at.to_rfc3339(),
                finished_at: row.finished_at.map(|value| value.to_rfc3339()),
                error_message: row.error_message,
                duration_seconds: row
                    .finished_at
                    .map(|value| value.signed_duration_since(row.started_at).num_seconds()),
            })
            .collect())
    }

    pub(crate) fn list_routines_with_status(&self) -> Result<Vec<RoutineSummary>, SchedulerError> {
        let cursor = self
            .tasks
            .find(
                self.owner_filter(),
                FindOptions::builder()
                    .sort(doc! { "created_at": -1 })
                    .build(),
            )
            .map_err(mongo_err)?;
        let mut summaries = Vec::new();
        let mut seen_task_ids = HashSet::new();
        let now = Utc::now();

        for row in cursor {
            let task_doc = row.map_err(mongo_err)?;
            let task_id = task_doc
                .get_str("task_id")
                .map_err(|err| SchedulerError::Storage(format!("missing task_id: {err}")))?;
            if !seen_task_ids.insert(task_id.to_string()) {
                continue;
            }

            let mut task = deserialize_task_document(&task_doc)?;
            if maybe_repair_legacy_weekday_cron_task(&mut task, now)? {
                self.update_task(&task)?;
            }
            if !is_user_visible_routine_task(&task, now) {
                continue;
            }

            let execution = self.latest_execution_for_task(task_id)?;
            let channel = task_doc.get_str("channel").unwrap_or("email").to_string();
            let name =
                derive_request_summary(&task_doc).unwrap_or_else(|| default_routine_name(&channel));
            let (schedule_type, next_run, run_at, is_recurring) =
                routine_schedule_fields(&task.schedule);

            summaries.push(RoutineSummary {
                id: task_id.to_string(),
                name,
                kind: task_doc.get_str("kind").unwrap_or("unknown").to_string(),
                channel,
                enabled: task_doc.get_bool("enabled").unwrap_or(task.enabled),
                schedule_type,
                next_run,
                run_at,
                last_run: task.last_run.map(|value| value.to_rfc3339()),
                execution_status: execution
                    .as_ref()
                    .and_then(|doc| doc.get_str("status").ok())
                    .map(|value| value.to_string()),
                error_message: execution.as_ref().and_then(|doc| {
                    doc.get_str("error_message")
                        .ok()
                        .map(|value| value.to_string())
                }),
                created_at: task.created_at.to_rfc3339(),
                is_recurring,
            });
        }

        Ok(summaries)
    }

    fn latest_execution_for_task(&self, task_id: &str) -> Result<Option<Document>, SchedulerError> {
        self.executions
            .find_one(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": task_id,
                },
                FindOneOptions::builder()
                    .sort(doc! { "started_at": -1 })
                    .build(),
            )
            .map_err(mongo_err)
    }

    fn owner_filter(&self) -> Document {
        doc! {
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
        }
    }

    fn task_filter(&self, task_id: &str) -> Document {
        doc! {
            "owner_scope.kind": &self.owner_kind,
            "owner_scope.id": &self.owner_id,
            "task_id": task_id,
        }
    }

    fn owner_scope_doc(&self) -> Document {
        doc! {
            "kind": &self.owner_kind,
            "id": &self.owner_id,
        }
    }

    fn owner_accepts_worker_pickup(&self) -> bool {
        self.owner_kind == "user"
    }
}

fn parse_execution_row(document: Document) -> Result<ExecutionRow, SchedulerError> {
    let doc_id = document
        .get("_id")
        .cloned()
        .ok_or_else(|| SchedulerError::Storage("missing _id for execution row".to_string()))?;
    let execution_id = bson_i64(document.get("execution_id"), "execution_id")?;
    let task_id = document
        .get_str("task_id")
        .map_err(|err| {
            SchedulerError::Storage(format!("missing task_id for execution row: {err}"))
        })?
        .to_string();
    let started_at = document
        .get_datetime("started_at")
        .map_err(|err| {
            SchedulerError::Storage(format!("missing started_at for execution row: {err}"))
        })?
        .to_chrono();
    let finished_at = match document.get("finished_at") {
        Some(Bson::DateTime(value)) => Some(value.to_chrono()),
        _ => None,
    };
    let status = document
        .get_str("status")
        .map_err(|err| SchedulerError::Storage(format!("missing status for execution row: {err}")))?
        .to_string();
    let error_message = document
        .get_str("error_message")
        .ok()
        .map(|value| value.to_string());

    Ok(ExecutionRow {
        doc_id,
        task_id,
        execution_id,
        started_at,
        finished_at,
        status,
        error_message,
    })
}

fn build_task_status_summary(
    task_id: &str,
    kind: &str,
    channel: &str,
    request_summary: Option<String>,
    task: &ScheduledTask,
    schedule_type: String,
    next_run: Option<String>,
    run_at: Option<String>,
    executions: &[ExecutionRow],
    retry_count: u32,
    auto_disabled_reason: Option<String>,
    auto_disabled_at: Option<String>,
    owner_accepts_worker_pickup: bool,
    now: chrono::DateTime<Utc>,
) -> TaskStatusSummary {
    let latest_execution = executions.first();
    let derived_status = derive_user_task_status(
        task,
        latest_execution,
        retry_count,
        auto_disabled_reason.as_deref(),
        owner_accepts_worker_pickup,
        now,
    );

    TaskStatusSummary {
        id: task_id.to_string(),
        kind: kind.to_string(),
        channel: channel.to_string(),
        request_summary,
        enabled: task.enabled,
        created_at: task.created_at.to_rfc3339(),
        last_run: task.last_run.map(|value| value.to_rfc3339()),
        schedule_type,
        next_run,
        run_at,
        execution_status: latest_execution.map(|row| row.status.clone()),
        error_message: latest_execution.and_then(|row| row.error_message.clone()),
        execution_started_at: latest_execution.map(|row| row.started_at.to_rfc3339()),
        auto_disabled_reason,
        auto_disabled_at,
        status: derived_status.status.to_string(),
        status_reason: derived_status.status_reason,
        status_changed_at: derived_status.status_changed_at,
        retry_at: derived_status.retry_at,
        will_retry: derived_status.will_retry,
        retry_count,
        is_running_long: derived_status.is_running_long,
        can_cancel: derived_status.can_cancel,
        can_resubmit: derived_status.can_resubmit,
    }
}

#[derive(Debug)]
struct DerivedTaskStatus {
    status: &'static str,
    status_reason: Option<String>,
    status_changed_at: Option<String>,
    retry_at: Option<String>,
    will_retry: bool,
    is_running_long: bool,
    can_cancel: bool,
    can_resubmit: bool,
}

fn derive_user_task_status(
    task: &ScheduledTask,
    latest_execution: Option<&ExecutionRow>,
    retry_count: u32,
    auto_disabled_reason: Option<&str>,
    owner_accepts_worker_pickup: bool,
    now: chrono::DateTime<Utc>,
) -> DerivedTaskStatus {
    let is_one_shot = matches!(&task.schedule, Schedule::OneShot { .. });
    let is_run_task = matches!(&task.kind, TaskKind::RunTask(_));

    let mut status_reason = None;
    let mut retry_at = None;
    let mut will_retry = false;
    let mut is_running_long = false;
    let (status, status_changed_at) = if let Some(row) = latest_execution {
        let status;
        let status_changed_at = Some(row.finished_at.unwrap_or(row.started_at).to_rfc3339());
        match row.status.as_str() {
            "running" => {
                let running_secs = now.signed_duration_since(row.started_at).num_seconds();
                is_running_long = running_secs >= LONG_RUNNING_WARNING_SECS;
                if task.enabled {
                    status = "running";
                    if is_running_long {
                        status_reason = Some(
                            "Running for over an hour. It may be waiting on a worker or external tool."
                                .to_string(),
                        );
                    }
                } else {
                    status = "cancellation_requested";
                    status_reason = Some(
                        "Cancellation requested. Waiting for the running worker to stop."
                            .to_string(),
                    );
                }
            }
            "failed" => {
                if let Some(reason) = auto_disabled_reason {
                    status = "failed";
                    status_reason = Some(format!(
                        "Automatic retries stopped because {}. Use Resubmit to run it again.",
                        humanize_auto_disabled_reason(reason)
                    ));
                } else if task.enabled && is_one_shot {
                    match &task.schedule {
                        Schedule::OneShot { run_at } if *run_at > now => {
                            status = "retry_scheduled";
                            retry_at = Some(run_at.to_rfc3339());
                            will_retry = true;
                            let retry_prefix = if retry_count > 0 {
                                format!("Automatic retry {retry_count} scheduled")
                            } else {
                                "Automatic retry scheduled".to_string()
                            };
                            status_reason =
                                Some(format!("{retry_prefix} for {}", run_at.to_rfc3339()));
                        }
                        Schedule::OneShot { run_at } => {
                            retry_at = Some(run_at.to_rfc3339());
                            will_retry = true;
                            if owner_accepts_worker_pickup {
                                status = "queued";
                                status_reason = Some(if retry_count > 0 {
                                    format!(
                                        "Automatic retry {retry_count} is due and waiting for a worker to claim it."
                                    )
                                } else {
                                    "Automatic retry is due and waiting for a worker to claim it."
                                        .to_string()
                                });
                            } else {
                                status = "retry_scheduled";
                                status_reason = Some(if retry_count > 0 {
                                    format!(
                                        "Automatic retry {retry_count} is pending on the live channel task."
                                    )
                                } else {
                                    "Automatic retry is pending on the live channel task."
                                        .to_string()
                                });
                            }
                        }
                        Schedule::Cron { .. } => {
                            status = "failed";
                        }
                    }
                } else {
                    status = "failed";
                    if is_run_task && is_one_shot {
                        status_reason =
                            Some("This workflow failed. Use Resubmit to run it again.".to_string());
                    }
                }
            }
            "success" => {
                status = "success";
            }
            "superseded" => {
                status = "superseded";
            }
            "cancelled" => {
                status = "cancelled";
                status_reason = row
                    .error_message
                    .clone()
                    .or_else(|| Some("Cancelled from the dashboard.".to_string()));
            }
            other => {
                status = match other {
                    "" => "scheduled",
                    _ => "scheduled",
                };
            }
        }
        (status, status_changed_at)
    } else {
        let status;
        let status_changed_at;
        match &task.schedule {
            Schedule::OneShot { run_at } => {
                if task.enabled {
                    if *run_at <= now {
                        if owner_accepts_worker_pickup {
                            status = "queued";
                            status_reason =
                                Some("Waiting for a worker to claim this task.".to_string());
                        } else {
                            status = "scheduled";
                            status_reason = Some(
                                "Mirrored task record. Worker pickup happens from the live channel task."
                                    .to_string(),
                            );
                        }
                        status_changed_at = Some(run_at.to_rfc3339());
                    } else {
                        status = "scheduled";
                        status_changed_at = Some(task.created_at.to_rfc3339());
                    }
                } else if *run_at <= now {
                    status = "expired";
                    status_reason = Some(
                        "This one-time task passed its scheduled run time without completing."
                            .to_string(),
                    );
                    status_changed_at = Some(run_at.to_rfc3339());
                } else {
                    status = "cancelled";
                    status_reason = Some("Cancelled before the task started running.".to_string());
                    status_changed_at = Some(task.created_at.to_rfc3339());
                }
            }
            Schedule::Cron { next_run, .. } => {
                if task.enabled {
                    status = "scheduled";
                    status_changed_at = Some(next_run.to_rfc3339());
                } else {
                    status = "paused";
                    status_reason = Some("This recurring task is paused.".to_string());
                    status_changed_at = Some(task.created_at.to_rfc3339());
                }
            }
        }
        (status, status_changed_at)
    };

    let can_cancel = match status {
        "scheduled" | "queued" | "retry_scheduled" => is_one_shot,
        "running" => is_one_shot && is_run_task,
        _ => false,
    };
    let can_resubmit = is_run_task && is_one_shot && matches!(status, "failed" | "expired");

    DerivedTaskStatus {
        status,
        status_reason,
        status_changed_at,
        retry_at,
        will_retry,
        is_running_long,
        can_cancel,
        can_resubmit,
    }
}

fn humanize_auto_disabled_reason(raw: &str) -> String {
    raw.trim()
        .strip_prefix("auto-disabled:")
        .unwrap_or(raw.trim())
        .trim()
        .trim_end_matches('.')
        .to_string()
}

fn resolve_aci_registration_grace_period() -> ChronoDuration {
    std::env::var("RUN_TASK_ACI_REGISTRATION_GRACE_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .map(ChronoDuration::seconds)
        .unwrap_or_else(|| ChronoDuration::seconds(DEFAULT_ACI_REGISTRATION_GRACE_SECS))
}

fn resolve_aci_trace_activity_grace_period() -> ChronoDuration {
    std::env::var("RUN_TASK_ACI_TRACE_ACTIVITY_GRACE_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .map(ChronoDuration::seconds)
        .unwrap_or_else(|| ChronoDuration::seconds(DEFAULT_ACI_TRACE_ACTIVITY_GRACE_SECS))
}

fn resolve_local_fallback_timeout_grace_period() -> ChronoDuration {
    let overall_timeout_secs = std::env::var("RUN_TASK_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0);
    let fallback_timeout_secs = std::env::var("RUN_TASK_CODEX_FALLBACK_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_LOCAL_FALLBACK_TIMEOUT_SECS);
    let effective_timeout_secs = overall_timeout_secs
        .map(|overall| overall.min(fallback_timeout_secs))
        .unwrap_or(fallback_timeout_secs)
        .max(1);
    ChronoDuration::seconds(effective_timeout_secs)
}

fn missing_aci_registry_reconciliation_reason(
    workspace_dir: Option<&Path>,
    started_at: chrono::DateTime<Utc>,
    _now: chrono::DateTime<Utc>,
) -> (&'static str, String) {
    let Some(workspace_dir) = workspace_dir else {
        return (
            "auto-disabled: execution started but ACI container was never created",
            "reconciled stale running execution; ACI container not found".to_string(),
        );
    };

    if workspace_records_unfinished_fallback_after_aci_run(workspace_dir, started_at) {
        return (
            "auto-disabled: primary runner failed and fallback never reached a terminal state",
            "reconciled stale running execution after primary ACI run failed and fallback never reached a terminal state".to_string(),
        );
    }

    if workspace_has_current_aci_execution_evidence(workspace_dir, started_at) {
        return (
            "auto-disabled: execution lost terminal reconciliation after an ACI-backed runner failure",
            "reconciled stale running execution after an ACI-backed runner executed but no live registry record remained".to_string(),
        );
    }

    (
        "auto-disabled: execution started but ACI container was never created",
        "reconciled stale running execution; ACI container not found".to_string(),
    )
}

fn unfinished_fallback_reconciliation_action(
    workspace_dir: Option<&Path>,
    started_at: chrono::DateTime<Utc>,
    now: chrono::DateTime<Utc>,
    stale_before: chrono::DateTime<Utc>,
) -> Option<UnfinishedFallbackReconciliationAction> {
    let workspace_dir = workspace_dir?;
    if !workspace_records_unfinished_fallback_after_aci_run(workspace_dir, started_at) {
        return None;
    }

    let fallback_started_before_watchdog =
        fallback_activity_started_before_or_at(workspace_dir, started_at, stale_before);
    let fallback_still_within_timeout =
        workspace_suggests_unfinished_fallback_after_aci_run(workspace_dir, started_at, now);
    if fallback_started_before_watchdog || !fallback_still_within_timeout {
        let (disable_reason, error_reason) = unfinished_fallback_reconciliation_reason();
        Some(UnfinishedFallbackReconciliationAction::Fail {
            disable_reason,
            error_reason,
        })
    } else {
        Some(UnfinishedFallbackReconciliationAction::KeepRunning)
    }
}

fn unfinished_fallback_reconciliation_reason() -> (&'static str, String) {
    (
        "auto-disabled: primary runner failed and fallback never reached a terminal state",
        "reconciled stale running execution after primary ACI run failed and fallback never reached a terminal state".to_string(),
    )
}

fn workspace_has_current_aci_execution_evidence(
    workspace_dir: &Path,
    started_at: chrono::DateTime<Utc>,
) -> bool {
    if let Some(metadata) = load_run_task_trace_metadata(workspace_dir) {
        if trace_started_at_matches_execution(metadata.started_at_unix_ms, started_at) {
            return true;
        }
    }

    [
        workspace_dir.join(".run_task_trace_codex_primary/aci/container_show.json"),
        workspace_dir.join(".run_task_trace_codex_primary/aci/remote_exit_code.txt"),
        workspace_dir.join(".run_task_trace_codex_primary/aci/remote_output.log"),
        workspace_dir.join(".codex_remote_output.log"),
        workspace_dir.join(".aci_recovery_context.json"),
    ]
    .iter()
    .any(|path| path.exists() && path_mtime_matches_execution(path, started_at))
}

fn workspace_suggests_recent_inflight_aci_result_handling(
    workspace_dir: &Path,
    started_at: chrono::DateTime<Utc>,
    now: chrono::DateTime<Utc>,
) -> bool {
    if !workspace_has_current_aci_execution_evidence(workspace_dir, started_at) {
        return false;
    }

    let Some(metadata) = load_run_task_trace_metadata(workspace_dir) else {
        return false;
    };
    if metadata.backend != "codex_azure_aci" {
        return false;
    }
    if metadata.current_stage.is_empty() || metadata.current_stage == "completed" {
        return false;
    }
    if metadata.finished_at_unix_ms.is_some() || metadata.success.is_some() {
        return false;
    }

    let activity_at_ms = [
        metadata.stage_updated_at_unix_ms,
        metadata.started_at_unix_ms,
        current_run_output_artifact_activity_ms(workspace_dir, started_at),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or_default();
    if activity_at_ms == 0 {
        return false;
    }

    let grace = resolve_aci_trace_activity_grace_period();
    let activity_age_ms = now.timestamp_millis().saturating_sub(activity_at_ms);
    activity_age_ms <= grace.num_milliseconds()
}

fn current_run_output_artifact_activity_ms(
    workspace_dir: &Path,
    started_at: chrono::DateTime<Utc>,
) -> Option<i64> {
    [
        workspace_dir.join("reply_email_draft.html"),
        workspace_dir.join("reply_message.txt"),
        workspace_dir.join(".notion_api_replied"),
        workspace_dir.join("reply_email_attachments"),
    ]
    .into_iter()
    .filter_map(|path| current_run_path_activity_ms(&path, started_at))
    .max()
}

fn current_run_path_activity_ms(path: &Path, started_at: chrono::DateTime<Utc>) -> Option<i64> {
    let modified_at_ms = path_modified_at_unix_ms(path)?;
    let earliest_current_run_ms = started_at
        .timestamp_millis()
        .saturating_sub(CURRENT_TRACE_START_MATCH_TOLERANCE_MS);
    if modified_at_ms < earliest_current_run_ms {
        return None;
    }
    Some(modified_at_ms)
}

fn workspace_suggests_unfinished_fallback_after_aci_run(
    workspace_dir: &Path,
    started_at: chrono::DateTime<Utc>,
    now: chrono::DateTime<Utc>,
) -> bool {
    let Some(metadata) = unfinished_fallback_trace_metadata(workspace_dir, started_at) else {
        return false;
    };
    let activity_at_ms = metadata
        .stage_updated_at_unix_ms
        .or(metadata.started_at_unix_ms)
        .unwrap_or_else(|| started_at.timestamp_millis());
    let activity_age_ms = now.timestamp_millis().saturating_sub(activity_at_ms);
    activity_age_ms <= resolve_local_fallback_timeout_grace_period().num_milliseconds()
}

fn workspace_records_unfinished_fallback_after_aci_run(
    workspace_dir: &Path,
    started_at: chrono::DateTime<Utc>,
) -> bool {
    unfinished_fallback_trace_metadata(workspace_dir, started_at).is_some()
}

fn unfinished_fallback_trace_metadata(
    workspace_dir: &Path,
    started_at: chrono::DateTime<Utc>,
) -> Option<RunTaskTraceMetadata> {
    if !workspace_has_current_aci_execution_evidence(workspace_dir, started_at) {
        return None;
    }
    let metadata = load_run_task_trace_metadata(workspace_dir)?;
    if metadata.backend == "claude_local"
        && metadata.current_stage == "executing_claude_local"
        && metadata.finished_at_unix_ms.is_none()
        && metadata.success.is_none()
    {
        Some(metadata)
    } else {
        None
    }
}

fn fallback_activity_started_before_or_at(
    workspace_dir: &Path,
    started_at: chrono::DateTime<Utc>,
    cutoff: chrono::DateTime<Utc>,
) -> bool {
    let Some(metadata) = load_run_task_trace_metadata(workspace_dir) else {
        return started_at <= cutoff;
    };
    let activity_at_ms = metadata
        .stage_updated_at_unix_ms
        .or(metadata.started_at_unix_ms)
        .unwrap_or_else(|| started_at.timestamp_millis());
    chrono::DateTime::<Utc>::from_timestamp_millis(activity_at_ms)
        .map(|activity_at| activity_at <= cutoff)
        .unwrap_or(started_at <= cutoff)
}

fn current_thread_supersede_reason(context: &RunTaskReconciliationContext) -> Option<String> {
    let expected_epoch = context.thread_epoch?;
    let workspace_dir = context.workspace_dir.as_deref().map(Path::new)?;
    let state_path = context
        .thread_state_path
        .as_deref()
        .map(Path::new)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_thread_state_path(workspace_dir));
    let current_epoch = current_thread_epoch(&state_path)?;
    if current_epoch > expected_epoch {
        Some(format!(
            "thread superseded by a newer follow-up (expected epoch {}, current epoch {})",
            expected_epoch, current_epoch
        ))
    } else {
        None
    }
}

fn workspace_peer_supersede_reason(
    task_id: &str,
    workspace_peer_rows: &HashMap<String, Vec<ExecutionRow>>,
    workspace_dir: Option<&Path>,
    started_at: chrono::DateTime<Utc>,
) -> Option<String> {
    let workspace_dir = workspace_dir?;
    if workspace_has_current_aci_execution_evidence(workspace_dir, started_at) {
        return None;
    }

    let tolerance = ChronoDuration::milliseconds(CURRENT_TRACE_START_MATCH_TOLERANCE_MS);
    let window_start = started_at - tolerance;
    let window_end = started_at + tolerance;
    let mut peer_running_started_at: Option<chrono::DateTime<Utc>> = None;
    let mut peer_terminal_finished_at: Option<chrono::DateTime<Utc>> = None;

    for (peer_task_id, rows) in workspace_peer_rows {
        if peer_task_id == task_id {
            continue;
        }
        for row in rows {
            match row.status.as_str() {
                "running" if row.started_at >= window_start && row.started_at <= window_end => {
                    peer_running_started_at = Some(match peer_running_started_at {
                        Some(existing) => existing.min(row.started_at),
                        None => row.started_at,
                    });
                }
                _ => {
                    if let Some(finished_at) = row.finished_at {
                        if finished_at >= window_start {
                            peer_terminal_finished_at = Some(match peer_terminal_finished_at {
                                Some(existing) => existing.max(finished_at),
                                None => finished_at,
                            });
                        }
                    }
                }
            }
        }
    }

    if let Some(peer_started_at) = peer_running_started_at {
        return Some(format!(
            "another task for the same workspace was already running at {}",
            peer_started_at.to_rfc3339()
        ));
    }

    peer_terminal_finished_at.map(|finished_at| {
        format!(
            "another task for the same workspace already completed at {}",
            finished_at.to_rfc3339()
        )
    })
}

fn load_run_task_trace_metadata(workspace_dir: &Path) -> Option<RunTaskTraceMetadata> {
    let metadata_path = workspace_dir.join(".run_task_trace/metadata.json");
    let contents = fs::read_to_string(metadata_path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&contents).ok()?;
    Some(RunTaskTraceMetadata {
        backend: value
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        current_stage: value
            .get("current_stage")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        started_at_unix_ms: value.get("started_at_unix_ms").and_then(|v| v.as_i64()),
        finished_at_unix_ms: value.get("finished_at_unix_ms").and_then(|v| v.as_i64()),
        stage_updated_at_unix_ms: value
            .get("stage_updated_at_unix_ms")
            .and_then(|v| v.as_i64()),
        success: value.get("success").and_then(|v| v.as_bool()),
    })
}

fn trace_started_at_matches_execution(
    trace_started_at_unix_ms: Option<i64>,
    started_at: chrono::DateTime<Utc>,
) -> bool {
    let Some(trace_started_at_unix_ms) = trace_started_at_unix_ms else {
        return false;
    };
    trace_started_at_unix_ms
        .saturating_sub(started_at.timestamp_millis())
        .abs()
        <= CURRENT_TRACE_START_MATCH_TOLERANCE_MS
}

fn path_mtime_matches_execution(path: &Path, started_at: chrono::DateTime<Utc>) -> bool {
    let Some(modified_at_ms) = path_modified_at_unix_ms(path) else {
        return false;
    };
    modified_at_ms
        .saturating_sub(started_at.timestamp_millis())
        .abs()
        <= CURRENT_TRACE_START_MATCH_TOLERANCE_MS
}

fn path_modified_at_unix_ms(path: &Path) -> Option<i64> {
    let metadata = fs::metadata(path).ok()?;
    let modified_at = metadata.modified().ok()?;
    let duration = modified_at.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

fn is_stale_reconciliation_error(reason: Option<&str>) -> bool {
    reason
        .map(|value| {
            value
                .trim_start()
                .starts_with("reconciled stale running execution")
        })
        .unwrap_or(false)
}

fn should_replace_stale_reconciliation_terminal_row(
    row: &ExecutionRow,
    desired_status: &str,
    desired_error_message: Option<&str>,
) -> bool {
    if row.status != "failed" || !is_stale_reconciliation_error(row.error_message.as_deref()) {
        return false;
    }

    match desired_status {
        "success" | "superseded" => true,
        "failed" => desired_error_message
            .map(|message| !is_stale_reconciliation_error(Some(message)))
            .unwrap_or(false),
        _ => false,
    }
}

fn should_clear_auto_disabled_reason_after_stale_replacement(
    desired_status: &str,
    desired_error_message: Option<&str>,
) -> bool {
    matches!(desired_status, "success" | "superseded")
        || (desired_status == "failed"
            && desired_error_message
                .map(|message| !is_stale_reconciliation_error(Some(message)))
                .unwrap_or(false))
}

fn clear_auto_disabled_reason_by_task_id(
    tasks: &Collection<Document>,
    task_id: &str,
) -> Result<(), SchedulerError> {
    let filter = doc! { "task_id": task_id };
    let update = doc! {
        "$unset": {
            "auto_disabled_reason": "",
            "auto_disabled_at": "",
        }
    };
    retry_mongo_write("tasks.clear_auto_disabled_reason_by_task_id", || {
        tasks.update_many(filter.clone(), update.clone(), None)
    })
    .map_err(mongo_err)?;
    Ok(())
}

fn apply_persisted_task_fields(
    document: &Document,
    task: &mut ScheduledTask,
) -> Result<(), SchedulerError> {
    if let Ok(enabled) = document.get_bool("enabled") {
        task.enabled = enabled;
    }
    if let Ok(last_run) = document.get_datetime("last_run") {
        task.last_run = Some(last_run.to_chrono());
    }
    if matches!(document.get("last_run"), Some(Bson::Null)) {
        task.last_run = None;
    }
    if let Ok(schedule) = document.get_document("schedule") {
        task.schedule = parse_schedule_doc(schedule)?;
    }
    Ok(())
}

fn parse_schedule_doc(document: &Document) -> Result<Schedule, SchedulerError> {
    let schedule_type = document
        .get_str("type")
        .map_err(|err| SchedulerError::Storage(format!("missing schedule.type: {err}")))?;
    match schedule_type {
        "cron" => {
            let expression = document
                .get_str("cron_expression")
                .map_err(|err| {
                    SchedulerError::Storage(format!("missing schedule.cron_expression: {err}"))
                })?
                .to_string();
            let next_run = document
                .get_datetime("next_run")
                .map_err(|err| {
                    SchedulerError::Storage(format!("missing schedule.next_run: {err}"))
                })?
                .to_chrono();
            Ok(Schedule::Cron {
                expression,
                next_run,
            })
        }
        "one_shot" => {
            let run_at = document
                .get_datetime("run_at")
                .map_err(|err| SchedulerError::Storage(format!("missing schedule.run_at: {err}")))?
                .to_chrono();
            Ok(Schedule::OneShot { run_at })
        }
        other => Err(SchedulerError::Storage(format!(
            "unsupported schedule.type: {other}"
        ))),
    }
}

fn bson_i64(value: Option<&Bson>, field: &str) -> Result<i64, SchedulerError> {
    match value {
        Some(Bson::Int64(value)) => Ok(*value),
        Some(Bson::Int32(value)) => Ok(i64::from(*value)),
        Some(other) => Err(SchedulerError::Storage(format!(
            "invalid {field} type for execution row: {other:?}"
        ))),
        None => Err(SchedulerError::Storage(format!(
            "missing {field} for execution row"
        ))),
    }
}

fn schedule_doc(schedule: &Schedule) -> Document {
    match schedule {
        Schedule::Cron {
            expression,
            next_run,
        } => doc! {
            "type": "cron",
            "cron_expression": expression,
            "next_run": BsonDateTime::from_chrono(*next_run),
            "run_at": Bson::Null,
        },
        Schedule::OneShot { run_at } => doc! {
            "type": "one_shot",
            "cron_expression": Bson::Null,
            "next_run": Bson::Null,
            "run_at": BsonDateTime::from_chrono(*run_at),
        },
    }
}

fn routine_schedule_fields(schedule: &Schedule) -> (String, Option<String>, Option<String>, bool) {
    match schedule {
        Schedule::Cron { next_run, .. } => {
            ("cron".to_string(), Some(next_run.to_rfc3339()), None, true)
        }
        Schedule::OneShot { run_at } => (
            "one_shot".to_string(),
            None,
            Some(run_at.to_rfc3339()),
            false,
        ),
    }
}

fn resolve_owner_scope(path: &Path) -> (String, String) {
    resolve_owner_scope_with(path, is_global_account_id)
}

fn resolve_owner_scope_with<F>(path: &Path, is_account_owner_id: F) -> (String, String)
where
    F: Fn(&str) -> bool,
{
    let mut components: Vec<String> = Vec::new();
    for component in path.components() {
        if let Some(value) = component.as_os_str().to_str() {
            components.push(value.to_string());
        }
    }

    for (idx, value) in components.iter().enumerate() {
        if value == "users" {
            if let Some(owner_id) = components.get(idx + 1) {
                return (
                    owner_kind_for_id(owner_id, &is_account_owner_id).to_string(),
                    owner_id.to_string(),
                );
            }
        }
    }

    if path.file_name().and_then(|v| v.to_str()) == Some("tasks.db") {
        if let Some(state_dir) = path.parent() {
            if state_dir.file_name().and_then(|v| v.to_str()) == Some("state") {
                if let Some(owner_dir) = state_dir.parent() {
                    if let Some(owner_id) = owner_dir.file_name().and_then(|v| v.to_str()) {
                        return (
                            owner_kind_for_id(owner_id, &is_account_owner_id).to_string(),
                            owner_id.to_string(),
                        );
                    }
                }
            }
        }
    }

    let hashed = format!("{:x}", md5::compute(path.to_string_lossy().as_bytes()));
    ("path_scope".to_string(), hashed)
}

fn owner_kind_for_id<F>(owner_id: &str, is_account_owner_id: F) -> &'static str
where
    F: Fn(&str) -> bool,
{
    if is_account_owner_id(owner_id) {
        "account"
    } else {
        "user"
    }
}

/// Mark orphaned execution as finished by workspace path.
/// Used by ACI recovery to update task_executions when recovering containers.
/// `status` should be "success" if results were recovered, "failed" otherwise.
/// Returns Ok(true) if an execution was marked, Ok(false) if none found.
pub fn mark_execution_finished_by_workspace(
    workspace_path: &Path,
    status: &str,
    error_message: Option<&str>,
) -> Result<bool, SchedulerError> {
    let client = get_shared_client();
    let db = database_from_env(client);
    let (owner_kind, owner_id) = resolve_owner_scope(workspace_path);

    let tasks: Collection<Document> = db.collection("tasks");
    let executions: Collection<Document> = db.collection("task_executions");

    // Find ALL task_ids with matching workspace_dir in task_json
    let workspace_str = workspace_path.to_string_lossy();
    let filter = doc! {
        "owner_scope.kind": &owner_kind,
        "owner_scope.id": &owner_id,
    };

    let cursor = retry_mongo_read("tasks.find_for_workspace_lookup", || {
        tasks.find(filter.clone(), None)
    })
    .map_err(mongo_err)?;

    let mut matching_task_ids: Vec<String> = Vec::new();
    for doc_result in cursor {
        let doc = doc_result.map_err(mongo_err)?;
        if let Ok(task_json) = doc.get_str("task_json") {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(task_json) {
                if let Some(ws) = parsed
                    .get("kind")
                    .and_then(|k| k.get("workspace_dir"))
                    .and_then(|v| v.as_str())
                {
                    if ws == workspace_str {
                        if let Ok(task_id) = doc.get_str("task_id") {
                            matching_task_ids.push(task_id.to_string());
                        }
                    }
                }
            }
        }
    }

    if matching_task_ids.is_empty() {
        tracing::debug!(
            "no tasks found for workspace {} - cannot mark execution",
            workspace_path.display()
        );
        return Ok(false);
    }

    let running_exec_filter = doc! {
        "owner_scope.kind": &owner_kind,
        "owner_scope.id": &owner_id,
        "task_id": { "$in": &matching_task_ids },
        "status": "running",
    };
    let running_exec_options = FindOneOptions::builder()
        .sort(doc! { "started_at": -1_i32 })
        .build();
    let running_exec = retry_mongo_read("task_executions.find_running_for_recovery", || {
        executions.find_one(running_exec_filter.clone(), running_exec_options.clone())
    })
    .map_err(mongo_err)?;

    let target_row = if let Some(exec_doc) = running_exec {
        Some(parse_execution_row(exec_doc)?)
    } else {
        let stale_failed_filter = doc! {
            "owner_scope.kind": &owner_kind,
            "owner_scope.id": &owner_id,
            "task_id": { "$in": &matching_task_ids },
            "status": "failed",
        };
        let stale_failed_options = FindOptions::builder()
            .sort(doc! { "started_at": -1_i32 })
            .limit(Some(8))
            .build();
        let cursor = retry_mongo_read("task_executions.find_stale_failed_for_recovery", || {
            executions.find(stale_failed_filter.clone(), stale_failed_options.clone())
        })
        .map_err(mongo_err)?;

        let mut replaceable = None;
        for doc_result in cursor {
            let row = parse_execution_row(doc_result.map_err(mongo_err)?)?;
            if should_replace_stale_reconciliation_terminal_row(&row, status, error_message) {
                replaceable = Some(row);
                break;
            }
        }
        replaceable
    };

    let Some(target_row) = target_row else {
        tracing::debug!(
            "no running or replaceable stale execution found for tasks {:?} - may already be marked",
            matching_task_ids
        );
        return Ok(false);
    };

    let now = Utc::now();

    let update = doc! {
        "$set": {
            "status": status,
            "finished_at": BsonDateTime::from_chrono(now),
            "error_message": error_message.map(Bson::from).unwrap_or(Bson::Null),
        }
    };

    retry_mongo_write("task_executions.mark_finished_by_recovery", || {
        executions.update_one(
            doc! { "_id": target_row.doc_id.clone() },
            update.clone(),
            None,
        )
    })
    .map_err(mongo_err)?;

    if target_row.status != "running"
        && should_clear_auto_disabled_reason_after_stale_replacement(status, error_message)
    {
        clear_auto_disabled_reason_by_task_id(&tasks, &target_row.task_id)?;
    }

    tracing::info!(
        "ACI recovery marked execution as {}: task_id={} workspace={} previous_status={}",
        status,
        target_row.task_id,
        workspace_path.display(),
        target_row.status
    );

    Ok(true)
}

fn datetime_field_to_rfc3339(document: &Document, key: &str) -> Option<String> {
    match document.get(key) {
        Some(Bson::DateTime(value)) => Some(value.to_chrono().to_rfc3339()),
        Some(Bson::String(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn numeric_field_to_u32(document: &Document, key: &str) -> Option<u32> {
    match document.get(key) {
        Some(Bson::Int32(value)) if *value >= 0 => Some(*value as u32),
        Some(Bson::Int64(value)) if *value >= 0 => Some(*value as u32),
        _ => None,
    }
}

fn mongo_err(err: mongodb::error::Error) -> SchedulerError {
    SchedulerError::Storage(format!("mongodb error: {err}"))
}

fn mongo_config_err(err: crate::mongo_store::MongoStoreError) -> SchedulerError {
    SchedulerError::Storage(err.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::{Duration as ChronoDuration, TimeZone, Utc};
    use mongodb::bson::{doc, Bson, DateTime as BsonDateTime};

    use super::{
        apply_persisted_task_fields, build_task_status_summary,
        missing_aci_registry_reconciliation_reason, resolve_owner_scope_with,
        should_replace_stale_reconciliation_terminal_row, workspace_peer_supersede_reason,
        workspace_suggests_recent_inflight_aci_result_handling, ExecutionRow,
    };
    use crate::channel::Channel;
    use crate::{RunTaskTask, Schedule, ScheduledTask, TaskKind};

    fn sample_run_task_task() -> RunTaskTask {
        RunTaskTask {
            workspace_dir: PathBuf::from("/tmp/task-workspace"),
            input_email_dir: PathBuf::from("incoming_email"),
            input_attachments_dir: PathBuf::from("incoming_attachments"),
            memory_dir: PathBuf::from("memory"),
            reference_dir: PathBuf::from("references"),
            model_name: "gpt-test".to_string(),
            runner: "codex".to_string(),
            codex_disabled: false,
            reply_to: vec!["thread".to_string()],
            reply_from: None,
            archive_root: None,
            thread_id: Some("slack:C123:1234.5678".to_string()),
            thread_epoch: Some(1),
            thread_state_path: None,
            channel: Channel::Slack,
            slack_team_id: Some("T123".to_string()),
            employee_id: None,
            requester_identifier_type: None,
            requester_identifier: None,
            account_id: None,
            channel_metadata: Default::default(),
        }
    }

    fn sample_one_shot_task(run_at: chrono::DateTime<Utc>) -> ScheduledTask {
        ScheduledTask {
            id: uuid::Uuid::new_v4(),
            kind: TaskKind::RunTask(sample_run_task_task()),
            schedule: Schedule::OneShot { run_at },
            enabled: true,
            created_at: run_at - ChronoDuration::minutes(30),
            last_run: None,
        }
    }

    fn temp_workspace(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        path.push(format!("{}_{}_{}", label, std::process::id(), now));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn resolve_owner_scope_extracts_user_id() {
        let path = PathBuf::from("/tmp/runtime/users/user-123/state/tasks.db");
        let scope = resolve_owner_scope_with(&path, |_| false);
        assert_eq!(scope.0, "user");
        assert_eq!(scope.1, "user-123");
    }

    #[test]
    fn resolve_owner_scope_classifies_known_account_ids() {
        let account_id = "2f5cdd1a-0d10-4bdf-bd48-3b993a09b0f9";
        let path = PathBuf::from(format!("/tmp/runtime/users/{account_id}/state/tasks.db"));
        let scope = resolve_owner_scope_with(&path, |candidate| candidate == account_id);
        assert_eq!(scope.0, "account");
        assert_eq!(scope.1, account_id);
    }

    #[test]
    fn task_status_summary_marks_future_one_shot_as_scheduled_not_running() {
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        let task = sample_one_shot_task(now + ChronoDuration::minutes(15));

        let summary = build_task_status_summary(
            &task.id.to_string(),
            "run_task",
            "slack",
            Some("Review launch checklist".to_string()),
            &task,
            "one_shot".to_string(),
            None,
            Some((now + ChronoDuration::minutes(15)).to_rfc3339()),
            &[],
            0,
            None,
            None,
            true,
            now,
        );

        assert_eq!(summary.execution_status, None);
        assert_eq!(summary.status, "scheduled");
        assert!(summary.can_cancel);
        assert!(!summary.can_resubmit);
    }

    #[test]
    fn apply_persisted_task_fields_overrides_stale_task_json_state() {
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        let mut task = sample_one_shot_task(now + ChronoDuration::minutes(15));
        let document = doc! {
            "enabled": false,
            "last_run": BsonDateTime::from_chrono(now),
            "schedule": doc! {
                "type": "one_shot",
                "cron_expression": Bson::Null,
                "next_run": Bson::Null,
                "run_at": BsonDateTime::from_chrono(now + ChronoDuration::minutes(45)),
            },
        };

        apply_persisted_task_fields(&document, &mut task).expect("apply persisted fields");

        assert!(!task.enabled);
        assert_eq!(task.last_run, Some(now));
        match task.schedule {
            Schedule::OneShot { run_at } => {
                assert_eq!(run_at, now + ChronoDuration::minutes(45));
            }
            other => panic!("expected one-shot schedule, got {other:?}"),
        }
    }

    #[test]
    fn task_status_summary_marks_long_running_task_and_allows_cancel() {
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        let started_at = now - ChronoDuration::hours(2);
        let task = sample_one_shot_task(started_at - ChronoDuration::minutes(5));
        let execution = ExecutionRow {
            doc_id: Bson::Null,
            task_id: task.id.to_string(),
            execution_id: 42,
            started_at,
            finished_at: None,
            status: "running".to_string(),
            error_message: None,
        };

        let summary = build_task_status_summary(
            &task.id.to_string(),
            "run_task",
            "slack",
            Some("Review launch checklist".to_string()),
            &task,
            "one_shot".to_string(),
            None,
            Some((started_at - ChronoDuration::minutes(5)).to_rfc3339()),
            &[execution],
            0,
            None,
            None,
            true,
            now,
        );

        assert_eq!(summary.status, "running");
        assert!(summary.is_running_long);
        assert!(summary.can_cancel);
        assert_eq!(summary.execution_status.as_deref(), Some("running"));
    }

    #[test]
    fn task_status_summary_marks_disabled_failed_run_task_as_resubmittable() {
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        let started_at = now - ChronoDuration::minutes(10);
        let mut task = sample_one_shot_task(started_at - ChronoDuration::minutes(5));
        task.enabled = false;
        task.last_run = Some(now - ChronoDuration::minutes(9));
        let execution = ExecutionRow {
            doc_id: Bson::Null,
            task_id: task.id.to_string(),
            execution_id: 7,
            started_at,
            finished_at: Some(now - ChronoDuration::minutes(9)),
            status: "failed".to_string(),
            error_message: Some("worker died".to_string()),
        };

        let summary = build_task_status_summary(
            &task.id.to_string(),
            "run_task",
            "slack",
            Some("Review launch checklist".to_string()),
            &task,
            "one_shot".to_string(),
            None,
            Some((started_at - ChronoDuration::minutes(5)).to_rfc3339()),
            &[execution],
            3,
            Some(
                "auto-disabled: execution started but ACI container was never created".to_string(),
            ),
            Some((now - ChronoDuration::minutes(8)).to_rfc3339()),
            true,
            now,
        );

        assert_eq!(summary.status, "failed");
        assert_eq!(
            summary.status_reason.as_deref(),
            Some(
                "Automatic retries stopped because execution started but ACI container was never created. Use Resubmit to run it again."
            )
        );
        assert!(!summary.can_cancel);
        assert!(summary.can_resubmit);
        assert!(!summary.will_retry);
        assert!(summary.retry_at.is_none());
    }

    #[test]
    fn task_status_summary_marks_due_one_shot_as_queued_for_live_user() {
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        let task = sample_one_shot_task(now - ChronoDuration::minutes(5));

        let summary = build_task_status_summary(
            &task.id.to_string(),
            "run_task",
            "slack",
            Some("Review launch checklist".to_string()),
            &task,
            "one_shot".to_string(),
            None,
            Some((now - ChronoDuration::minutes(5)).to_rfc3339()),
            &[],
            0,
            None,
            None,
            true,
            now,
        );

        assert_eq!(summary.status, "queued");
        assert_eq!(
            summary.status_reason.as_deref(),
            Some("Waiting for a worker to claim this task.")
        );
    }

    #[test]
    fn task_status_summary_marks_due_account_mirror_as_scheduled() {
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        let task = sample_one_shot_task(now - ChronoDuration::minutes(5));

        let summary = build_task_status_summary(
            &task.id.to_string(),
            "run_task",
            "slack",
            Some("Review launch checklist".to_string()),
            &task,
            "one_shot".to_string(),
            None,
            Some((now - ChronoDuration::minutes(5)).to_rfc3339()),
            &[],
            0,
            None,
            None,
            false,
            now,
        );

        assert_eq!(summary.status, "scheduled");
        assert_eq!(
            summary.status_reason.as_deref(),
            Some("Mirrored task record. Worker pickup happens from the live channel task.")
        );
    }

    #[test]
    fn task_status_summary_keeps_due_retry_off_account_mirror_queue() {
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        let started_at = now - ChronoDuration::minutes(10);
        let task = sample_one_shot_task(now - ChronoDuration::minutes(1));
        let execution = ExecutionRow {
            doc_id: Bson::Null,
            task_id: task.id.to_string(),
            execution_id: 7,
            started_at,
            finished_at: Some(now - ChronoDuration::minutes(9)),
            status: "failed".to_string(),
            error_message: Some("worker died".to_string()),
        };

        let summary = build_task_status_summary(
            &task.id.to_string(),
            "run_task",
            "slack",
            Some("Review launch checklist".to_string()),
            &task,
            "one_shot".to_string(),
            None,
            Some((now - ChronoDuration::minutes(1)).to_rfc3339()),
            &[execution],
            1,
            None,
            None,
            false,
            now,
        );

        assert_eq!(summary.status, "retry_scheduled");
        assert_eq!(
            summary.status_reason.as_deref(),
            Some("Automatic retry 1 is pending on the live channel task.")
        );
        assert!(summary.will_retry);
    }

    #[test]
    fn missing_aci_reason_stays_never_created_when_no_trace_exists() {
        let workspace = temp_workspace("aci_not_created");
        let started_at = Utc::now();
        let now = started_at + ChronoDuration::minutes(1);
        let (disable_reason, error_reason) =
            missing_aci_registry_reconciliation_reason(Some(workspace.as_path()), started_at, now);
        assert_eq!(
            disable_reason,
            "auto-disabled: execution started but ACI container was never created"
        );
        assert_eq!(
            error_reason,
            "reconciled stale running execution; ACI container not found"
        );
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn missing_aci_reason_prefers_primary_failure_when_trace_shows_fallback_stuck() {
        let workspace = temp_workspace("aci_trace_exists");
        let started_at = Utc::now();
        let now = started_at + ChronoDuration::minutes(20);
        let primary_aci_dir = workspace.join(".run_task_trace_codex_primary/aci");
        fs::create_dir_all(&primary_aci_dir).unwrap();
        fs::write(primary_aci_dir.join("container_show.json"), "{}").unwrap();
        let fallback_trace_dir = workspace.join(".run_task_trace");
        fs::create_dir_all(&fallback_trace_dir).unwrap();
        fs::write(
            fallback_trace_dir.join("metadata.json"),
            r#"{
  "backend": "claude_local",
  "current_stage": "executing_claude_local",
  "finished_at_unix_ms": null,
  "success": null
}"#,
        )
        .unwrap();

        let (disable_reason, error_reason) =
            missing_aci_registry_reconciliation_reason(Some(workspace.as_path()), started_at, now);
        assert_eq!(
            disable_reason,
            "auto-disabled: primary runner failed and fallback never reached a terminal state"
        );
        assert_eq!(
            error_reason,
            "reconciled stale running execution after primary ACI run failed and fallback never reached a terminal state"
        );
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn missing_aci_reason_ignores_stale_trace_from_prior_execution() {
        let workspace = temp_workspace("aci_old_trace");
        let old_started_at = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
        let current_started_at = Utc.with_ymd_and_hms(2026, 4, 1, 2, 0, 0).unwrap();

        let primary_aci_dir = workspace.join(".run_task_trace_codex_primary/aci");
        fs::create_dir_all(&primary_aci_dir).unwrap();
        fs::write(primary_aci_dir.join("container_show.json"), "{}").unwrap();

        let trace_dir = workspace.join(".run_task_trace");
        fs::create_dir_all(&trace_dir).unwrap();
        fs::write(
            trace_dir.join("metadata.json"),
            format!(
                r#"{{
  "backend": "codex_azure_aci",
  "current_stage": "completed",
  "started_at_unix_ms": {},
  "finished_at_unix_ms": {},
  "success": true
}}"#,
                old_started_at.timestamp_millis(),
                old_started_at.timestamp_millis() + 60_000
            ),
        )
        .unwrap();

        let (disable_reason, error_reason) = missing_aci_registry_reconciliation_reason(
            Some(workspace.as_path()),
            current_started_at,
            current_started_at + ChronoDuration::minutes(5),
        );
        assert_eq!(
            disable_reason,
            "auto-disabled: execution started but ACI container was never created"
        );
        assert_eq!(
            error_reason,
            "reconciled stale running execution; ACI container not found"
        );
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn inflight_aci_result_handling_accepts_recent_reply_artifact_activity() {
        let workspace = temp_workspace("aci_recent_reply");
        let started_at = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
        let now = started_at + ChronoDuration::minutes(40);
        let trace_dir = workspace.join(".run_task_trace");
        fs::create_dir_all(&trace_dir).unwrap();
        fs::write(
            trace_dir.join("metadata.json"),
            format!(
                r#"{{
  "backend": "codex_azure_aci",
  "current_stage": "creating_ephemeral_share",
  "started_at_unix_ms": {},
  "stage_updated_at_unix_ms": {},
  "finished_at_unix_ms": null,
  "success": null
}}"#,
                started_at.timestamp_millis(),
                started_at.timestamp_millis()
            ),
        )
        .unwrap();
        fs::write(workspace.join(".aci_recovery_context.json"), "{}").unwrap();
        fs::write(
            workspace.join("reply_email_draft.html"),
            "<html><body>fresh reply</body></html>",
        )
        .unwrap();

        assert!(
            workspace_suggests_recent_inflight_aci_result_handling(
                workspace.as_path(),
                started_at,
                now
            ),
            "a fresh reply artifact from the current run should keep result handling open"
        );
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn stale_reconciliation_failure_can_be_replaced_by_real_failure_reason() {
        let row = ExecutionRow {
            doc_id: Bson::Null,
            task_id: "task".to_string(),
            execution_id: 1,
            started_at: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
            finished_at: Some(Utc.with_ymd_and_hms(2026, 4, 1, 0, 10, 0).unwrap()),
            status: "failed".to_string(),
            error_message: Some(
                "reconciled stale running execution after an ACI-backed runner executed but no live registry record remained"
                    .to_string(),
            ),
        };

        assert!(should_replace_stale_reconciliation_terminal_row(
            &row,
            "failed",
            Some("task execution failed: Primary runner failed:"),
        ));
        assert!(!should_replace_stale_reconciliation_terminal_row(
            &row,
            "failed",
            Some("reconciled stale running execution; ACI container not found"),
        ));
    }

    #[test]
    fn workspace_peer_supersede_reason_detects_running_sibling_without_aci_evidence() {
        let workspace = temp_workspace("aci_workspace_peer_duplicate");
        let started_at = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 30).unwrap();
        let peer_started_at = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
        let peer_rows = HashMap::from([(
            "peer-task".to_string(),
            vec![ExecutionRow {
                doc_id: Bson::Null,
                task_id: "peer-task".to_string(),
                execution_id: 123,
                started_at: peer_started_at,
                finished_at: None,
                status: "running".to_string(),
                error_message: None,
            }],
        )]);

        let reason = workspace_peer_supersede_reason(
            "current-task",
            &peer_rows,
            Some(workspace.as_path()),
            started_at,
        )
        .expect("peer workspace should supersede duplicate");
        assert!(reason.contains("another task for the same workspace was already running"));
        let _ = fs::remove_dir_all(workspace);
    }
}
