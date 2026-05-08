use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::path::PathBuf;
use uuid::Uuid;

use super::types::{ScheduledTask, SchedulerError};

mod mongo;
pub mod reconciliation_alert;

pub use mongo::mark_execution_finished_by_workspace;
use mongo::MongoSchedulerStore;
pub use reconciliation_alert::{
    check_and_send_alert_if_needed, reset_reconciliation_failure_counter,
};

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

    /// Create a store using the shared MongoDB client singleton.
    /// Use this for hot paths like API request handlers.
    pub fn with_shared_client(path: PathBuf) -> Result<Self, SchedulerError> {
        Ok(Self {
            mongo: MongoSchedulerStore::with_shared_client(&path)?,
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

    pub(crate) fn replace_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
        self.mongo.replace_task(task)
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

    pub(crate) fn upsert_terminal_execution(
        &self,
        task_id: Uuid,
        execution: ExecutionRecordHandle,
        finished_at: DateTime<Utc>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        self.mongo
            .upsert_terminal_execution(task_id, execution, finished_at, status, error_message)
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

    pub(crate) fn append_execution_event(
        &self,
        task_id: &str,
        started_at: DateTime<Utc>,
        finished_at: Option<DateTime<Utc>>,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), SchedulerError> {
        self.mongo
            .append_execution_event(task_id, started_at, finished_at, status, error_message)
    }

    pub fn list_tasks_with_status(&self) -> Result<Vec<TaskStatusSummary>, SchedulerError> {
        self.mongo.list_tasks_with_status()
    }

    pub fn load_task_with_status(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskStatusSummary>, SchedulerError> {
        self.mongo.load_task_with_status(task_id)
    }

    pub fn list_task_executions(
        &self,
        task_id: &str,
    ) -> Result<Vec<TaskExecutionSummary>, SchedulerError> {
        self.mongo.list_task_executions(task_id)
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
    /// Why the task was auto-disabled, when automatic retries were stopped by the system.
    pub auto_disabled_reason: Option<String>,
    pub auto_disabled_at: Option<String>,
    /// User-facing task state derived from schedule, enabled flag, and execution history.
    pub status: String,
    /// Optional explanation for the current task state.
    pub status_reason: Option<String>,
    /// When the current user-facing state last changed.
    pub status_changed_at: Option<String>,
    /// When the next automatic retry is/was due.
    pub retry_at: Option<String>,
    /// Whether the current task state still has an automatic retry pending.
    pub will_retry: bool,
    /// Current retry counter persisted with the task.
    pub retry_count: u32,
    /// Whether the task has been running long enough to warn that it may be stuck.
    pub is_running_long: bool,
    /// Whether the dashboard should offer a cancel action for this task.
    pub can_cancel: bool,
    /// Whether the dashboard should offer a resubmit action for this task.
    pub can_resubmit: bool,
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

/// Execution history row for a task detail view.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskExecutionSummary {
    pub execution_id: i64,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
    pub duration_seconds: Option<i64>,
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
