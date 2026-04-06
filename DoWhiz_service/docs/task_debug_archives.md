# Task Debug Archive Runbook

Use this guide when a `RunTask` execution needs to be debugged after the fact, especially when the
Azure ACI container has already been deleted.

Every completed `RunTask` should leave behind:
- a Mongo lookup row in collection `task_debug_archives`
- an Azure Blob zip when upload succeeded
- or a `local_fallback_path` when Azure upload failed or was disabled

## 1) Find the service directory and load runtime env

Current staging/prod deployments use:

```bash
APP_DIR=/home/azureuser/server/DoWhiz/DoWhiz_service
cd "$APP_DIR"
set -a
source .env
set +a
```

If you are unsure which repo path a VM uses:

```bash
find /home/azureuser/server -maxdepth 3 -type d -name DoWhiz_service 2>/dev/null
```

If `pm2` is missing from `PATH`, load `nvm` first:

```bash
if [ -s /home/azureuser/.nvm/nvm.sh ]; then
  source /home/azureuser/.nvm/nvm.sh
fi
```

## 2) Check live worker/gateway health and logs

For an active or stuck run, start here before looking at historical bundles:

```bash
curl -sS http://127.0.0.1:9100/health
curl -sS http://127.0.0.1:9001/health
pgrep -af inbound_gateway
pgrep -af rust_service
pm2 logs dw_gateway --lines 200
pm2 logs dw_worker --lines 200
```

If the run is still in flight and `RUN_TASK_EXECUTION_BACKEND=azure_aci`, also inspect the worker
log for lines like:
- `[run_task] azure_aci create ...`
- `[run_task] azure_aci finished ...`
- `[task_debug_archive] upload attempt failed ...`

## 2a) Understand stale `running` execution cleanup

Recent scheduler builds reconcile orphaned `task_executions.status="running"` rows in two places:
- once on worker startup
- again right before a due task checks whether it is already running

The reconciliation rules are:
- older `running` rows that are already covered by a newer execution start or a later terminal
  completion are closed as `superseded`
- the newest `running` row is preserved if it is still within the watchdog timeout window
- the newest `running` row is closed as `failed` once it ages past the watchdog timeout without a
  terminal status

When debugging a live task, expect at most one recent `running` row for a task after the worker has
had a chance to start up and sweep stale state.

## 3) Find the archive row in Mongo

Latest rows:

```bash
mongosh "$MONGODB_URI" --quiet <<'MONGO'
db = db.getSiblingDB(process.env.MONGODB_DATABASE);
print(EJSON.stringify(
  db.task_debug_archives.find().sort({ created_at: -1 }).limit(5).toArray(),
  null,
  2
));
MONGO
```

Specific task:

```bash
export TASK_ID="<task-id>"
mongosh "$MONGODB_URI" --quiet <<'MONGO'
db = db.getSiblingDB(process.env.MONGODB_DATABASE);
print(EJSON.stringify(
  db.task_debug_archives.find({ task_id: process.env.TASK_ID }).sort({ created_at: -1 }).toArray(),
  null,
  2
));
MONGO
```

Fields to pay attention to:
- `status`
- `storage_account`
- `blob_container`
- `blob_path`
- `blob_reference`
- `local_fallback_path`
- `archive_build_duration_ms`
- `upload_duration_ms`
- `has_run_task_trace`
- `has_aci_logs`
- `created_at`

## 4) Interpret archive status

- `uploaded`: the zip is durable in Azure Blob; use `storage_account`, `blob_container`, and
  `blob_path` as the source of truth.
- `upload_failed`: the worker still wrote a zip to `local_fallback_path`, but this may not be
  durable if the workspace was later cleaned up.
- `local_only`: Azure upload was not configured; only `local_fallback_path` exists.

If `status` is not `uploaded` on staging/production, treat that as a storage config issue to fix
before relying on the archive for long-term debugging.

## 5) Choose the correct Azure auth path

The archive row records the actual `storage_account` used. Compare it against the configured auth
candidates before downloading:

```bash
python3 - <<'PY'
import os
from urllib.parse import urlparse

def connection_string_account(value: str) -> str:
    for part in value.split(";"):
        if part.startswith("AccountName="):
            return part.split("=", 1)[1]
    return ""

def sas_account(url: str) -> str:
    host = urlparse(url).hostname or ""
    return host.split(".")[0] if host else ""

print("AZURE_STORAGE_CONNECTION_STRING account:",
      connection_string_account(os.getenv("AZURE_STORAGE_CONNECTION_STRING", "")) or "-")
print("AZURE_STORAGE_ACCOUNT:", os.getenv("AZURE_STORAGE_ACCOUNT", "") or "-")
print("AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES_SAS_URL account:",
      sas_account(os.getenv("AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES_SAS_URL", "")) or "-")
print("AZURE_STORAGE_CONTAINER_SAS_URL account:",
      sas_account(os.getenv("AZURE_STORAGE_CONTAINER_SAS_URL", "")) or "-")
PY
```

Use the auth method whose account matches the row's `storage_account`.

## 6) Download the archive bundle

Connection string match:

```bash
BLOB_CONTAINER="<blob-container>"
BLOB_PATH="<blob-path>"
az storage blob download \
  --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
  --container-name "$BLOB_CONTAINER" \
  --name "$BLOB_PATH" \
  --file /tmp/task_debug_archive.zip \
  --overwrite true
```

Account SAS match:

```bash
BLOB_CONTAINER="<blob-container>"
BLOB_PATH="<blob-path>"
az storage blob download \
  --account-name "$AZURE_STORAGE_ACCOUNT" \
  --sas-token "$AZURE_STORAGE_SAS_TOKEN" \
  --container-name "$BLOB_CONTAINER" \
  --name "$BLOB_PATH" \
  --file /tmp/task_debug_archive.zip \
  --overwrite true
```

Container SAS URL match:

```bash
CONTAINER_SAS_URL="${AZURE_STORAGE_CONTAINER_TASK_DEBUG_ARCHIVES_SAS_URL:-$AZURE_STORAGE_CONTAINER_SAS_URL}"
BLOB_PATH="<blob-path>"
python3 - <<'PY'
import os
from pathlib import Path
from urllib.request import urlretrieve

container_url = os.environ["CONTAINER_SAS_URL"]
blob_path = os.environ["BLOB_PATH"]
base, _, query = container_url.partition("?")
url = f"{base.rstrip('/')}/{blob_path}?{query}"
target = Path("/tmp/task_debug_archive.zip")
urlretrieve(url, target)
print(target)
PY
```

Quick existence check:

```bash
az storage blob exists \
  --connection-string "$AZURE_STORAGE_CONNECTION_STRING" \
  --container-name "$BLOB_CONTAINER" \
  --name "$BLOB_PATH" \
  --query exists -o tsv
```

## 7) Inspect the bundle contents

Core files that should be present in a healthy full debug bundle:

```bash
python3 - <<'PY'
from zipfile import ZipFile

want = [
    "manifest.json",
    "manifests/workspace_before.json",
    "manifests/workspace_after.json",
    "manifests/workspace_diff.json",
    "runtime/env_allowlist.json",
    "runtime/env_redacted.json",
    "runtime/tool_versions.json",
    "runtime/git.json",
    "workspace_before/thread_state.json",
    "workspace_after/thread_state.json",
    "workspace_after/reply_email_draft.html",
    "workspace_after/.run_task_trace/metadata.json",
    "workspace_after/.run_task_trace/prompt.txt",
    "workspace_after/.run_task_trace/logs/combined.log",
    "workspace_after/.run_task_trace/logs/stdout.log",
    "workspace_after/.run_task_trace/logs/stderr.log",
    "workspace_after/.run_task_trace/aci/container_show.json",
    "workspace_after/.run_task_trace/aci/container_logs.txt",
    "workspace_after/.run_task_trace/aci/remote_output.log",
]

with ZipFile("/tmp/task_debug_archive.zip") as zf:
    names = set(zf.namelist())
    for item in want:
        print(f"{item}: {item in names}")
PY
```

What these files are for:
- `manifest.json`: top-level archive metadata
- `manifests/workspace_before.json`, `workspace_after.json`, `workspace_diff.json`: file-level
  inventory and delta
- `runtime/*`: sanitized env view, tool versions, and git snapshot
- `.run_task_trace/logs/*`: stdout, stderr, and combined run logs
- `.run_task_trace/aci/*`: the captured Azure container metadata and logs, even after the live
  container is deleted
- `reply_email_draft.html`, `thread_state.json`, `memo.md`: user-facing task outcome context

## 8) Reproduce with a known-good live E2E

Staging live email E2E:

```bash
cd /home/azureuser/server/DoWhiz/DoWhiz_service
if [ -s /home/azureuser/.nvm/nvm.sh ]; then
  source /home/azureuser/.nvm/nvm.sh
fi
pm2 stop dw_gateway || true
pm2 stop dw_worker || true

RUN_CODEX_E2E=1 \
POSTMARK_LIVE_TEST=1 \
RUST_SERVICE_LIVE_TEST=1 \
POSTMARK_SMTP_PORT=2525 \
POSTMARK_TEST_SERVICE_ADDRESS=dowhiz@deep-tutor.com \
POSTMARK_TEST_FROM=deep-tutor@deep-tutor.com \
cargo test -p scheduler_module --test service_real_email -- --nocapture

pm2 restart dw_gateway --update-env
pm2 restart dw_worker --update-env
pm2 save
```

Why this matters:
- `service_real_email` binds local ports `9100` and `9001`, so the PM2-managed services should be
  stopped first.
- Current staging SMTP routing requires `POSTMARK_SMTP_PORT=2525`.
- After the test, confirm a new `task_debug_archives` row was created and that `status=uploaded`.
