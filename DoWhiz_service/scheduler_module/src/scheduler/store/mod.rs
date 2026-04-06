use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::path::PathBuf;
use uuid::Uuid;

use super::types::{ScheduledTask, SchedulerError};

mod mongo;

use mongo::MongoSchedulerStore;

#[derive(Debug)]
pub(crate) struct SchedulerStore {
    mongo: MongoSchedulerStore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutionRecordHandle {
    pub execution_id: i64,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutionReconciliationSummary {
    pub superseded_count: usize,
    pub failed_count: usize,
}

impl ExecutionReconciliationSummary {
    pub(crate) fn total_reconciled(self) -> usize {
        self.superseded_count + self.failed_count
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.superseded_count += other.superseded_count;
        self.failed_count += other.failed_count;
    }
}

impl SchedulerStore {
    pub(crate) fn new(path: PathBuf) -> Result<Self, SchedulerError> {
        Ok(Self {
            mongo: MongoSchedulerStore::new(&path)?,
        })
    }

    pub(crate) fn load_tasks(&self) -> Result<Vec<ScheduledTask>, SchedulerError> {
        self.mongo.load_tasks()
    }

    pub(crate) fn load_task_by_id(
        &self,
        task_id: &str,
    ) -> Result<Option<ScheduledTask>, SchedulerError> {
        self.mongo.load_task_by_id(task_id)
    }

    pub(crate) fn insert_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
        self.mongo.insert_task(task)
    }

    pub(crate) fn update_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
        self.mongo.update_task(task)
    }

    /// Check if there's already a running execution for this task.
    ///
    /// This prevents duplicate executions when the worker process restarts
    /// and loses its in-memory claims state.
    pub(crate) fn has_running_execution(&self, task_id: &str) -> Result<bool, SchedulerError> {
        self.mongo.has_running_execution(task_id)
    }

    pub(crate) fn record_execution_start(
        &self,
        task_id: Uuid,
        started_at: DateTime<Utc>,
    ) -> Result<ExecutionRecordHandle, SchedulerError> {
        self.mongo.record_execution_start(task_id, started_at)
    }

    pub(crate) fn record_execution_finish(
        &self,
        task_id: Uuid,
        execution: ExecutionRecordHandle,
        finished_at: DateTime<Utc>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        self.mongo
            .record_execution_finish(task_id, execution, finished_at, status, error_message)
    }

    pub(crate) fn reconcile_stale_running_executions(
        &self,
        now: DateTime<Utc>,
        stale_after: ChronoDuration,
    ) -> Result<ExecutionReconciliationSummary, SchedulerError> {
        self.mongo
            .reconcile_stale_running_executions(now, stale_after)
    }

    pub(crate) fn reconcile_stale_running_executions_for_task(
        &self,
        task_id: &str,
        now: DateTime<Utc>,
        stale_after: ChronoDuration,
    ) -> Result<ExecutionReconciliationSummary, SchedulerError> {
        self.mongo
            .reconcile_stale_running_executions_for_task(task_id, now, stale_after)
    }

    pub(crate) fn record_task_debug_archive(
        &self,
        archive: &TaskDebugArchiveRecord,
    ) -> Result<(), SchedulerError> {
        self.mongo.record_task_debug_archive(archive)
    }

    pub(crate) fn get_retry_count(&self, task_id: &str) -> Result<u32, SchedulerError> {
        self.mongo.get_retry_count(task_id)
    }

    pub(crate) fn increment_retry_count(&self, task_id: &str) -> Result<u32, SchedulerError> {
        self.mongo.increment_retry_count(task_id)
    }

    pub(crate) fn reset_retry_count(&self, task_id: &str) -> Result<(), SchedulerError> {
        self.mongo.reset_retry_count(task_id)
    }

    pub(crate) fn disable_task_by_id(&self, task_id: &str) -> Result<(), SchedulerError> {
        if let Some(mut task) = self.load_task_by_id(task_id)? {
            task.enabled = false;
            self.update_task(&task)?;
        }
        Ok(())
    }

    pub fn list_tasks_with_status(&self) -> Result<Vec<TaskStatusSummary>, SchedulerError> {
        self.mongo.list_tasks_with_status()
    }

    pub fn list_routines_with_status(&self) -> Result<Vec<RoutineSummary>, SchedulerError> {
        self.mongo.list_routines_with_status()
    }
}

/// Summary of a task with its latest execution status.
/// Used for API responses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskStatusSummary {
    pub id: String,
    pub kind: String,
    pub channel: String,
    /// Short, user-facing summary derived from the original request content when available.
    pub request_summary: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub last_run: Option<String>,
    pub schedule_type: String,
    pub next_run: Option<String>,
    pub run_at: Option<String>,
    /// Status from the latest execution: "running", "success", "failed", "superseded", or None if never executed
    pub execution_status: Option<String>,
    pub error_message: Option<String>,
    pub execution_started_at: Option<String>,
}

/// Summary of a user-visible scheduled run_task surfaced as a dashboard routine.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoutineSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub channel: String,
    pub enabled: bool,
    pub schedule_type: String,
    pub next_run: Option<String>,
    pub run_at: Option<String>,
    pub last_run: Option<String>,
    pub execution_status: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub is_recurring: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskDebugArchiveRecord {
    pub task_id: String,
    pub execution_id: i64,
    pub archive_type: String,
    pub archive_version: i32,
    pub status: String,
    pub storage_backend: String,
    pub storage_account: Option<String>,
    pub blob_container: Option<String>,
    pub blob_path: Option<String>,
    pub blob_reference: Option<String>,
    pub local_fallback_path: Option<String>,
    pub sha256: String,
    pub size_bytes: i64,
    pub runner: String,
    pub model: String,
    pub deploy_target: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub archive_build_duration_ms: i64,
    pub upload_duration_ms: i64,
    pub workspace_before_file_count: i64,
    pub workspace_after_file_count: i64,
    pub redacted_file_count: i64,
    pub skipped_file_count: i64,
    pub has_workspace_before: bool,
    pub has_workspace_after: bool,
    pub has_run_task_trace: bool,
    pub has_aci_logs: bool,
    pub error_summary: Option<String>,
    pub created_at: DateTime<Utc>,
}
