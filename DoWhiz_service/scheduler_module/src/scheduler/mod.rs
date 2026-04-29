mod actions;
mod core;
mod debug_archive;
mod email_attachments;
mod executor;
pub(crate) mod outbound;
mod reply;
mod schedule;
mod snapshot;
mod store;
pub(crate) mod task_view;
mod types;
mod utils;

pub use core::Scheduler;
pub use executor::{ModuleExecutor, TaskExecutor};
pub use store::{check_and_send_alert_if_needed, RoutineSummary, TaskExecutionSummary, TaskStatusSummary};
pub use types::{
    RunTaskTask, Schedule, ScheduledTask, SchedulerError, SendReplyTask, TaskExecution, TaskKind,
};
pub use utils::{load_google_access_token_from_service_env, load_notion_access_token_for_account};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::fs;
use std::path::Path;

use self::schedule::{next_run_after, normalize_weekday_cron_expression};

const ROUTINE_ONE_SHOT_DELAY_THRESHOLD_MINUTES: i64 = 5;

/// Load task status summaries for the owner scope derived from `tasks_db_path`.
/// Returns an empty vector if the storage backend can't be reached.
pub fn load_tasks_with_status(tasks_db_path: &Path) -> Vec<TaskStatusSummary> {
    try_load_tasks_with_status(tasks_db_path).unwrap_or_default()
}

/// Load task status summaries for the owner scope derived from `tasks_db_path`.
/// Returns a storage error if the backend can't be reached.
pub fn try_load_tasks_with_status(
    tasks_db_path: &Path,
) -> Result<Vec<TaskStatusSummary>, SchedulerError> {
    let store = store::SchedulerStore::new(tasks_db_path.to_path_buf())?;
    store.list_tasks_with_status()
}

/// Load task status summaries using the shared MongoDB client.
/// Use this for API request handlers to avoid connection pool exhaustion.
pub fn try_load_tasks_with_status_shared(
    tasks_db_path: &Path,
) -> Result<Vec<TaskStatusSummary>, SchedulerError> {
    let store = store::SchedulerStore::with_shared_client(tasks_db_path.to_path_buf())?;
    store.list_tasks_with_status()
}

/// Load a single task status summary by ID from the owner scope derived from `tasks_db_path`.
pub fn try_load_task_with_status(
    tasks_db_path: &Path,
    task_id: &str,
) -> Result<Option<TaskStatusSummary>, SchedulerError> {
    let store = store::SchedulerStore::new(tasks_db_path.to_path_buf())?;
    store.load_task_with_status(task_id)
}

/// Load execution history for a single task from the owner scope derived from `tasks_db_path`.
pub fn try_load_task_executions(
    tasks_db_path: &Path,
    task_id: &str,
) -> Result<Vec<TaskExecutionSummary>, SchedulerError> {
    let store = store::SchedulerStore::new(tasks_db_path.to_path_buf())?;
    store.list_task_executions(task_id)
}

/// Append a synthetic execution history event for a task.
pub fn append_task_execution_event(
    tasks_db_path: &Path,
    task_id: &str,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    status: &str,
    error_message: Option<&str>,
) -> Result<(), SchedulerError> {
    let store = store::SchedulerStore::new(tasks_db_path.to_path_buf())?;
    store.append_execution_event(task_id, started_at, finished_at, status, error_message)
}

/// Load account/user-visible routine summaries for the owner scope derived from `tasks_db_path`.
/// Returns an empty vector if the storage backend can't be reached.
pub fn load_routines_with_status(tasks_db_path: &Path) -> Vec<RoutineSummary> {
    try_load_routines_with_status(tasks_db_path).unwrap_or_default()
}

/// Load account/user-visible routine summaries for the owner scope derived from `tasks_db_path`.
/// Returns a storage error if the backend can't be reached.
pub fn try_load_routines_with_status(
    tasks_db_path: &Path,
) -> Result<Vec<RoutineSummary>, SchedulerError> {
    let store = store::SchedulerStore::new(tasks_db_path.to_path_buf())?;
    store.list_routines_with_status()
}

/// Load a single scheduled task by ID from the owner scope derived from `tasks_db_path`.
pub fn load_scheduled_task(
    tasks_db_path: &Path,
    task_id: &str,
) -> Result<Option<ScheduledTask>, SchedulerError> {
    let store = store::SchedulerStore::new(tasks_db_path.to_path_buf())?;
    store.load_task_by_id(task_id)
}

/// Persist a scheduled task update back into scheduler storage.
pub fn persist_scheduled_task(
    tasks_db_path: &Path,
    task: &ScheduledTask,
) -> Result<(), SchedulerError> {
    let store = store::SchedulerStore::new(tasks_db_path.to_path_buf())?;
    store.update_task(task)
}

/// Insert a new scheduled task into scheduler storage.
pub fn insert_scheduled_task(
    tasks_db_path: &Path,
    task: &ScheduledTask,
) -> Result<(), SchedulerError> {
    let store = store::SchedulerStore::new(tasks_db_path.to_path_buf())?;
    store.insert_task(task)
}

/// MVP routine heuristic for the dashboard.
///
/// There is not yet a first-class "routine" marker in the scheduler data model, so we classify
/// user-visible routines conservatively:
/// - only `run_task` tasks can surface as routines
/// - all cron tasks count as routines
/// - one-shot tasks count as routines only when they are clearly scheduled for later, either
///   because they are still in the future or because `run_at` was materially later than
///   `created_at` when the task was created
///
/// This intentionally hides ambiguous internal tasks like immediate inbound work and follow-up
/// reply tasks until the product model grows an explicit routine object.
pub fn is_user_visible_routine_task(task: &ScheduledTask, now: DateTime<Utc>) -> bool {
    if !matches!(task.kind, TaskKind::RunTask(_)) {
        return false;
    }

    match &task.schedule {
        Schedule::Cron { .. } => true,
        Schedule::OneShot { run_at } => {
            if *run_at > now {
                return true;
            }
            *run_at
                >= task.created_at
                    + ChronoDuration::minutes(ROUTINE_ONE_SHOT_DELAY_THRESHOLD_MINUTES)
        }
    }
}

pub fn refreshed_schedule_for_resume(
    schedule: &Schedule,
    now: DateTime<Utc>,
) -> Result<Schedule, SchedulerError> {
    match schedule {
        Schedule::Cron { expression, .. } => {
            let next_run = next_run_after(expression, now)?;
            Ok(Schedule::Cron {
                expression: expression.clone(),
                next_run,
            })
        }
        Schedule::OneShot { run_at } => {
            if *run_at < now {
                return Err(SchedulerError::TaskFailed(
                    "one_shot run_at is in the past".to_string(),
                ));
            }
            Ok(Schedule::OneShot { run_at: *run_at })
        }
    }
}

/// Prepare a stored task for resume without persisting it.
///
/// This keeps the action logic testable without requiring a live storage backend.
pub fn prepare_task_for_resume(
    task: &ScheduledTask,
    now: DateTime<Utc>,
) -> Result<ScheduledTask, SchedulerError> {
    let mut resumed = task.clone();
    resumed.schedule = refreshed_schedule_for_resume(&task.schedule, now)?;
    resumed.enabled = true;
    Ok(resumed)
}

pub(crate) fn load_run_task_request_context(task: &RunTaskTask) -> Option<String> {
    let input_email_dir = if task.input_email_dir.is_absolute() {
        task.input_email_dir.clone()
    } else {
        task.workspace_dir.join(&task.input_email_dir)
    };

    let thread_request_path = input_email_dir.join("thread_request.md");
    if let Ok(content) = fs::read_to_string(thread_request_path) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Repair legacy weekday cron expressions for stored run_task schedules.
///
/// This is a compatibility shim for older model-generated schedules that encoded a Monday-Friday
/// request with numeric weekday fields like `1-5`, which the scheduler's cron parser can
/// interpret as Sunday-Thursday. We only rewrite the cron when the original request context
/// clearly asked for weekdays, so legitimate Sunday-Thursday routines remain untouched.
pub(crate) fn maybe_repair_legacy_weekday_cron_task(
    task: &mut ScheduledTask,
    now: DateTime<Utc>,
) -> Result<bool, SchedulerError> {
    let request_context = match &task.kind {
        TaskKind::RunTask(run_task) => load_run_task_request_context(run_task),
        _ => return Ok(false),
    };

    let Schedule::Cron {
        expression,
        next_run,
    } = &mut task.schedule
    else {
        return Ok(false);
    };

    let normalized = normalize_weekday_cron_expression(expression, request_context.as_deref());
    if normalized == *expression {
        return Ok(false);
    }

    let reference_time = task
        .last_run
        .map(|value| if value > now { value } else { now })
        .unwrap_or(now);
    *expression = normalized;
    *next_run = next_run_after(expression, reference_time)?;
    Ok(true)
}

#[cfg(test)]
mod tests;
