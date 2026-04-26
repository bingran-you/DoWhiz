use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::FindOptions;
use mongodb::sync::Collection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::task;

use crate::mongo_store::{database_from_env, get_shared_client, retry_mongo_read};
use crate::scheduler::task_view::{
    default_routine_name, derive_request_summary, derive_task_sender_summary,
    deserialize_task_document,
};
use crate::scheduler::{Schedule, ScheduledTask, TaskKind};

use super::analytics::{require_admin_email, resolve_window, AnalyticsState, DateRangeSummary};

const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 100;
const LONG_RUNNING_WARNING_SECS: i64 = 3600;
const TASK_BATCH_SIZE: usize = 500;
const TRACE_MATCH_TOLERANCE_MS: i64 = 10 * 60 * 1000;
const MAX_EXECUTION_SCAN_DOCS: usize = 5000;

#[derive(Debug, Deserialize)]
pub struct TaskOpsQuery {
    pub start: Option<String>,
    pub end: Option<String>,
    pub range: Option<String>,
    pub status: Option<String>,
    pub channel: Option<String>,
    pub q: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct TaskOpsSummary {
    pub total_runs: usize,
    pub running_now: usize,
    pub long_running: usize,
    pub successful_runs: usize,
    pub failed_runs: usize,
    pub success_rate: Option<f64>,
    pub median_duration_seconds: Option<i64>,
    pub p95_duration_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskOpsRow {
    pub task_id: String,
    pub execution_id: i64,
    pub title: String,
    pub request_summary: Option<String>,
    pub kind: String,
    pub channel: String,
    pub sender: Option<String>,
    pub sender_name: Option<String>,
    pub status: String,
    pub current_stage: Option<String>,
    pub is_running_long: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_seconds: Option<i64>,
    pub created_at: String,
    pub last_run: Option<String>,
    pub schedule_type: String,
    pub next_run: Option<String>,
    pub run_at: Option<String>,
    pub enabled: bool,
    pub retry_count: u32,
    pub error_message: Option<String>,
    pub owner_scope_kind: String,
    pub owner_scope_id: String,
    pub runner: Option<String>,
    pub model_name: Option<String>,
    pub backend: Option<String>,
    pub deploy_target: Option<String>,
    pub trace_started_at: Option<String>,
    pub trace_finished_at: Option<String>,
    pub trace_stage_updated_at: Option<String>,
    pub timing_ms: Option<run_task_module::RunTaskTraceTimingMs>,
    pub token_usage: Option<run_task_module::TokenUsage>,
}

#[derive(Debug, Serialize)]
pub struct TaskOpsResponse {
    pub generated_at: String,
    pub range: DateRangeSummary,
    pub summary: TaskOpsSummary,
    pub page: usize,
    pub page_size: usize,
    pub has_next_page: bool,
    pub rows: Vec<TaskOpsRow>,
}

#[derive(Debug, Clone)]
struct ExecutionDoc {
    task_id: String,
    execution_id: i64,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    status: String,
    error_message: Option<String>,
    owner_scope_kind: String,
    owner_scope_id: String,
}

#[derive(Debug)]
struct TaskRecord {
    doc: Document,
    task: ScheduledTask,
}

#[derive(Debug, Clone)]
struct TraceSnapshotView {
    backend: String,
    deploy_target: String,
    current_stage: Option<String>,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    stage_updated_at: Option<DateTime<Utc>>,
    timing_ms: Option<run_task_module::RunTaskTraceTimingMs>,
    token_usage: Option<run_task_module::TokenUsage>,
}

#[derive(Debug, Clone)]
struct TaskOpsPageSlice {
    rows: Vec<TaskOpsRow>,
    has_next_page: bool,
}

pub fn task_ops_router(state: AnalyticsState) -> Router {
    Router::new()
        .route("/analytics/task-ops", axum::routing::get(get_task_ops))
        .with_state(state)
}

pub async fn get_task_ops(
    State(state): State<AnalyticsState>,
    headers: HeaderMap,
    Query(query): Query<TaskOpsQuery>,
) -> impl IntoResponse {
    let admin_email = match require_admin_email(&state, &headers).await {
        Ok(email) => email,
        Err(response) => return response,
    };

    let (start, end) = match resolve_window(
        query.start.as_deref(),
        query.end.as_deref(),
        query.range.as_deref(),
    ) {
        Ok(window) => window,
        Err(msg) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": msg })),
            )
                .into_response();
        }
    };

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let status_filter = normalize_filter(query.status.as_deref());
    let channel_filter = normalize_filter(query.channel.as_deref());
    let search_query = normalize_filter(query.q.as_deref());

    let fetched = task::spawn_blocking(move || {
        load_task_ops_snapshot(
            start,
            end,
            page,
            page_size,
            status_filter.as_deref(),
            channel_filter.as_deref(),
            search_query.as_deref(),
        )
    })
    .await;

    let response = match fetched {
        Ok(Ok(response)) => response,
        Ok(Err(err)) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err })),
            )
                .into_response();
        }
        Err(err) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to load task ops: {err}") })),
            )
                .into_response();
        }
    };

    tracing::info!(
        "analytics.task_ops generated for admin={} range={}..{} page={} page_size={} has_next_page={}",
        admin_email,
        start.to_rfc3339(),
        end.to_rfc3339(),
        response.page,
        response.page_size,
        response.has_next_page
    );
    (axum::http::StatusCode::OK, Json(response)).into_response()
}

fn load_task_ops_snapshot(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    page: usize,
    page_size: usize,
    status_filter: Option<&str>,
    channel_filter: Option<&str>,
    search_query: Option<&str>,
) -> Result<TaskOpsResponse, String> {
    let client = get_shared_client();
    let db = database_from_env(client);
    let executions_coll = db.collection::<Document>("task_executions");
    let tasks_coll = db.collection::<Document>("tasks");

    // ONE global fetch - get all execution docs for the date range
    let all_executions = load_all_execution_docs(&executions_coll, start, end)?;

    // Compute summary in memory (before any filtering)
    let summary = compute_summary_from_execution_docs(&all_executions, status_filter);

    // Dedupe, filter, and paginate - all in memory
    let page_slice = paginate_executions_in_memory(
        &all_executions,
        &tasks_coll,
        page,
        page_size,
        status_filter,
        channel_filter,
        search_query,
    )?;

    Ok(TaskOpsResponse {
        generated_at: Utc::now().to_rfc3339(),
        range: DateRangeSummary {
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            days: (end - start).num_days(),
        },
        summary,
        page,
        page_size,
        has_next_page: page_slice.has_next_page,
        rows: page_slice.rows,
    })
}

/// Fetch all execution docs in the date range with a single query.
fn load_all_execution_docs(
    collection: &Collection<Document>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<ExecutionDoc>, String> {
    let filter = doc! {
        "started_at": {
            "$gte": BsonDateTime::from_chrono(start),
            "$lte": BsonDateTime::from_chrono(end),
        }
    };
    let options = FindOptions::builder()
        .sort(doc! { "started_at": -1 })
        .limit(MAX_EXECUTION_SCAN_DOCS as i64)
        .build();

    let cursor = retry_mongo_read("task_ops.all_executions", || {
        collection.find(filter.clone(), options.clone())
    })
    .map_err(|err| format!("failed to query task executions: {err}"))?;

    let mut docs = Vec::new();
    for row in cursor {
        let document = row.map_err(|err| format!("failed to read execution doc: {err}"))?;
        docs.push(parse_execution_doc(&document)?);
    }
    Ok(docs)
}

/// Compute summary stats from execution docs.
fn compute_summary_from_execution_docs(
    docs: &[ExecutionDoc],
    status_filter: Option<&str>,
) -> TaskOpsSummary {
    let now = Utc::now();
    let long_running_threshold = now - chrono::Duration::seconds(LONG_RUNNING_WARNING_SECS);

    let mut total_runs = 0usize;
    let mut running_now = 0usize;
    let mut long_running = 0usize;
    let mut successful_runs = 0usize;
    let mut failed_runs = 0usize;
    let mut durations: Vec<i64> = Vec::new();

    for doc in docs {
        // Apply status filter for total_runs count
        let matches_filter = status_filter
            .map(|f| doc.status.to_ascii_lowercase() == f)
            .unwrap_or(true);
        if matches_filter {
            total_runs += 1;
        }

        // Running stats (not affected by status_filter)
        if doc.status == "running" {
            running_now += 1;
            if doc.started_at < long_running_threshold {
                long_running += 1;
            }
        }

        // Success/failure counts
        match doc.status.as_str() {
            "success" | "superseded" => successful_runs += 1,
            "failed" | "cancelled" | "expired" => failed_runs += 1,
            _ => {}
        }

        // Collect durations for median/p95
        if let Some(finished_at) = doc.finished_at {
            let duration = finished_at.signed_duration_since(doc.started_at).num_seconds();
            if duration >= 0 {
                durations.push(duration);
            }
        }
    }

    let success_rate = if successful_runs + failed_runs == 0 {
        None
    } else {
        Some(successful_runs as f64 / (successful_runs + failed_runs) as f64)
    };

    durations.sort_unstable();
    let median_duration_seconds = if durations.is_empty() {
        None
    } else {
        Some(durations[durations.len() / 2])
    };
    let p95_duration_seconds = if durations.is_empty() {
        None
    } else {
        let p95_idx = ((durations.len() as f64) * 0.95).ceil() as usize;
        Some(durations[p95_idx.saturating_sub(1).min(durations.len() - 1)])
    };

    TaskOpsSummary {
        total_runs,
        running_now,
        long_running,
        successful_runs,
        failed_runs,
        success_rate,
        median_duration_seconds,
        p95_duration_seconds,
    }
}

/// Dedupe, filter, and paginate execution docs in memory.
fn paginate_executions_in_memory(
    all_executions: &[ExecutionDoc],
    tasks_coll: &Collection<Document>,
    page: usize,
    page_size: usize,
    status_filter: Option<&str>,
    channel_filter: Option<&str>,
    search_query: Option<&str>,
) -> Result<TaskOpsPageSlice, String> {
    // Step 1: Dedupe - keep latest execution per task_id
    let mut latest_by_task_id: HashMap<String, ExecutionDoc> = HashMap::new();
    for doc in all_executions {
        // Apply status filter early
        if let Some(filter) = status_filter {
            if doc.status.to_ascii_lowercase() != filter {
                continue;
            }
        }

        match latest_by_task_id.get(&doc.task_id) {
            Some(existing) => {
                if doc.execution_id == existing.execution_id
                    && doc.started_at == existing.started_at
                    && prefer_execution_row(doc, existing)
                {
                    latest_by_task_id.insert(doc.task_id.clone(), doc.clone());
                }
            }
            None => {
                latest_by_task_id.insert(doc.task_id.clone(), doc.clone());
            }
        }
    }

    // Sort by started_at descending
    let mut deduped: Vec<ExecutionDoc> = latest_by_task_id.into_values().collect();
    deduped.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    // Step 2: Load task records for all deduped task_ids
    let task_ids: Vec<String> = deduped.iter().map(|d| d.task_id.clone()).collect();
    let task_records = load_task_records_for_ids(tasks_coll, &task_ids)?;

    // Step 3: Build and filter rows
    let filtered_rows = build_filtered_rows(&deduped, &task_records, channel_filter, search_query);

    // Step 4: Paginate
    let start_index = page.saturating_sub(1).saturating_mul(page_size);
    let has_next_page = filtered_rows.len() > start_index + page_size;
    let rows = filtered_rows
        .into_iter()
        .skip(start_index)
        .take(page_size)
        .collect();

    Ok(TaskOpsPageSlice { rows, has_next_page })
}

fn parse_execution_doc(document: &Document) -> Result<ExecutionDoc, String> {
    let owner_scope = document
        .get_document("owner_scope")
        .map_err(|err| format!("missing owner_scope in task execution row: {err}"))?;
    let started_at = document
        .get_datetime("started_at")
        .map(|value| value.to_chrono())
        .map_err(|err| format!("missing started_at in task execution row: {err}"))?;
    let finished_at = match document.get("finished_at") {
        Some(Bson::DateTime(value)) => Some(value.to_chrono()),
        _ => None,
    };
    Ok(ExecutionDoc {
        task_id: document
            .get_str("task_id")
            .map_err(|err| format!("missing task_id in task execution row: {err}"))?
            .to_string(),
        execution_id: bson_i64(document.get("execution_id"), "execution_id")?,
        started_at,
        finished_at,
        status: document.get_str("status").unwrap_or("unknown").to_string(),
        error_message: document
            .get_str("error_message")
            .ok()
            .map(|value| value.to_string()),
        owner_scope_kind: owner_scope.get_str("kind").unwrap_or("unknown").to_string(),
        owner_scope_id: owner_scope.get_str("id").unwrap_or("unknown").to_string(),
    })
}

fn prefer_execution_row(candidate: &ExecutionDoc, existing: &ExecutionDoc) -> bool {
    let candidate_rank = owner_scope_rank(&candidate.owner_scope_kind);
    let existing_rank = owner_scope_rank(&existing.owner_scope_kind);
    if candidate_rank != existing_rank {
        return candidate_rank > existing_rank;
    }
    let candidate_error = candidate
        .error_message
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty();
    let existing_error = existing
        .error_message
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty();
    existing_error && !candidate_error
}

#[cfg(test)]
fn dedupe_latest_task_executions(mut rows: Vec<ExecutionDoc>) -> Vec<ExecutionDoc> {
    rows.sort_by(|left, right| {
        right
            .started_at
            .cmp(&left.started_at)
            .then_with(|| right.execution_id.cmp(&left.execution_id))
    });

    let mut deduped: Vec<ExecutionDoc> = Vec::new();
    let mut by_task_id: HashMap<String, usize> = HashMap::new();
    for row in rows {
        if let Some(existing_index) = by_task_id.get(&row.task_id).copied() {
            let existing = &deduped[existing_index];
            if row.execution_id == existing.execution_id
                && row.started_at == existing.started_at
                && prefer_execution_row(&row, existing)
            {
                deduped[existing_index] = row;
            }
            continue;
        }
        by_task_id.insert(row.task_id.clone(), deduped.len());
        deduped.push(row);
    }
    deduped
}

fn owner_scope_rank(owner_scope_kind: &str) -> i32 {
    match owner_scope_kind {
        "account" => 3,
        "user" => 2,
        _ => 1,
    }
}

fn load_task_records_for_ids(
    collection: &Collection<Document>,
    task_ids: &[String],
) -> Result<HashMap<String, TaskRecord>, String> {
    let mut by_task_id: HashMap<String, TaskRecord> = HashMap::new();
    for chunk in task_ids.chunks(TASK_BATCH_SIZE) {
        let filter = doc! {
            "task_id": { "$in": chunk.iter().cloned().collect::<Vec<_>>() }
        };
        let cursor = retry_mongo_read("task_ops.task_docs", || {
            collection.find(filter.clone(), None)
        })
        .map_err(|err| format!("failed to query tasks: {err}"))?;

        for row in cursor {
            let document = row.map_err(|err| format!("failed to read task row: {err}"))?;
            let task_id = document
                .get_str("task_id")
                .map_err(|err| format!("missing task_id in task row: {err}"))?
                .to_string();
            let task = deserialize_task_document(&document)
                .map_err(|err| format!("failed to parse task document {task_id}: {err}"))?;
            let candidate = TaskRecord {
                doc: document,
                task,
            };
            match by_task_id.get(&task_id) {
                Some(existing) if !prefer_task_record(&candidate, existing) => {}
                _ => {
                    by_task_id.insert(task_id, candidate);
                }
            }
        }
    }

    Ok(by_task_id)
}

fn prefer_task_record(candidate: &TaskRecord, existing: &TaskRecord) -> bool {
    let candidate_owner_kind = candidate
        .doc
        .get_document("owner_scope")
        .ok()
        .and_then(|scope| scope.get_str("kind").ok())
        .unwrap_or("unknown");
    let existing_owner_kind = existing
        .doc
        .get_document("owner_scope")
        .ok()
        .and_then(|scope| scope.get_str("kind").ok())
        .unwrap_or("unknown");
    let candidate_rank = owner_scope_rank(candidate_owner_kind);
    let existing_rank = owner_scope_rank(existing_owner_kind);
    if candidate_rank != existing_rank {
        return candidate_rank > existing_rank;
    }
    candidate
        .task
        .created_at
        .cmp(&existing.task.created_at)
        .then_with(|| {
            let candidate_enabled = candidate
                .doc
                .get_bool("enabled")
                .unwrap_or(candidate.task.enabled);
            let existing_enabled = existing
                .doc
                .get_bool("enabled")
                .unwrap_or(existing.task.enabled);
            bool_rank(candidate_enabled).cmp(&bool_rank(existing_enabled))
        })
        == Ordering::Greater
}

fn bool_rank(value: bool) -> i32 {
    if value {
        1
    } else {
        0
    }
}

fn build_task_ops_row(execution: &ExecutionDoc, task_record: &TaskRecord) -> TaskOpsRow {
    let request_summary = derive_request_summary(&task_record.doc);
    let sender_summary = derive_task_sender_summary(&task_record.doc);
    let (schedule_type, next_run, run_at) = schedule_fields(&task_record.task);
    let (runner, model_name, workspace_dir) = run_task_fields(&task_record.task);
    let trace = workspace_dir
        .as_ref()
        .and_then(|path| load_trace_snapshot_for_execution(path.as_path(), execution));
    let duration_seconds = execution.finished_at.map(|finished_at| {
        finished_at
            .signed_duration_since(execution.started_at)
            .num_seconds()
    });
    let title = request_summary.clone().unwrap_or_else(|| {
        default_title(
            &task_record.task,
            task_record.doc.get_str("channel").unwrap_or(""),
        )
    });

    TaskOpsRow {
        task_id: execution.task_id.clone(),
        execution_id: execution.execution_id,
        title,
        request_summary,
        kind: task_record
            .doc
            .get_str("kind")
            .unwrap_or("unknown")
            .to_string(),
        channel: task_record
            .doc
            .get_str("channel")
            .unwrap_or("unknown")
            .to_string(),
        sender: sender_summary.sender,
        sender_name: sender_summary.sender_name,
        status: execution.status.clone(),
        current_stage: trace
            .as_ref()
            .and_then(|snapshot| snapshot.current_stage.clone()),
        is_running_long: execution.status == "running"
            && Utc::now()
                .signed_duration_since(execution.started_at)
                .num_seconds()
                >= LONG_RUNNING_WARNING_SECS,
        started_at: execution.started_at.to_rfc3339(),
        finished_at: execution.finished_at.map(|value| value.to_rfc3339()),
        duration_seconds,
        created_at: task_record.task.created_at.to_rfc3339(),
        last_run: task_record.task.last_run.map(|value| value.to_rfc3339()),
        schedule_type,
        next_run,
        run_at,
        enabled: task_record
            .doc
            .get_bool("enabled")
            .unwrap_or(task_record.task.enabled),
        retry_count: numeric_field_to_u32(&task_record.doc, "retry_count").unwrap_or(0),
        error_message: execution.error_message.clone(),
        owner_scope_kind: execution.owner_scope_kind.clone(),
        owner_scope_id: execution.owner_scope_id.clone(),
        runner,
        model_name,
        backend: trace.as_ref().map(|snapshot| snapshot.backend.clone()),
        deploy_target: trace
            .as_ref()
            .map(|snapshot| snapshot.deploy_target.clone()),
        trace_started_at: trace
            .as_ref()
            .map(|snapshot| snapshot.started_at.to_rfc3339()),
        trace_finished_at: trace.as_ref().and_then(|snapshot| {
            snapshot
                .finished_at
                .as_ref()
                .map(|value| value.to_rfc3339())
        }),
        trace_stage_updated_at: trace.as_ref().and_then(|snapshot| {
            snapshot
                .stage_updated_at
                .as_ref()
                .map(|value| value.to_rfc3339())
        }),
        timing_ms: trace
            .as_ref()
            .and_then(|snapshot| snapshot.timing_ms.clone()),
        token_usage: trace
            .as_ref()
            .and_then(|snapshot| snapshot.token_usage.clone()),
    }
}

fn row_matches_filters(
    row: &TaskOpsRow,
    status_filter: Option<&str>,
    channel_filter: Option<&str>,
    search_query: Option<&str>,
) -> bool {
    if let Some(status_filter) = status_filter {
        if row.status.to_ascii_lowercase() != status_filter {
            return false;
        }
    }
    if let Some(channel_filter) = channel_filter {
        if row.channel.to_ascii_lowercase() != channel_filter {
            return false;
        }
    }
    if let Some(search_query) = search_query {
        let haystacks = [
            row.task_id.as_str(),
            row.title.as_str(),
            row.request_summary.as_deref().unwrap_or(""),
            row.sender_name.as_deref().unwrap_or(""),
            row.sender.as_deref().unwrap_or(""),
            row.error_message.as_deref().unwrap_or(""),
        ];
        let matches = haystacks
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(search_query));
        if !matches {
            return false;
        }
    }
    true
}

fn build_filtered_rows(
    executions: &[ExecutionDoc],
    task_records: &HashMap<String, TaskRecord>,
    channel_filter: Option<&str>,
    search_query: Option<&str>,
) -> Vec<TaskOpsRow> {
    let mut rows = Vec::new();
    for execution in executions {
        let Some(task_record) = task_records.get(&execution.task_id) else {
            continue;
        };
        if !matches!(task_record.task.kind, TaskKind::RunTask(_)) {
            continue;
        }

        let row = build_task_ops_row(execution, task_record);
        if !row_matches_filters(&row, None, channel_filter, search_query) {
            continue;
        }
        rows.push(row);
    }
    rows
}
fn default_title(task: &ScheduledTask, channel: &str) -> String {
    match &task.kind {
        TaskKind::RunTask(_) => default_routine_name(channel),
        TaskKind::SendReply(_) => "Send Reply".to_string(),
        TaskKind::Noop => "No-op".to_string(),
    }
}

fn schedule_fields(task: &ScheduledTask) -> (String, Option<String>, Option<String>) {
    match &task.schedule {
        Schedule::Cron { next_run, .. } => ("cron".to_string(), Some(next_run.to_rfc3339()), None),
        Schedule::OneShot { run_at } => ("one_shot".to_string(), None, Some(run_at.to_rfc3339())),
    }
}

fn run_task_fields(task: &ScheduledTask) -> (Option<String>, Option<String>, Option<PathBuf>) {
    match &task.kind {
        TaskKind::RunTask(run_task) => (
            Some(run_task.runner.clone()),
            Some(run_task.model_name.clone()),
            Some(run_task.workspace_dir.clone()),
        ),
        _ => (None, None, None),
    }
}

fn load_trace_snapshot_for_execution(
    workspace_dir: &std::path::Path,
    execution: &ExecutionDoc,
) -> Option<TraceSnapshotView> {
    let snapshot = run_task_module::load_trace_snapshot(workspace_dir)?;
    let trace_started_at = DateTime::from_timestamp_millis(snapshot.started_at_unix_ms as i64)?;
    let execution_started_at_ms = execution.started_at.timestamp_millis();
    let delta = (trace_started_at.timestamp_millis() - execution_started_at_ms).abs();
    if delta > TRACE_MATCH_TOLERANCE_MS {
        return None;
    }
    Some(TraceSnapshotView {
        backend: snapshot.backend,
        deploy_target: snapshot.deploy_target,
        current_stage: snapshot.current_stage,
        started_at: trace_started_at,
        finished_at: snapshot
            .finished_at_unix_ms
            .and_then(|value| DateTime::from_timestamp_millis(value as i64)),
        stage_updated_at: snapshot
            .stage_updated_at_unix_ms
            .and_then(|value| DateTime::from_timestamp_millis(value as i64)),
        timing_ms: snapshot.timing_ms,
        token_usage: snapshot.token_usage,
    })
}

fn numeric_field_to_u32(document: &Document, key: &str) -> Option<u32> {
    match document.get(key) {
        Some(Bson::Int32(value)) if *value >= 0 => Some(*value as u32),
        Some(Bson::Int64(value)) if *value >= 0 => Some(*value as u32),
        _ => None,
    }
}

fn bson_i64(value: Option<&Bson>, field: &str) -> Result<i64, String> {
    match value {
        Some(Bson::Int64(value)) => Ok(*value),
        Some(Bson::Int32(value)) => Ok(i64::from(*value)),
        Some(other) => Err(format!("invalid {field} type for execution row: {other:?}")),
        None => Err(format!("missing {field} for execution row")),
    }
}

fn normalize_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;

    use super::{build_filtered_rows, dedupe_latest_task_executions, ExecutionDoc, TaskOpsRow};

    fn sample_execution(
        task_id: &str,
        execution_id: i64,
        owner_scope_kind: &str,
        started_at: chrono::DateTime<Utc>,
        status: &str,
    ) -> ExecutionDoc {
        ExecutionDoc {
            task_id: task_id.to_string(),
            execution_id,
            started_at,
            finished_at: Some(started_at + chrono::Duration::minutes(5)),
            status: status.to_string(),
            error_message: None,
            owner_scope_kind: owner_scope_kind.to_string(),
            owner_scope_id: format!("{owner_scope_kind}-1"),
        }
    }

    #[test]
    fn dedupe_latest_execution_prefers_account_scope() {
        let started_at = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        let rows = vec![
            sample_execution("task-1", 100, "user", started_at, "success"),
            sample_execution("task-1", 100, "account", started_at, "success"),
        ];
        let deduped = dedupe_latest_task_executions(rows);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].owner_scope_kind, "account");
    }

    #[test]
    fn build_filtered_rows_is_empty_without_task_records() {
        let rows = vec![
            TaskOpsRow {
                task_id: "a".to_string(),
                execution_id: 1,
                title: "A".to_string(),
                request_summary: Some("A".to_string()),
                kind: "run_task".to_string(),
                channel: "email".to_string(),
                sender: None,
                sender_name: None,
                status: "success".to_string(),
                current_stage: None,
                is_running_long: false,
                started_at: "2026-04-24T12:00:00Z".to_string(),
                finished_at: Some("2026-04-24T12:05:00Z".to_string()),
                duration_seconds: Some(300),
                created_at: "2026-04-24T11:59:00Z".to_string(),
                last_run: None,
                schedule_type: "one_shot".to_string(),
                next_run: None,
                run_at: None,
                enabled: false,
                retry_count: 0,
                error_message: None,
                owner_scope_kind: "account".to_string(),
                owner_scope_id: "account-1".to_string(),
                runner: Some("codex".to_string()),
                model_name: Some("gpt-test".to_string()),
                backend: None,
                deploy_target: None,
                trace_started_at: None,
                trace_finished_at: None,
                trace_stage_updated_at: None,
                timing_ms: None,
                token_usage: None,
            },
            TaskOpsRow {
                task_id: "b".to_string(),
                execution_id: 2,
                title: "B".to_string(),
                request_summary: Some("B".to_string()),
                kind: "run_task".to_string(),
                channel: "email".to_string(),
                sender: None,
                sender_name: None,
                status: "failed".to_string(),
                current_stage: None,
                is_running_long: false,
                started_at: "2026-04-24T13:00:00Z".to_string(),
                finished_at: Some("2026-04-24T13:10:00Z".to_string()),
                duration_seconds: Some(600),
                created_at: "2026-04-24T12:59:00Z".to_string(),
                last_run: None,
                schedule_type: "one_shot".to_string(),
                next_run: None,
                run_at: None,
                enabled: false,
                retry_count: 0,
                error_message: Some("boom".to_string()),
                owner_scope_kind: "account".to_string(),
                owner_scope_id: "account-1".to_string(),
                runner: Some("codex".to_string()),
                model_name: Some("gpt-test".to_string()),
                backend: None,
                deploy_target: None,
                trace_started_at: None,
                trace_finished_at: None,
                trace_stage_updated_at: None,
                timing_ms: None,
                token_usage: None,
            },
        ];

        let filtered = build_filtered_rows(&[], &HashMap::new(), None, None);
        assert!(filtered.is_empty());
        let durations: Vec<i64> = rows.iter().filter_map(|row| row.duration_seconds).collect();
        assert_eq!(durations, vec![300, 600]);
    }

    #[test]
    fn dedupe_latest_task_executions_prefers_account_scope_for_same_latest_execution() {
        let started_at = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        let rows = vec![
            sample_execution("task-1", 100, "user", started_at, "success"),
            sample_execution("task-1", 100, "account", started_at, "success"),
            sample_execution(
                "task-1",
                99,
                "account",
                started_at - chrono::Duration::minutes(10),
                "failed",
            ),
        ];

        let deduped = dedupe_latest_task_executions(rows);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].owner_scope_kind, "account");
        assert_eq!(deduped[0].execution_id, 100);
    }
}
