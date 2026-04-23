use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use super::errors::RunTaskError;
use super::types::TokenUsage;

pub const RUN_TASK_TRACE_DIRNAME: &str = ".run_task_trace";
const TRACE_METADATA_FILENAME: &str = "metadata.json";
const TRACE_PROMPT_FILENAME: &str = "prompt.txt";

#[derive(Debug, Clone, Serialize)]
struct TraceEnvVarSummary {
    key: String,
    redacted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value_length: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
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
        self.metadata.finished_at_unix_ms = Some(now_unix_ms());
        self.metadata.exit_status = exit_status;
        self.metadata.success = Some(success);
        self.metadata.error = error.map(|value| value.to_string());
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
