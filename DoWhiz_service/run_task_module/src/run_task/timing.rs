use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};

/// Serializable version of TaskTiming with durations as milliseconds
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskTimingRecord {
    pub task_id: String,
    pub timestamp: String,
    pub queue_latency_ms: Option<f64>,
    pub setup_latency_ms: Option<f64>,
    pub ephemeral_share_create_ms: Option<f64>,
    pub ephemeral_share_upload_ms: Option<f64>,
    pub aci_cold_start_ms: Option<f64>,
    pub codex_execution_ms: Option<f64>,
    pub result_download_ms: Option<f64>,
    pub total_ms: Option<f64>,
}

impl From<&TaskTiming> for TaskTimingRecord {
    fn from(t: &TaskTiming) -> Self {
        Self {
            task_id: t.task_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            queue_latency_ms: t.queue_latency.map(|d| d.as_secs_f64() * 1000.0),
            setup_latency_ms: t.setup_latency.map(|d| d.as_secs_f64() * 1000.0),
            ephemeral_share_create_ms: t.ephemeral_share_create.map(|d| d.as_secs_f64() * 1000.0),
            ephemeral_share_upload_ms: t.ephemeral_share_upload.map(|d| d.as_secs_f64() * 1000.0),
            aci_cold_start_ms: t.aci_cold_start.map(|d| d.as_secs_f64() * 1000.0),
            codex_execution_ms: t.codex_execution.map(|d| d.as_secs_f64() * 1000.0),
            result_download_ms: t.result_download.map(|d| d.as_secs_f64() * 1000.0),
            total_ms: t.total.map(|d| d.as_secs_f64() * 1000.0),
        }
    }
}

fn timing_log_path() -> PathBuf {
    //write to ./task_timings.jsonl
    std::env::var("TIMING_LOG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./task_timings.jsonl"))
}

pub fn append_timing_to_file(timing: &TaskTiming) {
    let record = TaskTimingRecord::from(timing);
    let path = timing_log_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string(&record) {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            //Write via JSONL format
            let _ = writeln!(file, "{}", json);
        }
    }
}

/// Clear the timing log file, removing all recorded entries.
/// Returns Ok(()) if successful or if the file doesn't exist.
pub fn clear_timing_log() -> std::io::Result<()> {
    let path = timing_log_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Get the path to the timing log file.
pub fn get_timing_log_path() -> PathBuf {
    timing_log_path()
}

#[derive(Debug, Clone, Default)]
pub struct TaskTiming {
    pub task_id: String,
    pub queue_latency: Option<Duration>,
    pub setup_latency: Option<Duration>,
    pub ephemeral_share_create: Option<Duration>,
    pub ephemeral_share_upload: Option<Duration>,
    pub aci_cold_start: Option<Duration>,
    pub codex_execution: Option<Duration>,
    pub result_download: Option<Duration>,
    pub total: Option<Duration>,
}

pub struct TaskTimingBuilder {
    task_id: String,
    start: Instant,
    current_stage: Option<Instant>,
    timing: TaskTiming,
}

impl TaskTimingBuilder {
    pub fn new(task_id: impl Into<String>) -> Self {
        let task_id = task_id.into();
        Self {
            task_id: task_id.clone(),
            start: Instant::now(),
            current_stage: None,
            timing: TaskTiming {
                task_id,
                ..Default::default()
            },
        }
    }

    pub fn set_task_id(&mut self, task_id: impl Into<String>) {
        let id = task_id.into();
        self.task_id = id.clone();
        self.timing.task_id = id;
    }

    pub fn start_stage(&mut self) {
        self.current_stage = Some(Instant::now());
    }

    pub fn end_queue_latency(&mut self) {
        if let Some(start) = self.current_stage.take() {
            self.timing.queue_latency = Some(start.elapsed());
        }
    }

    pub fn end_setup(&mut self) {
        if let Some(start) = self.current_stage.take() {
            self.timing.setup_latency = Some(start.elapsed());
        }
    }

    pub fn end_ephemeral_create(&mut self) {
        if let Some(start) = self.current_stage.take() {
            self.timing.ephemeral_share_create = Some(start.elapsed());
        }
    }

    pub fn end_ephemeral_upload(&mut self) {
        if let Some(start) = self.current_stage.take() {
            self.timing.ephemeral_share_upload = Some(start.elapsed());
        }
    }

    pub fn end_aci_cold_start(&mut self) {
        if let Some(start) = self.current_stage.take() {
            self.timing.aci_cold_start = Some(start.elapsed());
        }
    }

    pub fn end_codex_execution(&mut self) {
        if let Some(start) = self.current_stage.take() {
            self.timing.codex_execution = Some(start.elapsed());
        }
    }

    pub fn end_result_download(&mut self) {
        if let Some(start) = self.current_stage.take() {
            self.timing.result_download = Some(start.elapsed());
        }
    }

    pub fn finish(mut self) -> TaskTiming {
        self.timing.total = Some(self.start.elapsed());
        self.timing
    }
}

pub struct TimingCollector {
    timings: RwLock<Vec<TaskTiming>>,
    max_entries: usize,
}

impl TimingCollector {
    pub fn new(max_entries: usize) -> Self {
        Self {
            timings: RwLock::new(Vec::new()),
            max_entries,
        }
    }

    pub fn record(&self, timing: TaskTiming) {
        // Write to file first
        append_timing_to_file(&timing);

        let mut timings = self.timings.write().unwrap();
        timings.push(timing);
        if timings.len() > self.max_entries {
            timings.remove(0);
        }
    }

    pub fn stats(&self) -> TimingStats {
        let timings = self.timings.read().unwrap();
        TimingStats::compute(&timings)
    }

    pub fn recent_timings(&self, count: usize) -> Vec<TaskTiming> {
        let timings = self.timings.read().unwrap();
        timings.iter().rev().take(count).cloned().collect()
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct StageStats {
    pub count: usize,
    pub mean_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub stddev_ms: f64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TimingStats {
    pub sample_count: usize,
    pub queue_latency: StageStats,
    pub setup_latency: StageStats,
    pub ephemeral_share_create: StageStats,
    pub ephemeral_share_upload: StageStats,
    pub aci_cold_start: StageStats,
    pub codex_execution: StageStats,
    pub result_download: StageStats,
    pub total: StageStats,
}

impl TimingStats {
    fn compute(timings: &[TaskTiming]) -> Self {
        Self {
            sample_count: timings.len(),
            queue_latency: Self::stage_stats(timings.iter().filter_map(|t| t.queue_latency)),
            setup_latency: Self::stage_stats(timings.iter().filter_map(|t| t.setup_latency)),
            ephemeral_share_create: Self::stage_stats(
                timings.iter().filter_map(|t| t.ephemeral_share_create),
            ),
            ephemeral_share_upload: Self::stage_stats(
                timings.iter().filter_map(|t| t.ephemeral_share_upload),
            ),
            aci_cold_start: Self::stage_stats(timings.iter().filter_map(|t| t.aci_cold_start)),
            codex_execution: Self::stage_stats(timings.iter().filter_map(|t| t.codex_execution)),
            result_download: Self::stage_stats(timings.iter().filter_map(|t| t.result_download)),
            total: Self::stage_stats(timings.iter().filter_map(|t| t.total)),
        }
    }

    pub fn stage_stats(durations: impl Iterator<Item = Duration>) -> StageStats {
        let mut values: Vec<f64> = durations.map(|d| d.as_secs_f64() * 1000.0).collect();
        if values.is_empty() {
            return StageStats::default();
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let count = values.len();
        let mean = values.iter().sum::<f64>() / count as f64;
        let median = values[count / 2];
        let p95_idx = ((count as f64 * 0.95) as usize).min(count - 1);
        let p95 = values[p95_idx];
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
        let stddev = variance.sqrt();

        StageStats {
            count,
            mean_ms: mean,
            median_ms: median,
            p95_ms: p95,
            stddev_ms: stddev,
        }
    }
}

pub static TIMING_COLLECTOR: LazyLock<TimingCollector> =
    LazyLock::new(|| TimingCollector::new(1000));

pub struct QueueLatencyCollector {
    latencies: RwLock<Vec<Duration>>,
    max_entries: usize,
}

impl QueueLatencyCollector {
    pub fn new(max_entries: usize) -> Self {
        Self {
            latencies: RwLock::new(Vec::new()),
            max_entries,
        }
    }

    pub fn record(&self, latency: Duration) {
        let mut latencies = self.latencies.write().unwrap();
        latencies.push(latency);
        if latencies.len() > self.max_entries {
            latencies.remove(0);
        }
    }

    pub fn stats(&self) -> StageStats {
        let latencies = self.latencies.read().unwrap();
        TimingStats::stage_stats(latencies.iter().copied())
    }
}

pub static QUEUE_LATENCY_COLLECTOR: LazyLock<QueueLatencyCollector> =
    LazyLock::new(|| QueueLatencyCollector::new(1000));

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_timing_builder_records_stages() {
        let mut builder = TaskTimingBuilder::new("test-task-1");

        builder.start_stage();
        sleep(Duration::from_millis(10));
        builder.end_setup();

        builder.start_stage();
        sleep(Duration::from_millis(15));
        builder.end_codex_execution();

        let timing = builder.finish();

        assert_eq!(timing.task_id, "test-task-1");
        assert!(timing.setup_latency.unwrap().as_millis() >= 10);
        assert!(timing.codex_execution.unwrap().as_millis() >= 15);
        assert!(timing.total.unwrap().as_millis() >= 25);
    }

    #[test]
    fn test_collector_computes_stats() {
        let collector = TimingCollector::new(100);

        for i in 0..10 {
            let timing = TaskTiming {
                task_id: format!("task-{}", i),
                codex_execution: Some(Duration::from_millis(100 + i * 10)),
                total: Some(Duration::from_millis(200 + i * 10)),
                ..Default::default()
            };
            collector.record(timing);
        }

        let stats = collector.stats();
        assert_eq!(stats.sample_count, 10);
        assert!(stats.codex_execution.mean_ms > 100.0);
        assert!(stats.total.mean_ms > 200.0);
    }

    #[test]
    fn test_collector_respects_max_entries() {
        let collector = TimingCollector::new(5);

        for i in 0..10 {
            let timing = TaskTiming {
                task_id: format!("task-{}", i),
                ..Default::default()
            };
            collector.record(timing);
        }

        let stats = collector.stats();
        assert_eq!(stats.sample_count, 5);
    }
}
