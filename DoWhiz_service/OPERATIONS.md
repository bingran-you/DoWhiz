# DoWhiz Operations Guide

Operational runbook for VM-based DoWhiz service operation.

Covers:
- `inbound_gateway`
- `rust_service` worker
- shared runtime `.env` policy
- health checks and incident triage

For deployment pipeline details, see:
- `DoWhiz_service/docs/staging_production_deploy.md`

## 1) Deployment Policy

- Staging deploy branch: `dev` (automatic deploy on push to `dev`)
- Production deploy branch: `main`

Runtime environment policy:
- Runtime services read unprefixed keys from `DoWhiz_service/.env`.
- VM `.env` is generated from `ENV_COMMON + ENV_STAGING/ENV_PROD` in CI/CD.
- Runtime `.env` must not include `STAGING_*`/`PROD_*` keys.
- `DEPLOY_TARGET` is optional and used for runtime policy decisions.
- `POSTMARK_INBOUND_HOOK_URL` should point to the VM public endpoint; ngrok is local-only and should not run on staging/production VMs.

## 2) Expected Config Selection

- Staging:
  - `GATEWAY_CONFIG_PATH=gateway.staging.toml`
  - `EMPLOYEE_CONFIG_PATH=employee.staging.toml`
- Production:
  - `GATEWAY_CONFIG_PATH=gateway.toml`
  - `EMPLOYEE_CONFIG_PATH=employee.toml`

## 3) Common Paths

Current repo location on staging/prod VM:
- `/home/azureuser/server/DoWhiz`

Legacy helper/runtime path still seen in some local tooling:
- `/home/azureuser/server/.dowhiz/DoWhiz`

Service directory used by current CI/CD:
- `/home/azureuser/server/DoWhiz/DoWhiz_service`

If you are unsure which path a VM currently uses:

```bash
find /home/azureuser/server -maxdepth 3 -type d -name DoWhiz_service 2>/dev/null
```

Common logs:
- `DoWhiz_service/gateway.log`
- `DoWhiz_service/worker.log`
- `/tmp/ngrok.log` (if ngrok used)

PM2 logs (if PM2-managed):
- `/home/azureuser/server/.pm2/logs/dw_gateway-out.log`
- `/home/azureuser/server/.pm2/logs/dw_worker-out.log`
- `/home/azureuser/server/.pm2/logs/dw_worker-error.log`

## 4) Start / Restart Patterns

On staging/production VMs, PM2 is the canonical runtime manager. Treat any leftover `systemd`
unit such as `dowhiz-oliver.service` as legacy and disable it so it cannot compete with PM2 or
confuse incident response.

### 4.1 Script-based (foreground/local style)

```bash
cd /home/azureuser/server/DoWhiz
./DoWhiz_service/scripts/run_gateway_local.sh
./DoWhiz_service/scripts/run_employee.sh <employee_id> 9001 --skip-hook --skip-ngrok
```

Use `boiled_egg` on staging and `little_bear` on production.

### 4.2 PM2-based (recommended on VM)

```bash
cd /home/azureuser/server/DoWhiz/DoWhiz_service
set -a
source .env
set +a
if [ -s /home/azureuser/.nvm/nvm.sh ]; then
  source /home/azureuser/.nvm/nvm.sh
fi

export PM2_APP_DIR="$PWD"
export PM2_KILL_TIMEOUT_MS="${PM2_KILL_TIMEOUT_MS:-300000}"
export PM2_LISTEN_TIMEOUT_MS="${PM2_LISTEN_TIMEOUT_MS:-15000}"
pm2 startOrRestart ./ecosystem.config.cjs --only dw_worker,dw_gateway --update-env

pm2 save
pm2 list
```

If a legacy worker unit exists on a VM, disable it once:

```bash
sudo systemctl disable --now dowhiz-oliver.service || true
```

## 5) Health Checks

```bash
curl -sS http://127.0.0.1:9100/health
curl -sS http://127.0.0.1:9001/health
```

Queue/config sanity:

```bash
cd /home/azureuser/server/DoWhiz/DoWhiz_service
grep -E '^(INGESTION_QUEUE_BACKEND|SERVICE_BUS_CONNECTION_STRING|SERVICE_BUS_NAMESPACE|SERVICE_BUS_POLICY_NAME|SERVICE_BUS_POLICY_KEY|SERVICE_BUS_QUEUE_NAME|GATEWAY_CONFIG_PATH|EMPLOYEE_CONFIG_PATH|RUN_TASK_EXECUTION_BACKEND|DEPLOY_TARGET)=' .env
```

Process sanity:

```bash
pgrep -af inbound_gateway
pgrep -af rust_service
pm2 list
```

## 6) Azure ACI Prerequisite (Worker)

When `RUN_TASK_EXECUTION_BACKEND=azure_aci`, the worker requires Azure Files mount at `RUN_TASK_AZURE_ACI_HOST_SHARE_ROOT`.

Check/mount helper:

```bash
cd /home/azureuser/server/DoWhiz/DoWhiz_service
./scripts/ensure_aci_share_mount.sh
```

If the mount is missing, worker startup should fail fast.

## 7) Live Email E2E Notes

```bash
cd /home/azureuser/server/DoWhiz/DoWhiz_service
RUN_CODEX_E2E=1 POSTMARK_LIVE_TEST=1 cargo test -p scheduler_module --test service_real_email -- --nocapture
```

If SMTP 25 is blocked by cloud policy, set:
- `POSTMARK_SMTP_PORT=2525`

`service_real_email` binds `9100` and `9001`. On staging/prod, stop `dw_gateway` and `dw_worker`
first, then restart them after the test.

## 8) Historical RunTask Debugging

Full runbook:
- `DoWhiz_service/docs/task_debug_archives.md`

Quick checks:

```bash
cd /home/azureuser/server/DoWhiz/DoWhiz_service
set -a
source .env
set +a

mongosh "$MONGODB_URI" --quiet <<'MONGO'
db = db.getSiblingDB(process.env.MONGODB_DATABASE);
print(EJSON.stringify(
  db.task_debug_archives.find().sort({ created_at: -1 }).limit(5).toArray(),
  null,
  2
));
MONGO
```

Look at:
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

If `status=uploaded`, use the matching Azure auth path to download the zip.

If `status=upload_failed` or `local_only`, inspect `local_fallback_path`. If that path is gone or
was created under a temporary E2E workspace, fix Azure archive auth before treating the archive as
durable.

For active or stuck runs:

```bash
pm2 logs dw_worker --lines 200
pm2 logs dw_gateway --lines 200
pgrep -af rust_service
pgrep -af inbound_gateway
```

If the live ACI container has already been deleted, the archive zip becomes the source of truth.
Check `.run_task_trace/aci/container_show.json` and `.run_task_trace/aci/container_logs.txt` inside
the downloaded bundle.

## 9) Common Failure Patterns

1. Gateway startup error about backend
- Cause: `INGESTION_QUEUE_BACKEND` is not `servicebus`.
- Fix: set queue backend + Service Bus credentials.

2. Messages enqueued but worker idle
- Cause: queue mismatch or wrong `EMPLOYEE_ID` routing target.
- Fix: align queue/env and route targets.

3. Worker run_task fails immediately in staging/prod
- Cause: local backend selected while target policy forbids local execution.
- Fix: configure Azure ACI backend vars or adjust dev target for local environments.

4. Raw payload fetch/store failures
- Cause: storage backend credentials incomplete.
- Fix: verify selected backend and full credential set.

## 10) Rollback

Operational rollback (same code, restart services):

```bash
cd /home/azureuser/server/DoWhiz/DoWhiz_service
export PM2_APP_DIR="$PWD"
pm2 startOrRestart ./ecosystem.config.cjs --only dw_worker,dw_gateway --update-env
```

Code rollback:
1. Checkout previous known-good commit on target branch.
2. Redeploy binaries and `.env` via CI/CD workflow.
3. Re-run health checks.
