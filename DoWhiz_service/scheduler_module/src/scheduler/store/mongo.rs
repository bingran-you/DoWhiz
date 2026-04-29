use chrono::{Duration as ChronoDuration, Utc};
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::{FindOneOptions, FindOptions, UpdateOptions};
use mongodb::sync::{Client, Collection};
use mongodb::IndexModel;
use run_task_module::{find_aci_container_by_workspace, query_aci_container_status, AciContainerStatus};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use uuid::Uuid;

use crate::mongo_store::{
    create_client_from_env, database_from_env, ensure_index_compatible, get_shared_client,
    retry_mongo_read, retry_mongo_write,
};

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
        let task_json = serde_json::to_string(task)
            .map_err(|err| SchedulerError::Storage(format!("serialize task failed: {err}")))?;
        let filter = self.task_filter(&task.id.to_string());
        let update = doc! {
            "$set": {
                "enabled": task.enabled,
                "last_run": task.last_run.map(BsonDateTime::from_chrono).map(Bson::DateTime).unwrap_or(Bson::Null),
                "schedule": schedule_doc(&task.schedule),
                "task_json": task_json,
            }
        };
        let result = retry_mongo_write("tasks.update_task", || {
            self.tasks.update_one(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;

        // Log warning if no document was matched - this indicates a bug
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
                "update_task succeeded: task_id={} matched={} modified={} enabled={}",
                task.id,
                result.matched_count,
                result.modified_count,
                task.enabled
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
        let filter = self.task_filter(task_id);
        let update = doc! {
            "$set": {
                "enabled": false,
                "auto_disabled_reason": reason,
                "auto_disabled_at": BsonDateTime::from_chrono(Utc::now()),
            }
        };
        let result = retry_mongo_write("tasks.disable_task_by_id", || {
            self.tasks.update_one(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;

        if result.matched_count == 0 {
            tracing::warn!(
                "disable_task_by_id matched 0 documents: task_id={} owner_scope=({}, {})",
                task_id,
                self.owner_kind,
                self.owner_id
            );
        } else {
            tracing::info!(
                "disable_task_by_id succeeded: task_id={} reason={}",
                task_id,
                reason
            );
        }
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
            return Err(SchedulerError::Storage(format!(
                "missing running execution row for task {} execution_id={} started_at={}",
                task_id,
                execution.execution_id,
                execution.started_at.to_rfc3339()
            )));
        }
        Ok(())
    }

    /// Get workspace_dir from a task's task_json field.
    /// Returns None if task not found or workspace_dir cannot be parsed.
    fn get_task_workspace_dir(&self, task_id: &str) -> Option<String> {
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
        parsed
            .get("kind")?
            .get("workspace_dir")?
            .as_str()
            .map(|s| s.to_string())
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

        let mut summary = ExecutionReconciliationSummary::default();
        for row in rows.iter().filter(|row| row.status == "running") {
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
            } else if let Some(ref rg) = aci_resource_group {
                // Look up ACI container by workspace_dir from tasks
                let workspace_dir = self.get_task_workspace_dir(task_id);
                let container_record = workspace_dir
                    .as_ref()
                    .and_then(|ws| find_aci_container_by_workspace(ws));

                match container_record {
                    Some(record) => {
                        // Container found in registry, check its actual Azure status
                        match query_aci_container_status(&record.container_name, rg) {
                            AciContainerStatus::NotFound => {
                                // Container was registered but no longer exists in Azure
                                // This means it completed but execution wasn't marked done (crash/restart)
                                Some((
                                    "failed",
                                    "reconciled stale running execution; ACI container was registered but no longer exists in Azure".to_string(),
                                ))
                            }
                            AciContainerStatus::Terminal(state) => Some((
                                "failed",
                                format!(
                                    "reconciled stale running execution; ACI container terminated with state: {}",
                                    state
                                ),
                            )),
                            _ => {
                                // Container still running or error querying - fall through to stale check
                                if row.started_at <= stale_before {
                                    Some((
                                        "failed",
                                        format!(
                                            "reconciled stale running execution after worker restart; execution exceeded {}s without a terminal status",
                                            stale_timeout_secs
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
                        let aci_grace_period = ChronoDuration::minutes(60);
                        let execution_age = now - row.started_at;

                        if execution_age > aci_grace_period {
                            // Past grace period - task died in "danger zone" between
                            // record_execution_start() and ACI container creation
                            if let Err(e) = self.disable_task_by_id(
                                task_id,
                                "auto-disabled: execution started but ACI container was never created",
                            ) {
                                tracing::error!(
                                    "failed to disable task {} after ACI not found: {}",
                                    task_id,
                                    e
                                );
                            }
                            Some((
                                "failed",
                                "reconciled stale running execution; ACI container not found".to_string(),
                            ))
                        } else {
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
            return Err(SchedulerError::Storage(
                "missing running execution row while reconciling stale execution".to_string(),
            ));
        }
        Ok(())
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
        let filter = self.task_filter(task_id);
        let update = doc! { "$inc": { "retry_count": 1i32 } };
        retry_mongo_write("tasks.increment_retry_count", || {
            self.tasks.update_one(filter.clone(), update.clone(), None)
        })
        .map_err(mongo_err)?;
        self.get_retry_count(task_id)
    }

    pub(crate) fn reset_retry_count(&self, task_id: &str) -> Result<(), SchedulerError> {
        let filter = self.task_filter(task_id);
        let update = doc! { "$set": { "retry_count": 0i32 } };
        retry_mongo_write("tasks.reset_retry_count", || {
            self.tasks.update_one(filter.clone(), update.clone(), None)
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
    now: chrono::DateTime<Utc>,
) -> TaskStatusSummary {
    let latest_execution = executions.first();
    let derived_status = derive_user_task_status(task, latest_execution, retry_count, now);

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
        status: derived_status.status.to_string(),
        status_reason: derived_status.status_reason,
        status_changed_at: derived_status.status_changed_at,
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
    is_running_long: bool,
    can_cancel: bool,
    can_resubmit: bool,
}

fn derive_user_task_status(
    task: &ScheduledTask,
    latest_execution: Option<&ExecutionRow>,
    retry_count: u32,
    now: chrono::DateTime<Utc>,
) -> DerivedTaskStatus {
    let is_one_shot = matches!(&task.schedule, Schedule::OneShot { .. });
    let is_run_task = matches!(&task.kind, TaskKind::RunTask(_));

    let mut status_reason = None;
    let mut is_running_long = false;
    let (status, status_changed_at) = if let Some(row) = latest_execution {
        let mut status = "scheduled";
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
                if task.enabled && is_one_shot {
                    match &task.schedule {
                        Schedule::OneShot { run_at } if *run_at > now => {
                            status = "retry_scheduled";
                            let retry_prefix = if retry_count > 0 {
                                format!("Retry {retry_count} scheduled")
                            } else {
                                "Retry scheduled".to_string()
                            };
                            status_reason =
                                Some(format!("{retry_prefix} for {}", run_at.to_rfc3339()));
                        }
                        Schedule::OneShot { .. } => {
                            status = "queued";
                            status_reason = Some(
                                "Retry is due and waiting for a worker to pick it up.".to_string(),
                            );
                        }
                        Schedule::Cron { .. } => {
                            status = "failed";
                        }
                    }
                } else {
                    status = "failed";
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
                        status = "queued";
                        status_reason =
                            Some("Waiting for a worker to pick up this task.".to_string());
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
        is_running_long,
        can_cancel,
        can_resubmit,
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
    let mut components: Vec<String> = Vec::new();
    for component in path.components() {
        if let Some(value) = component.as_os_str().to_str() {
            components.push(value.to_string());
        }
    }

    for (idx, value) in components.iter().enumerate() {
        if value == "users" {
            if let Some(owner_id) = components.get(idx + 1) {
                return ("user".to_string(), owner_id.to_string());
            }
        }
    }

    if path.file_name().and_then(|v| v.to_str()) == Some("tasks.db") {
        if let Some(state_dir) = path.parent() {
            if state_dir.file_name().and_then(|v| v.to_str()) == Some("state") {
                if let Some(owner_dir) = state_dir.parent() {
                    if let Some(owner_id) = owner_dir.file_name().and_then(|v| v.to_str()) {
                        return ("user".to_string(), owner_id.to_string());
                    }
                }
            }
        }
    }

    let hashed = format!("{:x}", md5::compute(path.to_string_lossy().as_bytes()));
    ("path_scope".to_string(), hashed)
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

    // Find task_id by matching workspace_dir in task_json
    let workspace_str = workspace_path.to_string_lossy();
    let filter = doc! {
        "owner_scope.kind": &owner_kind,
        "owner_scope.id": &owner_id,
    };

    let cursor = retry_mongo_read("tasks.find_for_workspace_lookup", || {
        tasks.find(filter.clone(), None)
    })
    .map_err(mongo_err)?;

    let mut task_id: Option<String> = None;
    for doc_result in cursor {
        let doc = doc_result.map_err(mongo_err)?;
        if let Ok(task_json) = doc.get_str("task_json") {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(task_json) {
                if let Some(ws) = parsed.get("kind").and_then(|k| k.get("workspace_dir")).and_then(|v| v.as_str()) {
                    if ws == workspace_str {
                        task_id = doc.get_str("task_id").ok().map(|s| s.to_string());
                        break;
                    }
                }
            }
        }
    }

    let Some(task_id) = task_id else {
        tracing::debug!(
            "no task found for workspace {} - cannot mark execution",
            workspace_path.display()
        );
        return Ok(false);
    };

    // Find running execution for this task
    let exec_filter = doc! {
        "owner_scope.kind": &owner_kind,
        "owner_scope.id": &owner_id,
        "task_id": &task_id,
        "status": "running",
    };

    let running_exec = retry_mongo_read("task_executions.find_running_for_recovery", || {
        executions.find_one(exec_filter.clone(), None)
    })
    .map_err(mongo_err)?;

    let Some(exec_doc) = running_exec else {
        tracing::debug!(
            "no running execution found for task {} - may already be marked",
            task_id
        );
        return Ok(false);
    };

    let doc_id = exec_doc.get("_id").cloned().unwrap_or(Bson::Null);
    let now = Utc::now();

    // Mark as finished with the given status
    let update = doc! {
        "$set": {
            "status": status,
            "finished_at": BsonDateTime::from_chrono(now),
            "error_message": error_message,
        }
    };

    retry_mongo_write("task_executions.mark_finished_by_recovery", || {
        executions.update_one(doc! { "_id": doc_id.clone() }, update.clone(), None)
    })
    .map_err(mongo_err)?;

    tracing::info!(
        "ACI recovery marked execution as {}: task_id={} workspace={}",
        status,
        task_id,
        workspace_path.display()
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
    use std::path::PathBuf;

    use chrono::{Duration as ChronoDuration, TimeZone, Utc};
    use mongodb::bson::Bson;

    use super::{build_task_status_summary, resolve_owner_scope, ExecutionRow};
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

    #[test]
    fn resolve_owner_scope_extracts_user_id() {
        let path = PathBuf::from("/tmp/runtime/users/user-123/state/tasks.db");
        let scope = resolve_owner_scope(&path);
        assert_eq!(scope.0, "user");
        assert_eq!(scope.1, "user-123");
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
            now,
        );

        assert_eq!(summary.execution_status, None);
        assert_eq!(summary.status, "scheduled");
        assert!(summary.can_cancel);
        assert!(!summary.can_resubmit);
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
            now,
        );

        assert_eq!(summary.status, "failed");
        assert!(!summary.can_cancel);
        assert!(summary.can_resubmit);
    }
}
