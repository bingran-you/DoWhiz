use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use azure_core::StatusCode;
use azure_storage::StorageCredentials;
use azure_storage_blobs::prelude::*;
use chrono::{DateTime, Utc};
use reqwest::{blocking::Client, Url};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use uuid::Uuid;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::env_alias::{bool_with_scale_oliver, var_with_scale_oliver};

use super::store::TaskDebugArchiveRecord;
use super::types::{ScheduledTask, SchedulerError, TaskKind};

const ARCHIVE_TYPE: &str = "full_debug_bundle";
const ARCHIVE_VERSION: i32 = 1;
const DEFAULT_ARCHIVE_CONTAINER: &str = "task-debug-archives";
const DEFAULT_ARCHIVE_PATH_PREFIX: &str = "task_debug_archives";
const LOCAL_FALLBACK_DIRNAME: &str = ".task_debug_archives_failed";
const MAX_GIT_CAPTURE_BYTES: usize = 1_000_000;
const MAX_TOOL_OUTPUT_BYTES: usize = 32_000;
const HEAVY_SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".cache",
    ".npm",
    ".pnpm-store",
    ".yarn",
    ".next",
    LOCAL_FALLBACK_DIRNAME,
];
const REDACTED_FILE_NAMES: &[&str] = &[
    ".google_access_token",
    "google_workspace_cli_credentials.json",
    "credentials.json",
];
const REDACTED_FILE_EXTENSIONS: &[&str] = &["pem", "key", "p12", "pfx"];
const ENV_ALLOWLIST: &[&str] = &[
    "DEPLOY_TARGET",
    "EMPLOYEE_ID",
    "EMPLOYEE_CONFIG_PATH",
    "GATEWAY_CONFIG_PATH",
    "RUN_TASK_EXECUTION_BACKEND",
    "RUN_TASK_TIMEOUT_SECS",
    "TASK_TIMEOUT_SECS",
    "RUN_TASK_AZURE_ACI_RESOURCE_GROUP",
    "RUN_TASK_AZURE_ACI_IMAGE",
    "RUN_TASK_AZURE_ACI_CPU",
    "RUN_TASK_AZURE_ACI_MEMORY_GB",
    "RUN_TASK_AZURE_ACI_FILE_SHARE",
    "AZURE_STORAGE_ACCOUNT",
    "AZURE_STORAGE_CONTAINER_INGEST",
    "AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES",
    "MONGODB_DATABASE",
];
const SENSITIVE_ENV_MARKERS: &[&str] = &[
    "KEY",
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "CONNECTION_STRING",
    "SAS",
    "COOKIE",
    "SESSION",
    "AUTH",
];

pub(crate) struct PendingTaskDebugArchive {
    task_id: Uuid,
    execution_id: i64,
    workspace_dir: PathBuf,
    archive_root: Option<PathBuf>,
    runner: String,
    model: String,
    deploy_target: String,
    started_at: DateTime<Utc>,
    before_capture_duration_ms: i64,
    temp_dir: TempDir,
    before_snapshot: SnapshotCollection,
    storage_plan: ArchiveStoragePlan,
}

#[derive(Debug, Clone)]
enum ArchiveStoragePlan {
    Azure(Vec<AzureArchiveTarget>),
    LocalOnly { reason: String },
}

#[derive(Debug, Clone)]
struct AzureArchiveTarget {
    container: String,
    auth: AzureArchiveAuth,
    path_prefix: String,
    storage_backend: String,
    storage_account: Option<String>,
}

#[derive(Debug, Clone)]
enum AzureArchiveAuth {
    ContainerSasUrl(String),
    AccountSas {
        account: String,
        sas_token: String,
    },
    ConnectionString {
        account: String,
        account_key: String,
    },
}

#[derive(Debug, Clone, Serialize, Default)]
struct SnapshotCollection {
    summary: SnapshotSummary,
    #[serde(skip_serializing)]
    included_files: Vec<SnapshotFileSource>,
    redacted_count: usize,
    skipped_count: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
struct SnapshotSummary {
    label: String,
    root: String,
    file_count: usize,
    included_file_count: usize,
    total_bytes: u64,
    entries: Vec<SnapshotEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotEntry {
    path: String,
    entry_type: String,
    status: String,
    size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct SnapshotFileSource {
    relative_path: String,
    source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Default)]
struct SnapshotDiff {
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<String>,
    unchanged: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
struct RuntimeSnapshot {
    env_allowlist: BTreeMap<String, String>,
    env_redacted: BTreeMap<String, RedactedEnvValue>,
    tool_versions: BTreeMap<String, String>,
    git: GitSnapshot,
}

#[derive(Debug, Clone, Serialize, Default)]
struct RedactedEnvValue {
    value_length: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
struct GitSnapshot {
    repo_root: Option<String>,
    head: Option<String>,
    status: Option<String>,
    remotes: Option<String>,
    diff_head: Option<String>,
    diff_staged: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ArchiveManifest {
    archive_type: String,
    archive_version: i32,
    task_id: String,
    execution_id: i64,
    status: String,
    runner: String,
    model: String,
    deploy_target: String,
    started_at: String,
    finished_at: String,
    duration_ms: i64,
    archive_build_duration_ms: i64,
    workspace_before_file_count: usize,
    workspace_after_file_count: usize,
    redacted_file_count: usize,
    skipped_file_count: usize,
    has_workspace_before: bool,
    has_workspace_after: bool,
    has_run_task_trace: bool,
    has_aci_logs: bool,
    error_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ExecutionSummary {
    task_id: String,
    execution_id: i64,
    started_at: String,
    finished_at: String,
    status: String,
    duration_ms: i64,
    error_summary: Option<String>,
}

impl PendingTaskDebugArchive {
    pub(crate) fn begin(
        task: &ScheduledTask,
        execution_id: i64,
        started_at: DateTime<Utc>,
    ) -> Result<Option<Self>, SchedulerError> {
        if !bool_with_scale_oliver("TASK_DEBUG_ARCHIVE_ENABLED", true) {
            return Ok(None);
        }
        let TaskKind::RunTask(run_task) = &task.kind else {
            return Ok(None);
        };

        let temp_dir = TempDir::new()?;
        let before_root = temp_dir.path().join("workspace_before");
        fs::create_dir_all(&before_root)?;

        let capture_started = Instant::now();
        let before_snapshot = collect_snapshot(
            &run_task.workspace_dir,
            "workspace_before",
            SnapshotMode::CopyTo(before_root),
        )?;
        let before_capture_duration_ms = capture_started.elapsed().as_millis() as i64;

        Ok(Some(Self {
            task_id: task.id,
            execution_id,
            workspace_dir: run_task.workspace_dir.clone(),
            archive_root: run_task.archive_root.clone(),
            runner: run_task.runner.clone(),
            model: run_task.model_name.clone(),
            deploy_target: env::var("DEPLOY_TARGET")
                .unwrap_or_else(|_| "local".to_string())
                .trim()
                .to_ascii_lowercase(),
            started_at,
            before_capture_duration_ms,
            temp_dir,
            before_snapshot,
            storage_plan: resolve_archive_storage_plan(),
        }))
    }

    pub(crate) fn finalize(
        self,
        task_before: &ScheduledTask,
        task_after: &ScheduledTask,
        finished_at: DateTime<Utc>,
        status: &str,
        error_summary: Option<&str>,
    ) -> Result<TaskDebugArchiveRecord, SchedulerError> {
        let build_started = Instant::now();
        let after_snapshot = collect_snapshot(
            &self.workspace_dir,
            "workspace_after",
            SnapshotMode::InPlace,
        )?;
        let runtime_snapshot = collect_runtime_snapshot(&self.workspace_dir);
        let workspace_diff = build_snapshot_diff(&self.before_snapshot, &after_snapshot);
        let has_run_task_trace = after_snapshot
            .summary
            .entries
            .iter()
            .any(|entry| entry.path.contains(run_task_module::RUN_TASK_TRACE_DIRNAME));
        let has_aci_logs = after_snapshot.summary.entries.iter().any(|entry| {
            entry
                .path
                .ends_with(".run_task_trace/aci/container_logs.txt")
                || entry
                    .path
                    .contains("/.run_task_trace/aci/container_logs.txt")
        });

        let manifest = ArchiveManifest {
            archive_type: ARCHIVE_TYPE.to_string(),
            archive_version: ARCHIVE_VERSION,
            task_id: self.task_id.to_string(),
            execution_id: self.execution_id,
            status: status.to_string(),
            runner: self.runner.clone(),
            model: self.model.clone(),
            deploy_target: self.deploy_target.clone(),
            started_at: self.started_at.to_rfc3339(),
            finished_at: finished_at.to_rfc3339(),
            duration_ms: finished_at
                .signed_duration_since(self.started_at)
                .num_milliseconds(),
            archive_build_duration_ms: 0,
            workspace_before_file_count: self.before_snapshot.summary.included_file_count,
            workspace_after_file_count: after_snapshot.summary.included_file_count,
            redacted_file_count: self.before_snapshot.redacted_count
                + after_snapshot.redacted_count,
            skipped_file_count: self.before_snapshot.skipped_count + after_snapshot.skipped_count,
            has_workspace_before: true,
            has_workspace_after: true,
            has_run_task_trace,
            has_aci_logs,
            error_summary: error_summary.map(|value| truncate_string(value, 4000)),
        };
        let execution_summary = ExecutionSummary {
            task_id: self.task_id.to_string(),
            execution_id: self.execution_id,
            started_at: self.started_at.to_rfc3339(),
            finished_at: finished_at.to_rfc3339(),
            status: status.to_string(),
            duration_ms: finished_at
                .signed_duration_since(self.started_at)
                .num_milliseconds(),
            error_summary: error_summary.map(|value| truncate_string(value, 4000)),
        };

        let zip_path = self.temp_dir.path().join("task_debug_bundle.zip");
        build_archive_zip(
            &zip_path,
            &manifest,
            &execution_summary,
            task_before,
            task_after,
            &self.before_snapshot,
            &after_snapshot,
            &workspace_diff,
            &runtime_snapshot,
        )?;
        let archive_build_duration_ms =
            self.before_capture_duration_ms + build_started.elapsed().as_millis() as i64;
        let size_bytes = fs::metadata(&zip_path)?.len() as i64;
        let sha256 = sha256_file(&zip_path)?;

        let upload_started = Instant::now();
        let fallback_root = self
            .archive_root
            .as_deref()
            .and_then(|path| path.parent())
            .unwrap_or(self.workspace_dir.as_path());
        let upload = upload_archive_bundle(
            &self.storage_plan,
            &zip_path,
            self.task_id,
            self.execution_id,
            self.started_at,
            fallback_root,
        )?;
        let upload_duration_ms = upload_started.elapsed().as_millis() as i64;

        Ok(TaskDebugArchiveRecord {
            task_id: self.task_id.to_string(),
            execution_id: self.execution_id,
            archive_type: ARCHIVE_TYPE.to_string(),
            archive_version: ARCHIVE_VERSION,
            status: upload.status,
            storage_backend: upload.storage_backend,
            storage_account: upload.storage_account,
            blob_container: upload.blob_container,
            blob_path: upload.blob_path,
            blob_reference: upload.blob_reference,
            local_fallback_path: upload.local_fallback_path,
            sha256,
            size_bytes,
            runner: self.runner,
            model: self.model,
            deploy_target: self.deploy_target,
            started_at: self.started_at,
            finished_at,
            duration_ms: finished_at
                .signed_duration_since(self.started_at)
                .num_milliseconds(),
            archive_build_duration_ms,
            upload_duration_ms,
            workspace_before_file_count: self.before_snapshot.summary.included_file_count as i64,
            workspace_after_file_count: after_snapshot.summary.included_file_count as i64,
            redacted_file_count: (self.before_snapshot.redacted_count
                + after_snapshot.redacted_count) as i64,
            skipped_file_count: (self.before_snapshot.skipped_count + after_snapshot.skipped_count)
                as i64,
            has_workspace_before: true,
            has_workspace_after: true,
            has_run_task_trace,
            has_aci_logs,
            error_summary: error_summary.map(|value| truncate_string(value, 4000)),
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone)]
enum SnapshotMode {
    CopyTo(PathBuf),
    InPlace,
}

#[derive(Debug, Clone)]
struct ArchiveUploadResult {
    status: String,
    storage_backend: String,
    storage_account: Option<String>,
    blob_container: Option<String>,
    blob_path: Option<String>,
    blob_reference: Option<String>,
    local_fallback_path: Option<String>,
}

fn collect_snapshot(
    workspace_dir: &Path,
    label: &str,
    mode: SnapshotMode,
) -> Result<SnapshotCollection, SchedulerError> {
    let mut snapshot = SnapshotCollection {
        summary: SnapshotSummary {
            label: label.to_string(),
            root: workspace_dir.to_string_lossy().into_owned(),
            ..SnapshotSummary::default()
        },
        ..SnapshotCollection::default()
    };
    if !workspace_dir.exists() {
        return Ok(snapshot);
    }
    collect_snapshot_dir(workspace_dir, Path::new(""), &mode, &mut snapshot)?;
    Ok(snapshot)
}

fn collect_snapshot_dir(
    workspace_root: &Path,
    relative_dir: &Path,
    mode: &SnapshotMode,
    snapshot: &mut SnapshotCollection,
) -> Result<(), SchedulerError> {
    let absolute_dir = if relative_dir.as_os_str().is_empty() {
        workspace_root.to_path_buf()
    } else {
        workspace_root.join(relative_dir)
    };
    let entries = match fs::read_dir(&absolute_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(SchedulerError::Io(err)),
    };

    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let relative_path = if relative_dir.as_os_str().is_empty() {
            PathBuf::from(&file_name)
        } else {
            relative_dir.join(&file_name)
        };
        let absolute_path = entry.path();
        let metadata = fs::symlink_metadata(&absolute_path)?;
        let relative_display = normalize_archive_path(&relative_path);

        if metadata.file_type().is_symlink() {
            snapshot.skipped_count += 1;
            snapshot.summary.entries.push(SnapshotEntry {
                path: relative_display,
                entry_type: "symlink".to_string(),
                status: "skipped".to_string(),
                size_bytes: 0,
                sha256: None,
                reason: Some("symlink_not_captured".to_string()),
            });
            continue;
        }

        if metadata.is_dir() {
            if should_skip_directory(&relative_path) {
                snapshot.skipped_count += 1;
                snapshot.summary.entries.push(SnapshotEntry {
                    path: relative_display,
                    entry_type: "dir".to_string(),
                    status: "skipped".to_string(),
                    size_bytes: 0,
                    sha256: None,
                    reason: Some("heavy_directory_skipped".to_string()),
                });
                continue;
            }
            collect_snapshot_dir(workspace_root, &relative_path, mode, snapshot)?;
            continue;
        }

        match classify_snapshot_file(&relative_path) {
            SnapshotDecision::Redact(reason) => {
                snapshot.redacted_count += 1;
                snapshot.summary.file_count += 1;
                snapshot.summary.entries.push(SnapshotEntry {
                    path: relative_display,
                    entry_type: "file".to_string(),
                    status: "redacted".to_string(),
                    size_bytes: metadata.len(),
                    sha256: None,
                    reason: Some(reason.to_string()),
                });
            }
            SnapshotDecision::Include => {
                let (sha256, size_bytes, copied_path) = match mode {
                    SnapshotMode::CopyTo(target_root) => {
                        let copied_path = target_root.join(&relative_path);
                        if let Some(parent) = copied_path.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        let (sha256, size_bytes) =
                            copy_file_and_hash(&absolute_path, &copied_path)?;
                        (sha256, size_bytes, copied_path)
                    }
                    SnapshotMode::InPlace => {
                        let (sha256, size_bytes) = hash_file(&absolute_path)?;
                        (sha256, size_bytes, absolute_path.clone())
                    }
                };
                snapshot.summary.file_count += 1;
                snapshot.summary.included_file_count += 1;
                snapshot.summary.total_bytes += size_bytes;
                snapshot.summary.entries.push(SnapshotEntry {
                    path: relative_display.clone(),
                    entry_type: "file".to_string(),
                    status: "included".to_string(),
                    size_bytes,
                    sha256: Some(sha256),
                    reason: None,
                });
                snapshot.included_files.push(SnapshotFileSource {
                    relative_path: relative_display,
                    source_path: copied_path,
                });
            }
        }
    }

    Ok(())
}

enum SnapshotDecision {
    Include,
    Redact(&'static str),
}

fn classify_snapshot_file(relative_path: &Path) -> SnapshotDecision {
    let file_name = relative_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if file_name.starts_with(".env") {
        return SnapshotDecision::Redact("env_file_redacted");
    }
    if REDACTED_FILE_NAMES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(file_name))
    {
        return SnapshotDecision::Redact("credential_file_redacted");
    }
    if relative_path
        .components()
        .any(|component| component.as_os_str() == ".secrets" || component.as_os_str() == ".auth")
    {
        return SnapshotDecision::Redact("secret_directory_redacted");
    }
    if let Some(extension) = relative_path.extension().and_then(|value| value.to_str()) {
        if REDACTED_FILE_EXTENSIONS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        {
            return SnapshotDecision::Redact("private_key_redacted");
        }
    }
    SnapshotDecision::Include
}

fn should_skip_directory(relative_path: &Path) -> bool {
    relative_path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        HEAVY_SKIP_DIRS
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
    })
}

fn build_snapshot_diff(before: &SnapshotCollection, after: &SnapshotCollection) -> SnapshotDiff {
    let before_map = snapshot_entry_map(before);
    let after_map = snapshot_entry_map(after);
    let mut diff = SnapshotDiff::default();

    for (path, after_hash) in &after_map {
        match before_map.get(path) {
            None => diff.added.push(path.clone()),
            Some(before_hash) if before_hash != after_hash => diff.changed.push(path.clone()),
            Some(_) => diff.unchanged += 1,
        }
    }
    for path in before_map.keys() {
        if !after_map.contains_key(path) {
            diff.removed.push(path.clone());
        }
    }

    diff.added.sort();
    diff.removed.sort();
    diff.changed.sort();
    diff
}

fn snapshot_entry_map(snapshot: &SnapshotCollection) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for entry in &snapshot.summary.entries {
        map.insert(
            entry.path.clone(),
            format!(
                "{}:{}:{}",
                entry.status,
                entry.size_bytes,
                entry.sha256.as_deref().unwrap_or("")
            ),
        );
    }
    map
}

fn build_archive_zip(
    zip_path: &Path,
    manifest: &ArchiveManifest,
    execution_summary: &ExecutionSummary,
    task_before: &ScheduledTask,
    task_after: &ScheduledTask,
    before_snapshot: &SnapshotCollection,
    after_snapshot: &SnapshotCollection,
    diff: &SnapshotDiff,
    runtime: &RuntimeSnapshot,
) -> Result<(), SchedulerError> {
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);

    write_json_entry(&mut zip, "manifest.json", manifest)?;
    write_json_entry(&mut zip, "task/task_before.json", task_before)?;
    write_json_entry(&mut zip, "task/task_after.json", task_after)?;
    write_json_entry(&mut zip, "task/execution_summary.json", execution_summary)?;
    write_json_entry(
        &mut zip,
        "manifests/workspace_before.json",
        &before_snapshot.summary,
    )?;
    write_json_entry(
        &mut zip,
        "manifests/workspace_after.json",
        &after_snapshot.summary,
    )?;
    write_json_entry(&mut zip, "manifests/workspace_diff.json", diff)?;
    write_json_entry(
        &mut zip,
        "runtime/env_allowlist.json",
        &runtime.env_allowlist,
    )?;
    write_json_entry(&mut zip, "runtime/env_redacted.json", &runtime.env_redacted)?;
    write_json_entry(
        &mut zip,
        "runtime/tool_versions.json",
        &runtime.tool_versions,
    )?;
    write_json_entry(&mut zip, "runtime/git.json", &runtime.git)?;

    add_snapshot_files(&mut zip, "workspace_before", before_snapshot)?;
    add_snapshot_files(&mut zip, "workspace_after", after_snapshot)?;

    zip.finish()
        .map_err(|err| SchedulerError::Storage(format!("zip finalize failed: {}", err)))?;
    Ok(())
}

fn add_snapshot_files(
    zip: &mut ZipWriter<File>,
    prefix: &str,
    snapshot: &SnapshotCollection,
) -> Result<(), SchedulerError> {
    for entry in &snapshot.included_files {
        let archive_path = format!("{}/{}", prefix, entry.relative_path);
        let data = fs::read(&entry.source_path)?;
        write_bytes_entry(zip, &archive_path, &data)?;
    }
    Ok(())
}

fn write_json_entry<T: Serialize>(
    zip: &mut ZipWriter<File>,
    path: &str,
    value: &T,
) -> Result<(), SchedulerError> {
    let payload = serde_json::to_vec_pretty(value)
        .map_err(|err| SchedulerError::Storage(format!("json encode failed: {}", err)))?;
    write_bytes_entry(zip, path, &payload)
}

fn write_bytes_entry(
    zip: &mut ZipWriter<File>,
    path: &str,
    bytes: &[u8],
) -> Result<(), SchedulerError> {
    zip.start_file(path, zip_file_options())
        .map_err(|err| SchedulerError::Storage(format!("zip start_file failed: {}", err)))?;
    zip.write_all(bytes)
        .map_err(|err| SchedulerError::Storage(format!("zip write failed: {}", err)))?;
    Ok(())
}

fn zip_file_options() -> FileOptions {
    FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644)
}

fn upload_archive_bundle(
    plan: &ArchiveStoragePlan,
    zip_path: &Path,
    task_id: Uuid,
    execution_id: i64,
    started_at: DateTime<Utc>,
    fallback_root: &Path,
) -> Result<ArchiveUploadResult, SchedulerError> {
    match plan {
        ArchiveStoragePlan::Azure(targets) => {
            let Some(primary_target) = targets.first() else {
                let fallback_path =
                    persist_local_fallback(zip_path, task_id, execution_id, Some(fallback_root))?;
                return Ok(ArchiveUploadResult {
                    status: "local_only".to_string(),
                    storage_backend: "local_fallback:missing_azure_blob_targets".to_string(),
                    storage_account: None,
                    blob_container: None,
                    blob_path: None,
                    blob_reference: None,
                    local_fallback_path: Some(fallback_path.to_string_lossy().into_owned()),
                });
            };
            let blob_path = build_archive_blob_path(
                &primary_target.path_prefix,
                task_id,
                execution_id,
                started_at,
            );
            let bytes = fs::read(zip_path)?;
            for target in targets {
                match upload_archive_bytes(target, &blob_path, &bytes) {
                    Ok(()) => {
                        return Ok(ArchiveUploadResult {
                            status: "uploaded".to_string(),
                            storage_backend: target.storage_backend.clone(),
                            storage_account: target.storage_account.clone(),
                            blob_container: Some(target.container.clone()),
                            blob_path: Some(blob_path.clone()),
                            blob_reference: Some(build_blob_reference(
                                target.storage_account.as_deref(),
                                &target.container,
                                &blob_path,
                            )),
                            local_fallback_path: None,
                        });
                    }
                    Err(err) => {
                        eprintln!(
                            "[task_debug_archive] upload attempt failed backend={} account={} container={} path={} error={}",
                            target.storage_backend,
                            target.storage_account.as_deref().unwrap_or("unknown"),
                            target.container,
                            blob_path,
                            err
                        );
                    }
                }
            }

            let fallback_path =
                persist_local_fallback(zip_path, task_id, execution_id, Some(fallback_root))?;
            Ok(ArchiveUploadResult {
                status: "upload_failed".to_string(),
                storage_backend: primary_target.storage_backend.clone(),
                storage_account: None,
                blob_container: Some(primary_target.container.clone()),
                blob_path: Some(blob_path),
                blob_reference: None,
                local_fallback_path: Some(fallback_path.to_string_lossy().into_owned()),
            })
        }
        ArchiveStoragePlan::LocalOnly { reason } => {
            let fallback_path =
                persist_local_fallback(zip_path, task_id, execution_id, Some(fallback_root))?;
            Ok(ArchiveUploadResult {
                status: "local_only".to_string(),
                storage_backend: format!("local_fallback:{}", reason),
                storage_account: None,
                blob_container: None,
                blob_path: None,
                blob_reference: None,
                local_fallback_path: Some(fallback_path.to_string_lossy().into_owned()),
            })
        }
    }
}

fn upload_archive_bytes(
    target: &AzureArchiveTarget,
    blob_path: &str,
    bytes: &[u8],
) -> Result<(), SchedulerError> {
    match &target.auth {
        AzureArchiveAuth::ContainerSasUrl(url) => {
            let upload_url = build_blob_url(url, blob_path);
            let response = Client::new()
                .put(upload_url)
                .header("x-ms-blob-type", "BlockBlob")
                .body(bytes.to_vec())
                .send()
                .map_err(|err| {
                    // `reqwest::Error` includes the full request URL by default, which would leak
                    // the container SAS query params into logs.
                    SchedulerError::Storage(format!("blob upload failed: {}", err.without_url()))
                })?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                return Err(SchedulerError::Storage(format!(
                    "blob upload failed (status {}): {}",
                    status, body
                )));
            }
            Ok(())
        }
        AzureArchiveAuth::AccountSas { account, sas_token } => {
            let base_url = format!(
                "https://{}.blob.core.windows.net/{}?{}",
                account, target.container, sas_token
            );
            let upload_url = build_blob_url(&base_url, blob_path);
            let response = Client::new()
                .put(upload_url)
                .header("x-ms-blob-type", "BlockBlob")
                .body(bytes.to_vec())
                .send()
                .map_err(|err| {
                    // `reqwest::Error` includes the full request URL by default, which would leak
                    // the account SAS query params into logs.
                    SchedulerError::Storage(format!("blob upload failed: {}", err.without_url()))
                })?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                return Err(SchedulerError::Storage(format!(
                    "blob upload failed (status {}): {}",
                    status, body
                )));
            }
            Ok(())
        }
        AzureArchiveAuth::ConnectionString {
            account,
            account_key,
        } => {
            upload_via_connection_string(account, account_key, &target.container, blob_path, bytes)
        }
    }
}

fn upload_via_connection_string(
    account: &str,
    account_key: &str,
    container: &str,
    blob_path: &str,
    bytes: &[u8],
) -> Result<(), SchedulerError> {
    let account = account.to_string();
    let account_key = account_key.to_string();
    let container = container.to_string();
    let blob_path = blob_path.to_string();
    let payload = bytes.to_vec();

    std::thread::Builder::new()
        .name("task-debug-archive-upload".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| {
                    SchedulerError::Storage(format!("tokio runtime init failed: {}", err))
                })?;
            runtime.block_on(async move {
                let creds = StorageCredentials::access_key(&account, account_key);
                let container_client =
                    BlobServiceClient::new(&account, creds).container_client(container);
                if !container_client.exists().await.map_err(|err| {
                    SchedulerError::Storage(format!("container exists check failed: {}", err))
                })? {
                    let create_result = container_client.create().await;
                    if let Err(err) = create_result {
                        let already_exists = err
                            .as_http_error()
                            .map(|http| http.status() == StatusCode::Conflict)
                            .unwrap_or(false);
                        if !already_exists {
                            return Err(SchedulerError::Storage(format!(
                                "container create failed: {}",
                                err
                            )));
                        }
                    }
                }
                let blob_client = container_client.blob_client(blob_path);
                blob_client
                    .put_block_blob(payload)
                    .content_type("application/zip")
                    .await
                    .map_err(|err| {
                        SchedulerError::Storage(format!("blob upload failed: {}", err))
                    })?;
                Ok(())
            })
        })
        .map_err(|err| {
            SchedulerError::Storage(format!("archive upload thread spawn failed: {}", err))
        })?
        .join()
        .map_err(|_| SchedulerError::Storage("archive upload thread panicked".to_string()))?
}

fn persist_local_fallback(
    zip_path: &Path,
    task_id: Uuid,
    execution_id: i64,
    archive_root: Option<&Path>,
) -> Result<PathBuf, SchedulerError> {
    let fallback_root = archive_root
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            zip_path
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(env::temp_dir)
        })
        .join(LOCAL_FALLBACK_DIRNAME);
    fs::create_dir_all(&fallback_root)?;
    let target_path = fallback_root.join(format!("{}_{}.zip", task_id, execution_id));
    fs::copy(zip_path, &target_path)?;
    Ok(target_path)
}

fn resolve_archive_storage_plan() -> ArchiveStoragePlan {
    let desired_container = var_with_scale_oliver("AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES")
        .unwrap_or_else(|| DEFAULT_ARCHIVE_CONTAINER.to_string());
    let mut targets = Vec::new();
    if let Some(url) = var_with_scale_oliver("AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES_SAS_URL")
    {
        let storage_account = parse_account_from_sas_url(&url);
        let container =
            parse_container_from_sas_url(&url).unwrap_or_else(|| desired_container.clone());
        targets.push(AzureArchiveTarget {
            container,
            auth: AzureArchiveAuth::ContainerSasUrl(url),
            path_prefix: DEFAULT_ARCHIVE_PATH_PREFIX.to_string(),
            storage_backend: "azure_blob".to_string(),
            storage_account,
        });
    }

    if let (Some(account), Some(sas_token)) = (
        var_with_scale_oliver("AZURE_STORAGE_ACCOUNT"),
        var_with_scale_oliver("AZURE_STORAGE_SAS_TOKEN")
            .map(|value| value.trim_start_matches('?').to_string())
            .filter(|value| !value.is_empty()),
    ) {
        targets.push(AzureArchiveTarget {
            container: desired_container.clone(),
            auth: AzureArchiveAuth::AccountSas {
                account: account.clone(),
                sas_token,
            },
            path_prefix: DEFAULT_ARCHIVE_PATH_PREFIX.to_string(),
            storage_backend: "azure_blob".to_string(),
            storage_account: Some(account),
        });
    }

    if let Some(connection_string) = resolve_connection_string() {
        if let (Some(account), Some(account_key)) = (
            parse_connection_string_component(&connection_string, "AccountName"),
            parse_connection_string_component(&connection_string, "AccountKey"),
        ) {
            targets.push(AzureArchiveTarget {
                container: desired_container.clone(),
                auth: AzureArchiveAuth::ConnectionString {
                    account: account.clone(),
                    account_key,
                },
                path_prefix: DEFAULT_ARCHIVE_PATH_PREFIX.to_string(),
                storage_backend: "azure_blob".to_string(),
                storage_account: Some(account),
            });
        }
    }

    if let Some(url) = var_with_scale_oliver("AZURE_STORAGE_CONTAINER_SAS_URL") {
        let fallback_container = var_with_scale_oliver("AZURE_STORAGE_CONTAINER_INGEST")
            .or_else(|| parse_container_from_sas_url(&url))
            .unwrap_or_else(|| DEFAULT_ARCHIVE_CONTAINER.to_string());
        let storage_account = parse_account_from_sas_url(&url);
        targets.push(AzureArchiveTarget {
            container: fallback_container,
            auth: AzureArchiveAuth::ContainerSasUrl(url),
            path_prefix: DEFAULT_ARCHIVE_PATH_PREFIX.to_string(),
            storage_backend: "azure_blob_shared_container".to_string(),
            storage_account,
        });
    }

    if !targets.is_empty() {
        return ArchiveStoragePlan::Azure(targets);
    }

    ArchiveStoragePlan::LocalOnly {
        reason: "missing_azure_blob_config".to_string(),
    }
}

fn resolve_connection_string() -> Option<String> {
    var_with_scale_oliver("AZURE_STORAGE_CONNECTION_STRING_INGEST")
        .or_else(|| var_with_scale_oliver("AZURE_STORAGE_CONNECTION_STRING"))
        .or_else(|| var_with_scale_oliver("DOWHIZ_AZURE_STORAGE_CONNECTION_STRING"))
}

fn parse_connection_string_component(connection_string: &str, key: &str) -> Option<String> {
    for segment in connection_string.split(';') {
        let mut parts = segment.splitn(2, '=');
        let candidate_key = parts.next()?.trim();
        let value = parts.next()?.trim();
        if candidate_key == key && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn parse_container_from_sas_url(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let path = without_scheme.split('/').nth(1)?;
    let container = path.split('?').next()?.trim();
    if container.is_empty() {
        None
    } else {
        Some(container.to_string())
    }
}

fn parse_account_from_sas_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.trim();
    let account = host.split('.').next()?.trim();
    if account.is_empty() {
        None
    } else {
        Some(account.to_string())
    }
}

fn build_blob_reference(storage_account: Option<&str>, container: &str, blob_path: &str) -> String {
    if let Some(account) = storage_account.filter(|value| !value.trim().is_empty()) {
        format!("azure://{}/{}/{}", account, container, blob_path)
    } else {
        format!("azure://{}/{}", container, blob_path)
    }
}

fn build_blob_url(container_url: &str, blob_path: &str) -> String {
    let mut parts = container_url.splitn(2, '?');
    let base = parts.next().unwrap_or("").trim_end_matches('/');
    let query = parts.next().unwrap_or("").trim_start_matches('?');
    if query.is_empty() {
        format!("{}/{}", base, blob_path)
    } else {
        format!("{}/{}?{}", base, blob_path, query)
    }
}

fn build_archive_blob_path(
    prefix: &str,
    task_id: Uuid,
    execution_id: i64,
    started_at: DateTime<Utc>,
) -> String {
    format!(
        "{}/{}/{}/{}/{}/{}-v{}.zip",
        prefix,
        started_at.format("%Y"),
        started_at.format("%m"),
        started_at.format("%d"),
        task_id,
        execution_id,
        ARCHIVE_VERSION
    )
}

fn collect_runtime_snapshot(workspace_dir: &Path) -> RuntimeSnapshot {
    RuntimeSnapshot {
        env_allowlist: collect_env_allowlist(),
        env_redacted: collect_redacted_envs(),
        tool_versions: collect_tool_versions(),
        git: collect_git_snapshot(workspace_dir),
    }
}

fn collect_env_allowlist() -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for key in ENV_ALLOWLIST {
        if let Ok(value) = env::var(key) {
            if !value.trim().is_empty() {
                values.insert((*key).to_string(), value);
            }
        }
    }
    values
}

fn collect_redacted_envs() -> BTreeMap<String, RedactedEnvValue> {
    let mut values = BTreeMap::new();
    for (key, value) in env::vars() {
        let upper = key.to_ascii_uppercase();
        if SENSITIVE_ENV_MARKERS
            .iter()
            .any(|marker| upper.contains(marker))
        {
            values.insert(
                key,
                RedactedEnvValue {
                    value_length: value.len(),
                },
            );
        }
    }
    values
}

fn collect_tool_versions() -> BTreeMap<String, String> {
    let mut versions = BTreeMap::new();
    for (tool, args) in [
        ("git", vec!["--version"]),
        ("docker", vec!["--version"]),
        ("az", vec!["version", "--output", "json"]),
        ("codex", vec!["--version"]),
        ("node", vec!["--version"]),
        ("npm", vec!["--version"]),
    ] {
        versions.insert(tool.to_string(), capture_command_output(tool, &args));
    }
    versions
}

fn collect_git_snapshot(workspace_dir: &Path) -> GitSnapshot {
    let repo_root = capture_git_output(workspace_dir, &["rev-parse", "--show-toplevel"]);
    if repo_root.trim().is_empty() {
        return GitSnapshot::default();
    }
    GitSnapshot {
        repo_root: Some(repo_root.trim().to_string()),
        head: Some(
            capture_git_output(workspace_dir, &["rev-parse", "HEAD"])
                .trim()
                .to_string(),
        ),
        status: Some(capture_git_output(
            workspace_dir,
            &["-c", "core.pager=cat", "status", "--short", "--branch"],
        )),
        remotes: Some(capture_git_output(workspace_dir, &["remote", "-v"])),
        diff_head: Some(truncate_string(
            &capture_git_output(
                workspace_dir,
                &[
                    "-c",
                    "core.pager=cat",
                    "diff",
                    "--binary",
                    "--no-ext-diff",
                    "HEAD",
                ],
            ),
            MAX_GIT_CAPTURE_BYTES,
        )),
        diff_staged: Some(truncate_string(
            &capture_git_output(
                workspace_dir,
                &[
                    "-c",
                    "core.pager=cat",
                    "diff",
                    "--cached",
                    "--binary",
                    "--no-ext-diff",
                ],
            ),
            MAX_GIT_CAPTURE_BYTES,
        )),
    }
}

fn capture_git_output(workspace_dir: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command.current_dir(workspace_dir).args(args);
    capture_command(command)
}

fn capture_command_output(command: &str, args: &[&str]) -> String {
    let mut cmd = Command::new(command);
    cmd.args(args);
    truncate_string(&capture_command(cmd), MAX_TOOL_OUTPUT_BYTES)
}

fn capture_command(mut command: Command) -> String {
    match command.output() {
        Ok(output) => {
            let mut combined = String::new();
            combined.push_str(&String::from_utf8_lossy(&output.stdout));
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
            combined.trim().to_string()
        }
        Err(err) => format!("command_failed: {}", err),
    }
}

fn copy_file_and_hash(source: &Path, destination: &Path) -> Result<(String, u64), SchedulerError> {
    let mut input = File::open(source)?;
    let mut output = File::create(destination)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut total_bytes = 0u64;
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        total_bytes += read as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), total_bytes))
}

fn hash_file(path: &Path) -> Result<(String, u64), SchedulerError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut total_bytes = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total_bytes += read as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), total_bytes))
}

fn sha256_file(path: &Path) -> Result<String, SchedulerError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn normalize_archive_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn truncate_string(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::super::types::{RunTaskTask, Schedule};
    use super::*;
    use crate::channel::Channel;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = env::var(key).ok();
            env::set_var(key, value);
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = env::var(key).ok();
            env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.previous {
                env::set_var(self.key, value);
            } else {
                env::remove_var(self.key);
            }
        }
    }

    fn sample_task(workspace_dir: &Path) -> ScheduledTask {
        ScheduledTask {
            id: Uuid::new_v4(),
            kind: TaskKind::RunTask(RunTaskTask {
                workspace_dir: workspace_dir.to_path_buf(),
                input_email_dir: PathBuf::from("incoming_email"),
                input_attachments_dir: PathBuf::from("incoming_attachments"),
                memory_dir: PathBuf::from("memory"),
                reference_dir: PathBuf::from("references"),
                model_name: "gpt-5.4".to_string(),
                runner: "codex".to_string(),
                codex_disabled: false,
                reply_to: vec!["user@example.com".to_string()],
                reply_from: Some("service@example.com".to_string()),
                archive_root: None,
                thread_id: Some("thread-1".to_string()),
                thread_epoch: Some(1),
                thread_state_path: None,
                channel: Channel::Email,
                slack_team_id: None,
                employee_id: Some("little_bear".to_string()),
                requester_identifier_type: Some("email".to_string()),
                requester_identifier: Some("user@example.com".to_string()),
                account_id: None,
                channel_metadata: Default::default(),
            }),
            schedule: Schedule::OneShot { run_at: Utc::now() },
            enabled: true,
            created_at: Utc::now(),
            last_run: None,
        }
    }

    #[test]
    fn classify_snapshot_file_redacts_expected_secret_paths() {
        assert!(matches!(
            classify_snapshot_file(Path::new(".env")),
            SnapshotDecision::Redact(_)
        ));
        assert!(matches!(
            classify_snapshot_file(Path::new(".secrets/api.txt")),
            SnapshotDecision::Redact(_)
        ));
        assert!(matches!(
            classify_snapshot_file(Path::new(".auth/google_workspace_cli_credentials.json")),
            SnapshotDecision::Redact(_)
        ));
        assert!(matches!(
            classify_snapshot_file(Path::new("incoming_email/body.txt")),
            SnapshotDecision::Include
        ));
    }

    #[test]
    fn pending_archive_finalize_local_only_writes_zip_with_expected_entries() {
        let _guard = env_lock().lock().expect("env lock");
        let _env_guards = vec![
            EnvVarGuard::set("TASK_DEBUG_ARCHIVE_ENABLED", "1"),
            EnvVarGuard::unset("AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES_SAS_URL"),
            EnvVarGuard::unset("AZURE_STORAGE_ACCOUNT"),
            EnvVarGuard::unset("AZURE_STORAGE_SAS_TOKEN"),
            EnvVarGuard::unset("AZURE_STORAGE_CONNECTION_STRING_INGEST"),
            EnvVarGuard::unset("AZURE_STORAGE_CONNECTION_STRING"),
            EnvVarGuard::unset("AZURE_STORAGE_CONTAINER_SAS_URL"),
            EnvVarGuard::unset("AZURE_STORAGE_CONTAINER_INGEST"),
            EnvVarGuard::unset("DOWHIZ_AZURE_STORAGE_CONNECTION_STRING"),
        ];

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path();
        fs::create_dir_all(workspace.join("incoming_email")).expect("incoming_email");
        fs::create_dir_all(workspace.join("incoming_attachments")).expect("incoming_attachments");
        fs::create_dir_all(workspace.join("memory")).expect("memory");
        fs::create_dir_all(workspace.join("references")).expect("references");
        fs::create_dir_all(workspace.join(".secrets")).expect("secrets");
        fs::create_dir_all(
            workspace
                .join(run_task_module::RUN_TASK_TRACE_DIRNAME)
                .join("aci"),
        )
        .expect("trace");
        fs::write(workspace.join("incoming_email/body.txt"), "hello").expect("body");
        fs::write(workspace.join(".env"), "SECRET=1").expect("env");
        fs::write(workspace.join(".secrets/token.txt"), "topsecret").expect("token");
        fs::write(
            workspace
                .join(run_task_module::RUN_TASK_TRACE_DIRNAME)
                .join("aci")
                .join("container_logs.txt"),
            "aci logs",
        )
        .expect("aci logs");

        let task_before = sample_task(workspace);
        let session = PendingTaskDebugArchive::begin(&task_before, 42, Utc::now())
            .expect("begin archive")
            .expect("session");

        fs::write(
            workspace.join("reply_email_draft.html"),
            "<html><body>done</body></html>",
        )
        .expect("reply");
        let task_after = task_before.clone();
        let record = session
            .finalize(&task_before, &task_after, Utc::now(), "success", None)
            .expect("finalize archive");

        assert_eq!(record.status, "local_only");
        assert_eq!(record.storage_account, None);
        assert!(record.has_run_task_trace);
        assert!(record.has_aci_logs);
        assert!(record.redacted_file_count >= 2);

        let archive_path = PathBuf::from(
            record
                .local_fallback_path
                .clone()
                .expect("local fallback path"),
        );
        assert!(archive_path.exists());

        let file = File::open(&archive_path).expect("open zip");
        let mut zip = zip::ZipArchive::new(file).expect("zip archive");
        assert!(zip.by_name("manifest.json").is_ok());
        assert!(zip.by_name("task/task_before.json").is_ok());
        assert!(zip.by_name("manifests/workspace_before.json").is_ok());
        assert!(
            zip.by_name("workspace_after/reply_email_draft.html")
                .is_ok(),
            "reply draft should be captured in after snapshot"
        );
    }

    #[test]
    fn resolve_archive_storage_plan_builds_ordered_candidates_with_precise_accounts() {
        let _guard = env_lock().lock().expect("env lock");
        let _env_guards = [
            EnvVarGuard::set(
                "AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES_SAS_URL",
                "https://archiveacct.blob.core.windows.net/archive-zips?sv=test",
            ),
            EnvVarGuard::set("AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES", "ignored"),
            EnvVarGuard::set("AZURE_STORAGE_ACCOUNT", "accountsas"),
            EnvVarGuard::set("AZURE_STORAGE_SAS_TOKEN", "?sig=test"),
            EnvVarGuard::set(
                "AZURE_STORAGE_CONNECTION_STRING",
                "DefaultEndpointsProtocol=https;AccountName=connacct;AccountKey=abc;EndpointSuffix=core.windows.net",
            ),
            EnvVarGuard::set(
                "AZURE_STORAGE_CONTAINER_SAS_URL",
                "https://sharedacct.blob.core.windows.net/shared-ingest?sv=test",
            ),
            EnvVarGuard::set("AZURE_STORAGE_CONTAINER_INGEST", "shared-ingest"),
        ];

        match resolve_archive_storage_plan() {
            ArchiveStoragePlan::Azure(targets) => {
                assert_eq!(targets.len(), 4);
                assert_eq!(targets[0].container, "archive-zips");
                assert_eq!(targets[0].storage_account.as_deref(), Some("archiveacct"));
                assert_eq!(targets[1].container, "ignored");
                assert_eq!(targets[1].storage_account.as_deref(), Some("accountsas"));
                assert_eq!(targets[2].container, "ignored");
                assert_eq!(targets[2].storage_account.as_deref(), Some("connacct"));
                assert_eq!(targets[3].container, "shared-ingest");
                assert_eq!(targets[3].storage_account.as_deref(), Some("sharedacct"));
            }
            ArchiveStoragePlan::LocalOnly { .. } => {
                panic!("expected azure candidates");
            }
        }
    }

    #[test]
    fn build_blob_reference_includes_storage_account_when_available() {
        assert_eq!(
            build_blob_reference(
                Some("archiveacct"),
                "task-debug-archives",
                "task_debug_archives/2026/03/23/task/1-v1.zip",
            ),
            "azure://archiveacct/task-debug-archives/task_debug_archives/2026/03/23/task/1-v1.zip"
        );
        assert_eq!(
            build_blob_reference(
                None,
                "task-debug-archives",
                "task_debug_archives/2026/03/23/task/1-v1.zip",
            ),
            "azure://task-debug-archives/task_debug_archives/2026/03/23/task/1-v1.zip"
        );
    }
}
