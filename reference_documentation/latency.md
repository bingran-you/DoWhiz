# Task Latency Analysis

## Download timing data from staging

```bash
scp dowhizstaging:/home/azureuser/server/DoWhiz/DoWhiz_service/task_timings.jsonl /tmp/
```

## Generate plots

```bash
cd DoWhiz/DoWhiz_service
python scripts/analyze_timings.py /tmp/task_timings.jsonl
```

## View plots manually

```bash
open /tmp/mean_breakdown.png
open /tmp/stacked_timeline.png
open /tmp/distribution.png
```

## Timing stages

- `setup_latency_ms` - Initial setup before ACI
- `ephemeral_share_create_ms` - Creating Azure file share
- `aci_cold_start_ms` - ACI container spin-up/provisioning from pre-built image
- `codex_execution_ms` - Actual codex running
- `result_download_ms` - Downloading results from ephemeral task fileshare to global fileshare @
`/home/azureuser/server/.dowhiz/DoWhiz/run_task`

## CLI Commands

The `timing_cli` binary provides commands for managing timing logs.

### Build

```bash
cd DoWhiz_service
cargo build -p run_task_module --bin timing_cli --release
```

### Commands (on Local Machine, after scp)

```bash
# Clear the timing log (start fresh)
TIMING_LOG_PATH=/tmp/task_timings.jsonl ./target/release/timing_cli clear

# Show the path to the timing log
TIMING_LOG_PATH=/tmp/task_timings.jsonl ./target/release/timing_cli path

# Display timing log contents
TIMING_LOG_PATH=/tmp/task_timings.jsonl ./target/release/timing_cli show

# Run Python analysis script to generate plots
TIMING_LOG_PATH=/tmp/task_timings.jsonl ./target/release/timing_cli analyze

# Help
./target/release/timing_cli help
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TIMING_LOG_PATH` | `./task_timings.jsonl` | Path to the JSONL timing log file |
| `ANALYZE_SCRIPT_PATH` | `scripts/analyze_timings.py` | Path to the Python analysis script |

---

## Warm Pool Architecture

Warm pool containers eliminate cold start latency by keeping pre-provisioned ACI containers polling for tasks.

### Warm Pool Timing Stages

| Stage | Description |
|-------|-------------|
| `ephemeral_share_create_ms` | Creating Azure File Share for workspace |
| `ephemeral_share_upload_ms` | Uploading workspace files to share |
| `codex_execution_ms` | Queue wait + agent execution time |
| `result_download_ms` | Downloading results from share |

Note: `setup_latency_ms` and `aci_cold_start_ms` do not apply to warm pool - setup happens on the scheduler before pushing to queue, and containers are already running.
