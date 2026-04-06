use chrono::{Duration as ChronoDuration, Utc};
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::{FindOneOptions, FindOptions, UpdateOptions};
use mongodb::sync::Collection;
use mongodb::IndexModel;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use uuid::Uuid;

use crate::mongo_store::{create_client_from_env, database_from_env, ensure_index_compatible};

use super::super::types::{Schedule, ScheduledTask, SchedulerError};
use super::super::utils::{task_kind_channel, task_kind_label};
use super::super::{is_user_visible_routine_task, maybe_repair_legacy_weekday_cron_task};
use super::{
    ExecutionReconciliationSummary, ExecutionRecordHandle, RoutineSummary, TaskDebugArchiveRecord,
    TaskStatusSummary,
};

static EXECUTION_SEQ: AtomicI64 = AtomicI64::new(0);
const REQUEST_SUMMARY_MAX_CHARS: usize = 72;

#[derive(Debug, Clone)]
struct ExecutionRow {
    doc_id: Bson,
    started_at: chrono::DateTime<Utc>,
    finished_at: Option<chrono::DateTime<Utc>>,
    status: String,
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
        let db = database_from_env(&client);
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
        let result = self.tasks
            .update_one(
                filter.clone(),
                doc! {
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
                },
                UpdateOptions::builder().upsert(Some(true)).build(),
            )
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
        let result = self.tasks
            .update_one(
                filter.clone(),
                doc! {
                    "$set": {
                        "enabled": task.enabled,
                        "last_run": task.last_run.map(BsonDateTime::from_chrono).map(Bson::DateTime).unwrap_or(Bson::Null),
                        "schedule": schedule_doc(&task.schedule),
                        "task_json": task_json,
                    }
                },
                None,
            )
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

    /// Check if there's already a running execution for this task.
    ///
    /// This prevents duplicate executions when the worker process restarts
    /// and loses its in-memory claims state.
    pub(crate) fn has_running_execution(&self, task_id: &str) -> Result<bool, SchedulerError> {
        let count = self
            .executions
            .count_documents(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": task_id,
                    "status": "running",
                },
                None,
            )
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
        self.executions
            .insert_one(
                doc! {
                    "owner_scope": self.owner_scope_doc(),
                    "execution_id": execution.execution_id,
                    "task_id": task_id.to_string(),
                    "started_at": BsonDateTime::from_chrono(started_at),
                    "finished_at": Bson::Null,
                    "status": "running",
                    "error_message": Bson::Null,
                },
                None,
            )
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
        let result = self
            .executions
            .update_one(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": task_id.to_string(),
                    "execution_id": execution.execution_id,
                    "started_at": BsonDateTime::from_chrono(execution.started_at),
                    "status": "running",
                },
                doc! {
                    "$set": {
                        "finished_at": BsonDateTime::from_chrono(finished_at),
                        "status": status,
                        "error_message": error_message.map(Bson::from).unwrap_or(Bson::Null),
                    }
                },
                None,
            )
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

    pub(crate) fn reconcile_stale_running_executions(
        &self,
        now: chrono::DateTime<Utc>,
        stale_after: ChronoDuration,
    ) -> Result<ExecutionReconciliationSummary, SchedulerError> {
        let task_ids = self
            .executions
            .distinct(
                "task_id",
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "status": "running",
                },
                None,
            )
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
        self.debug_archives
            .update_one(
                doc! {
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "task_id": &archive.task_id,
                    "execution_id": archive.execution_id,
                },
                doc! {
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
                },
                UpdateOptions::builder().upsert(Some(true)).build(),
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

    fn finish_execution_row(
        &self,
        doc_id: &Bson,
        finished_at: chrono::DateTime<Utc>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        let result = self
            .executions
            .update_one(
                doc! {
                    "_id": doc_id.clone(),
                    "owner_scope.kind": &self.owner_kind,
                    "owner_scope.id": &self.owner_id,
                    "status": "running",
                },
                doc! {
                    "$set": {
                        "finished_at": BsonDateTime::from_chrono(finished_at),
                        "status": status,
                        "error_message": error_message.map(Bson::from).unwrap_or(Bson::Null),
                    }
                },
                None,
            )
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
        self.tasks
            .update_one(
                self.task_filter(task_id),
                doc! { "$inc": { "retry_count": 1i32 } },
                None,
            )
            .map_err(mongo_err)?;
        self.get_retry_count(task_id)
    }

    pub(crate) fn reset_retry_count(&self, task_id: &str) -> Result<(), SchedulerError> {
        self.tasks
            .update_one(
                self.task_filter(task_id),
                doc! { "$set": { "retry_count": 0i32 } },
                None,
            )
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
            let request_summary = derive_request_summary(&task_doc);
            let execution = self.latest_execution_for_task(task_id)?;
            let (schedule_type, next_run, run_at) = match &task.schedule {
                Schedule::Cron { next_run, .. } => {
                    ("cron".to_string(), Some(next_run.to_rfc3339()), None)
                }
                Schedule::OneShot { run_at } => {
                    ("one_shot".to_string(), None, Some(run_at.to_rfc3339()))
                }
            };
            summaries.push(TaskStatusSummary {
                id: task_id.to_string(),
                kind: task_doc.get_str("kind").unwrap_or("unknown").to_string(),
                channel: task_doc.get_str("channel").unwrap_or("email").to_string(),
                request_summary,
                enabled: task.enabled,
                created_at: task.created_at.to_rfc3339(),
                last_run: task.last_run.map(|value| value.to_rfc3339()),
                schedule_type,
                next_run,
                run_at,
                execution_status: execution
                    .as_ref()
                    .and_then(|doc| doc.get_str("status").ok())
                    .map(|value| value.to_string()),
                error_message: execution.as_ref().and_then(|doc| {
                    doc.get_str("error_message")
                        .ok()
                        .map(|value| value.to_string())
                }),
                execution_started_at: execution
                    .as_ref()
                    .and_then(|doc| datetime_field_to_rfc3339(doc, "started_at")),
            });
        }
        Ok(summaries)
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
    bson_i64(document.get("execution_id"), "execution_id")?;
    document.get_str("task_id").map_err(|err| {
        SchedulerError::Storage(format!("missing task_id for execution row: {err}"))
    })?;
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

    Ok(ExecutionRow {
        doc_id,
        started_at,
        finished_at,
        status,
    })
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

fn deserialize_task_document(document: &Document) -> Result<ScheduledTask, SchedulerError> {
    let task_json = document.get_str("task_json").map_err(|err| {
        SchedulerError::Storage(format!("missing task_json for task document: {err}"))
    })?;
    serde_json::from_str(task_json)
        .map_err(|err| SchedulerError::Storage(format!("invalid task_json: {err}")))
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

fn derive_request_summary(task_doc: &Document) -> Option<String> {
    let task_json = task_doc.get_str("task_json").ok()?;
    let task_value: serde_json::Value = serde_json::from_str(task_json).ok()?;
    let task_kind = task_value.pointer("/kind/type").and_then(|v| v.as_str())?;

    match task_kind {
        "send_email" => task_value
            .pointer("/kind/subject")
            .and_then(|v| v.as_str())
            .and_then(normalize_summary_text),
        "run_task" => {
            let workspace_dir = task_value
                .pointer("/kind/workspace_dir")
                .and_then(|v| v.as_str())?;
            let channel = task_value
                .pointer("/kind/channel")
                .and_then(|v| v.as_str())
                .or_else(|| task_doc.get_str("channel").ok())
                .unwrap_or("");
            let thread_epoch = task_value
                .pointer("/kind/thread_epoch")
                .and_then(|v| v.as_u64());
            derive_run_task_summary(Path::new(workspace_dir), channel, thread_epoch)
        }
        _ => None,
    }
}

fn derive_run_task_summary(
    workspace_dir: &Path,
    channel: &str,
    thread_epoch: Option<u64>,
) -> Option<String> {
    let incoming_dir = workspace_dir.join("incoming_email");
    if !incoming_dir.exists() {
        return None;
    }

    match channel {
        "email" => derive_email_summary(&incoming_dir),
        "google_docs" => derive_google_workspace_summary(&incoming_dir, "gdocs", thread_epoch),
        "google_sheets" => derive_google_workspace_summary(&incoming_dir, "gsheets", thread_epoch),
        "google_slides" => derive_google_workspace_summary(&incoming_dir, "gslides", thread_epoch),
        "discord" => derive_discord_summary(&incoming_dir, thread_epoch),
        "slack" => derive_text_file_summary(&incoming_dir, &["_slack_message.txt"], thread_epoch),
        "sms" => derive_text_file_summary(&incoming_dir, &["_sms_message.txt"], thread_epoch),
        "bluebubbles" => {
            derive_text_file_summary(&incoming_dir, &["_bluebubbles_message.txt"], thread_epoch)
        }
        "telegram" => {
            derive_header_text_file_summary(&incoming_dir, &["_telegram.txt"], thread_epoch)
        }
        "whatsapp" => {
            derive_header_text_file_summary(&incoming_dir, &["_whatsapp.txt"], thread_epoch)
        }
        "wechat" => derive_header_text_file_summary(&incoming_dir, &["_wechat.txt"], thread_epoch),
        "lark" => derive_header_text_file_summary(&incoming_dir, &["_lark.txt"], thread_epoch),
        _ => None,
    }
}

fn derive_email_summary(incoming_dir: &Path) -> Option<String> {
    let payload_path = incoming_dir.join("postmark_payload.json");
    let raw_payload = fs::read_to_string(payload_path).ok()?;
    let payload_value: serde_json::Value = serde_json::from_str(&raw_payload).ok()?;

    payload_value
        .get("Subject")
        .and_then(|v| v.as_str())
        .and_then(normalize_summary_text)
        .or_else(|| {
            payload_value
                .get("StrippedTextReply")
                .and_then(|v| v.as_str())
                .and_then(normalize_summary_text)
        })
        .or_else(|| {
            payload_value
                .get("TextBody")
                .and_then(|v| v.as_str())
                .and_then(normalize_summary_text)
        })
}

fn derive_google_workspace_summary(
    incoming_dir: &Path,
    file_prefix: &str,
    thread_epoch: Option<u64>,
) -> Option<String> {
    let comment_suffix = format!("_{}_comment.json", file_prefix);
    let comment_path = file_with_epoch_or_latest(incoming_dir, &comment_suffix, thread_epoch);
    if let Some(comment_path) = comment_path {
        if let Ok(raw_comment) = fs::read_to_string(comment_path) {
            if let Ok(comment) = serde_json::from_str::<serde_json::Value>(&raw_comment) {
                if let Some(summary) = comment
                    .get("content")
                    .and_then(|v| v.as_str())
                    .and_then(normalize_summary_text)
                {
                    return Some(summary);
                }
            }
        }
    }

    let meta_suffix = format!("_{}_meta.json", file_prefix);
    let meta_path = file_with_epoch_or_latest(incoming_dir, &meta_suffix, thread_epoch)?;
    let raw_meta = fs::read_to_string(meta_path).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&raw_meta).ok()?;
    let file_name = meta.get("file_name").and_then(|v| v.as_str())?;

    normalize_summary_text(&format!("Comment on {}", file_name))
}

fn derive_discord_summary(incoming_dir: &Path, thread_epoch: Option<u64>) -> Option<String> {
    let raw = read_text_by_epoch_or_latest(incoming_dir, "_discord_message.txt", thread_epoch)?;
    if let Some((_, user_section)) = raw.split_once("User message:\n") {
        if let Some(summary) = normalize_summary_text(user_section) {
            return Some(summary);
        }
    }
    normalize_summary_text(&raw)
}

fn derive_text_file_summary(
    incoming_dir: &Path,
    suffixes: &[&str],
    thread_epoch: Option<u64>,
) -> Option<String> {
    // Use the first suffix for epoch-based lookup
    let raw = read_text_by_epoch_or_latest(incoming_dir, suffixes[0], thread_epoch)?;
    normalize_summary_text(&raw)
}

fn derive_header_text_file_summary(
    incoming_dir: &Path,
    suffixes: &[&str],
    thread_epoch: Option<u64>,
) -> Option<String> {
    // Use the first suffix for epoch-based lookup
    let raw = read_text_by_epoch_or_latest(incoming_dir, suffixes[0], thread_epoch)?;
    extract_header_file_body_summary(&raw).or_else(|| normalize_summary_text(&raw))
}

/// Read a file by thread_epoch if available, otherwise fall back to latest file with suffix.
fn read_text_by_epoch_or_latest(
    incoming_dir: &Path,
    suffix: &str,
    thread_epoch: Option<u64>,
) -> Option<String> {
    let path = file_with_epoch_or_latest(incoming_dir, suffix, thread_epoch)?;
    fs::read_to_string(path).ok()
}

/// Get file path by thread_epoch if available, otherwise fall back to latest file with suffix.
/// Finds files ending with suffix, extracts the numeric prefix, and matches against epoch.
/// Falls back to latest_file_with_suffix if no match found or epoch is None.
fn file_with_epoch_or_latest(
    incoming_dir: &Path,
    suffix: &str,
    thread_epoch: Option<u64>,
) -> Option<PathBuf> {
    if let Some(epoch) = thread_epoch {
        // Find file matching the epoch by parsing numeric prefix
        if let Some(path) = find_file_by_epoch(incoming_dir, suffix, epoch) {
            return Some(path);
        }
    }
    // Fallback to latest file with suffix
    latest_file_with_suffix(incoming_dir, &[suffix])
}

/// Find a file by extracting numeric prefix and matching against epoch.
/// E.g., "0002_lark.txt" with suffix "_lark.txt" → prefix "0002" → 2 matches epoch=2
fn find_file_by_epoch(incoming_dir: &Path, suffix: &str, epoch: u64) -> Option<PathBuf> {
    for entry in fs::read_dir(incoming_dir).ok()? {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(suffix) {
            continue;
        }
        // Strip suffix and parse numeric prefix
        let prefix = name.strip_suffix(suffix)?;
        if let Ok(file_epoch) = prefix.parse::<u64>() {
            if file_epoch == epoch {
                return Some(entry.path());
            }
        }
    }
    None
}

fn read_latest_text_by_suffix(incoming_dir: &Path, suffixes: &[&str]) -> Option<String> {
    let path = latest_file_with_suffix(incoming_dir, suffixes)?;
    fs::read_to_string(path).ok()
}

fn latest_file_with_suffix(incoming_dir: &Path, suffixes: &[&str]) -> Option<PathBuf> {
    let mut matches: Vec<(String, PathBuf)> = Vec::new();

    for entry in fs::read_dir(incoming_dir).ok()? {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if suffixes.iter().any(|suffix| name.ends_with(suffix)) {
            matches.push((name, entry.path()));
        }
    }

    matches.sort_by(|a, b| a.0.cmp(&b.0));
    matches.pop().map(|(_, path)| path)
}

fn extract_header_file_body_summary(raw: &str) -> Option<String> {
    let mut body_started = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            body_started = true;
            continue;
        }

        if !body_started
            && (trimmed.starts_with("From:")
                || trimmed.starts_with("Date:")
                || trimmed.starts_with("To:")
                || trimmed.starts_with("Subject:"))
        {
            continue;
        }

        return clean_summary_line(trimmed);
    }

    None
}

fn normalize_summary_text(raw: &str) -> Option<String> {
    let first_line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    clean_summary_line(first_line)
}

fn clean_summary_line(line: &str) -> Option<String> {
    let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    Some(truncate_summary(&compact, REQUEST_SUMMARY_MAX_CHARS))
}

fn truncate_summary(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut output = String::new();

    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => output.push(ch),
            None => return output,
        }
    }

    if chars.next().is_some() {
        output.push_str("...");
    }

    output
}

fn default_routine_name(channel: &str) -> String {
    match channel {
        "slack" => "Scheduled Slack work".to_string(),
        "discord" => "Scheduled Discord work".to_string(),
        "email" => "Scheduled email work".to_string(),
        "google_docs" => "Scheduled Google Docs work".to_string(),
        "google_sheets" => "Scheduled Google Sheets work".to_string(),
        "google_slides" => "Scheduled Google Slides work".to_string(),
        "lark" => "Scheduled Lark work".to_string(),
        _ => "Scheduled Oliver work".to_string(),
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
    use std::fs;
    use std::path::PathBuf;

    use mongodb::bson::doc;
    use tempfile::TempDir;

    use super::{derive_request_summary, resolve_owner_scope};

    #[test]
    fn resolve_owner_scope_extracts_user_id() {
        let path = PathBuf::from("/tmp/runtime/users/user-123/state/tasks.db");
        let scope = resolve_owner_scope(&path);
        assert_eq!(scope.0, "user");
        assert_eq!(scope.1, "user-123");
    }

    #[test]
    fn derive_request_summary_prefers_send_email_subject() {
        let task_json = serde_json::json!({
            "kind": {
                "type": "send_email",
                "subject": "Weekly analytics summary and next actions"
            }
        })
        .to_string();
        let doc = doc! {
            "task_json": task_json,
            "channel": "email",
        };

        let summary = derive_request_summary(&doc);
        assert_eq!(
            summary.as_deref(),
            Some("Weekly analytics summary and next actions")
        );
    }

    #[test]
    fn derive_request_summary_reads_latest_slack_message() {
        let temp = TempDir::new().expect("tempdir");
        let incoming_dir = temp.path().join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("create incoming_email");
        fs::write(
            incoming_dir.join("00001_slack_message.txt"),
            "Earlier message",
        )
        .expect("write old message");
        fs::write(
            incoming_dir.join("00002_slack_message.txt"),
            "Please draft a concise project update for the team.",
        )
        .expect("write latest message");

        let task_json = serde_json::json!({
            "kind": {
                "type": "run_task",
                "workspace_dir": temp.path().to_string_lossy(),
                "channel": "slack"
            }
        })
        .to_string();
        let doc = doc! {
            "task_json": task_json,
            "channel": "slack",
        };

        let summary = derive_request_summary(&doc);
        assert_eq!(
            summary.as_deref(),
            Some("Please draft a concise project update for the team.")
        );
    }

    #[test]
    fn derive_request_summary_skips_header_lines_for_telegram_text() {
        let temp = TempDir::new().expect("tempdir");
        let incoming_dir = temp.path().join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("create incoming_email");
        fs::write(
            incoming_dir.join("0001_telegram.txt"),
            "From: User (123)\nDate: 2026-03-13T20:00:00Z\n\nReview the attached budget and flag risks.",
        )
        .expect("write telegram message");

        let task_json = serde_json::json!({
            "kind": {
                "type": "run_task",
                "workspace_dir": temp.path().to_string_lossy(),
                "channel": "telegram"
            }
        })
        .to_string();
        let doc = doc! {
            "task_json": task_json,
            "channel": "telegram",
        };

        let summary = derive_request_summary(&doc);
        assert_eq!(
            summary.as_deref(),
            Some("Review the attached budget and flag risks.")
        );
    }

    #[test]
    fn derive_request_summary_uses_thread_epoch_for_lark() {
        let temp = TempDir::new().expect("tempdir");
        let incoming_dir = temp.path().join("incoming_email");
        fs::create_dir_all(&incoming_dir).expect("create incoming_email");

        // Create multiple lark messages
        fs::write(
            incoming_dir.join("0002_lark.txt"),
            "From: ou_user1\nDate: 2026-03-13T10:00:00Z\n\nFirst message about project setup.",
        )
        .expect("write first message");
        fs::write(
            incoming_dir.join("0003_lark.txt"),
            "From: ou_user1\nDate: 2026-03-13T11:00:00Z\n\nSecond message about code review.",
        )
        .expect("write second message");
        fs::write(
            incoming_dir.join("0004_lark.txt"),
            "From: ou_user1\nDate: 2026-03-13T12:00:00Z\n\nThird message about deployment.",
        )
        .expect("write third message");

        // Task with thread_epoch=2 should read 0002_lark.txt
        let task_json_epoch2 = serde_json::json!({
            "kind": {
                "type": "run_task",
                "workspace_dir": temp.path().to_string_lossy(),
                "channel": "lark",
                "thread_epoch": 2
            }
        })
        .to_string();
        let doc_epoch2 = doc! {
            "task_json": task_json_epoch2,
            "channel": "lark",
        };
        let summary_epoch2 = derive_request_summary(&doc_epoch2);
        assert_eq!(
            summary_epoch2.as_deref(),
            Some("First message about project setup.")
        );

        // Task with thread_epoch=3 should read 0003_lark.txt
        let task_json_epoch3 = serde_json::json!({
            "kind": {
                "type": "run_task",
                "workspace_dir": temp.path().to_string_lossy(),
                "channel": "lark",
                "thread_epoch": 3
            }
        })
        .to_string();
        let doc_epoch3 = doc! {
            "task_json": task_json_epoch3,
            "channel": "lark",
        };
        let summary_epoch3 = derive_request_summary(&doc_epoch3);
        assert_eq!(
            summary_epoch3.as_deref(),
            Some("Second message about code review.")
        );

        // Task without thread_epoch should fall back to latest (0004_lark.txt)
        let task_json_no_epoch = serde_json::json!({
            "kind": {
                "type": "run_task",
                "workspace_dir": temp.path().to_string_lossy(),
                "channel": "lark"
            }
        })
        .to_string();
        let doc_no_epoch = doc! {
            "task_json": task_json_no_epoch,
            "channel": "lark",
        };
        let summary_no_epoch = derive_request_summary(&doc_no_epoch);
        assert_eq!(
            summary_no_epoch.as_deref(),
            Some("Third message about deployment.")
        );
    }
}
