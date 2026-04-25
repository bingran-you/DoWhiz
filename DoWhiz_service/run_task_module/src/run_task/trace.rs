use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use super::errors::RunTaskError;
use super::timing::TaskTiming;
use super::types::TokenUsage;

pub const RUN_TASK_TRACE_DIRNAME: &str = ".run_task_trace";
const TRACE_METADATA_FILENAME: &str = "metadata.json";
const TRACE_PROMPT_FILENAME: &str = "prompt.txt";

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct TraceEnvVarSummary {
    key: String,
    redacted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value_length: Option<usize>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, Default)]
pub struct RunTaskTraceTimingMs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ephemeral_share_create_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ephemeral_share_upload_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aci_cold_start_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex_execution_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_download_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<f64>,
}

impl From<&TaskTiming> for RunTaskTraceTimingMs {
    fn from(timing: &TaskTiming) -> Self {
        Self {
            queue_latency_ms: timing.queue_latency.map(|d| d.as_secs_f64() * 1000.0),
            setup_latency_ms: timing.setup_latency.map(|d| d.as_secs_f64() * 1000.0),
            ephemeral_share_create_ms: timing
                .ephemeral_share_create
                .map(|d| d.as_secs_f64() * 1000.0),
            ephemeral_share_upload_ms: timing
                .ephemeral_share_upload
                .map(|d| d.as_secs_f64() * 1000.0),
            aci_cold_start_ms: timing.aci_cold_start.map(|d| d.as_secs_f64() * 1000.0),
            codex_execution_ms: timing.codex_execution.map(|d| d.as_secs_f64() * 1000.0),
            result_download_ms: timing.result_download.map(|d| d.as_secs_f64() * 1000.0),
            total_ms: timing.total.map(|d| d.as_secs_f64() * 1000.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct RunTaskTraceMetadata {
    trace_version: u32,
    runner: String,
    backend: String,
    model_name: String,
    deploy_target: String,
    workspace_dir: String,
    timeout_secs: u64,
    started_at_unix_ms: u64,
    finished_at_unix_ms: Option<u64>,
    exit_status: Option<i32>,
    success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage_updated_at_unix_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timing_ms: Option<RunTaskTraceTimingMs>,
    command_summary: Value,
    env_overrides: Vec<TraceEnvVarSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token_usage: Option<TokenUsage>,
}

pub(super) struct RunTaskTraceRecorder {
    workspace_dir: PathBuf,
    metadata: RunTaskTraceMetadata,
}

pub(super) fn trace_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join(RUN_TASK_TRACE_DIRNAME)
}

fn metadata_path(workspace_dir: &Path) -> PathBuf {
    trace_dir(workspace_dir).join(TRACE_METADATA_FILENAME)
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct RunTaskTraceSnapshot {
    pub runner: String,
    pub backend: String,
    pub model_name: String,
    pub deploy_target: String,
    pub started_at_unix_ms: u64,
    pub finished_at_unix_ms: Option<u64>,
    pub success: Option<bool>,
    pub error: Option<String>,
    pub current_stage: Option<String>,
    pub stage_updated_at_unix_ms: Option<u64>,
    pub timing_ms: Option<RunTaskTraceTimingMs>,
    pub token_usage: Option<TokenUsage>,
}

pub fn load_trace_snapshot(workspace_dir: &Path) -> Option<RunTaskTraceSnapshot> {
    let bytes = fs::read(metadata_path(workspace_dir)).ok()?;
    let metadata: RunTaskTraceMetadata = serde_json::from_slice(&bytes).ok()?;
    Some(RunTaskTraceSnapshot {
        runner: metadata.runner,
        backend: metadata.backend,
        model_name: metadata.model_name,
        deploy_target: metadata.deploy_target,
        started_at_unix_ms: metadata.started_at_unix_ms,
        finished_at_unix_ms: metadata.finished_at_unix_ms,
        success: metadata.success,
        error: metadata.error,
        current_stage: metadata.current_stage,
        stage_updated_at_unix_ms: metadata.stage_updated_at_unix_ms,
        timing_ms: metadata.timing_ms,
        token_usage: metadata.token_usage,
    })
}

impl RunTaskTraceRecorder {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        workspace_dir: &Path,
        runner: &str,
        backend: &str,
        model_name: &str,
        prompt: &str,
        timeout: Duration,
        command_summary: Value,
        env_overrides: &[(String, String)],
    ) -> Result<Self, RunTaskError> {
        let trace_root = trace_dir(workspace_dir);
        fs::create_dir_all(&trace_root)?;
        fs::write(trace_root.join(TRACE_PROMPT_FILENAME), prompt)?;

        let recorder = Self {
            workspace_dir: workspace_dir.to_path_buf(),
            metadata: RunTaskTraceMetadata {
                trace_version: 1,
                runner: runner.to_string(),
                backend: backend.to_string(),
                model_name: model_name.to_string(),
                deploy_target: env::var("DEPLOY_TARGET")
                    .unwrap_or_else(|_| "local".to_string())
                    .trim()
                    .to_ascii_lowercase(),
                workspace_dir: workspace_dir.to_string_lossy().into_owned(),
                timeout_secs: timeout.as_secs(),
                started_at_unix_ms: now_unix_ms(),
                finished_at_unix_ms: None,
                exit_status: None,
                success: None,
                error: None,
                current_stage: Some("initializing".to_string()),
                stage_updated_at_unix_ms: Some(now_unix_ms()),
                timing_ms: None,
                command_summary,
                env_overrides: summarize_env_overrides(env_overrides),
                token_usage: None,
            },
        };
        recorder.persist_metadata()?;
        Ok(recorder)
    }

    pub(super) fn record_outputs(
        &self,
        stdout_output: &str,
        stderr_output: &str,
        combined_output: &str,
    ) -> Result<(), RunTaskError> {
        self.write_text("logs/stdout.log", stdout_output)?;
        self.write_text("logs/stderr.log", stderr_output)?;
        self.write_text("logs/combined.log", combined_output)?;
        Ok(())
    }

    pub(super) fn set_stage(&mut self, stage: &str) -> Result<(), RunTaskError> {
        self.metadata.current_stage = Some(stage.trim().to_string());
        self.metadata.stage_updated_at_unix_ms = Some(now_unix_ms());
        self.persist_metadata()
    }

    pub(super) fn record_timing(&mut self, timing: &TaskTiming) -> Result<(), RunTaskError> {
        self.metadata.timing_ms = Some(RunTaskTraceTimingMs::from(timing));
        self.persist_metadata()
    }

    pub(super) fn record_text(&self, rel_path: &str, content: &str) -> Result<(), RunTaskError> {
        self.write_text(rel_path, content)
    }

    pub(super) fn record_json<T: Serialize>(
        &self,
        rel_path: &str,
        value: &T,
    ) -> Result<(), RunTaskError> {
        let payload = serde_json::to_vec_pretty(value).map_err(|err| {
            RunTaskError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        })?;
        self.write_bytes(rel_path, &payload)
    }

    pub(super) fn copy_file(&self, source_path: &Path, rel_path: &str) -> Result<(), RunTaskError> {
        let bytes = fs::read(source_path)?;
        self.write_bytes(rel_path, &bytes)
    }

    pub(super) fn finish(
        &mut self,
        exit_status: Option<i32>,
        success: bool,
        error: Option<&str>,
        token_usage: Option<&TokenUsage>,
    ) -> Result<(), RunTaskError> {
        let finished_at = now_unix_ms();
        self.metadata.finished_at_unix_ms = Some(finished_at);
        self.metadata.exit_status = exit_status;
        self.metadata.success = Some(success);
        self.metadata.error = error.map(|value| value.to_string());
        self.metadata.current_stage = Some(if success {
            "completed".to_string()
        } else {
            "failed".to_string()
        });
        self.metadata.stage_updated_at_unix_ms = Some(finished_at);
        self.metadata.token_usage = token_usage.cloned();
        self.persist_metadata()
    }

    fn persist_metadata(&self) -> Result<(), RunTaskError> {
        let payload = serde_json::to_vec_pretty(&self.metadata).map_err(|err| {
            RunTaskError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        })?;
        self.write_bytes(TRACE_METADATA_FILENAME, &payload)
    }

    fn write_text(&self, rel_path: &str, content: &str) -> Result<(), RunTaskError> {
        self.write_bytes(rel_path, content.as_bytes())
    }

    fn write_bytes(&self, rel_path: &str, bytes: &[u8]) -> Result<(), RunTaskError> {
        let path = trace_dir(&self.workspace_dir).join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }
}

fn summarize_env_overrides(entries: &[(String, String)]) -> Vec<TraceEnvVarSummary> {
    let mut by_key = BTreeMap::new();
    for (key, value) in entries {
        let sensitive = is_sensitive_env_key(key);
        by_key.insert(
            key.clone(),
            TraceEnvVarSummary {
                key: key.clone(),
                redacted: sensitive,
                value: if sensitive { None } else { Some(value.clone()) },
                value_length: if sensitive { Some(value.len()) } else { None },
            },
        );
    }
    by_key.into_values().collect()
}

fn is_sensitive_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.contains("KEY")
        || upper.contains("TOKEN")
        || upper.contains("SECRET")
        || upper.contains("PASSWORD")
        || upper.contains("CONNECTION_STRING")
        || upper.contains("SAS")
        || upper.contains("COOKIE")
        || upper.contains("SESSION")
        || upper.contains("AUTH")
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
