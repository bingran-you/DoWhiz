# DoWhiz Production Server Commands

Quick reference for common operations on `dowhizprod1`.

## PM2 Service Logs

```bash
# Worker logs (task execution)
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 logs dw_worker --lines 100 --nostream'

# Gateway logs (API)
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 logs dw_gateway --lines 100 --nostream'

# Live streaming (follow mode)
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 logs dw_worker'
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 logs dw_gateway'

# Both services at once
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 logs'

# Error logs only
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 logs dw_worker --err --lines 50'

# List all PM2 processes
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 list'

# Restart services
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 restart dw_worker'
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 restart dw_gateway'
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 restart all'

# Reload (zero-downtime)
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 reload dw_worker'
```

## Workspace Paths

```bash
# Base workspace directory
/home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/

# User workspaces
/home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/<user_id>/workspaces/

# List workspaces for a user
ssh dowhizprod1 'ls -la /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/<user_id>/workspaces/'

# Check Codex config in a workspace
ssh dowhizprod1 'cat /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/<user_id>/workspaces/<workspace>/.codex/config.toml'

# Check task trace metadata
ssh dowhizprod1 'cat /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/<user_id>/workspaces/<workspace>/.task_trace.json'

# Check Codex exit code (0 = success)
ssh dowhizprod1 'cat /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/<user_id>/workspaces/<workspace>/.codex_remote_exit_code'

# View full Codex output log
ssh dowhizprod1 'cat /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/<user_id>/workspaces/<workspace>/.codex_remote_output.log'

# Tail last 100 lines of Codex output
ssh dowhizprod1 'tail -100 /home/azureuser/server/.dowhiz/DoWhiz/run_task/little_bear/users/<user_id>/workspaces/<workspace>/.codex_remote_output.log'
```

## Azure Container Instances (ACI)

```bash
# List all Codex containers
ssh dowhizprod1 'az container list --resource-group rg-dowhiz-oliver-dev --query "[?starts_with(name, '"'"'dwz-codex'"'"')].{name:name, state:instanceView.state, created:containers[0].instanceView.currentState.startTime}" -o table'

# List containers with provisioning state
ssh dowhizprod1 'az container list --resource-group rg-dowhiz-oliver-dev --query "[?starts_with(name, '"'"'dwz-codex'"'"')].{name:name, provisioningState:provisioningState}" -o table'

# Get container details
ssh dowhizprod1 'az container show --resource-group rg-dowhiz-oliver-dev --name <container-name>'

# Delete a specific container
ssh dowhizprod1 'az container delete --resource-group rg-dowhiz-oliver-dev --name <container-name> --yes'

# Delete all Codex containers (bulk cleanup)
ssh dowhizprod1 'az container list --resource-group rg-dowhiz-oliver-dev --query "[?starts_with(name, '"'"'dwz-codex'"'"')].name" -o tsv | xargs -I {} az container delete --resource-group rg-dowhiz-oliver-dev --name {} --yes'
```

Note: `az container logs` typically doesn't work reliably for these containers. Check workspace output files or PM2 worker logs instead.

## MongoDB / CosmosDB

CosmosDB requires pymongo (mongosh doesn't work reliably). Use Python heredocs:

```bash
# Count running executions
ssh dowhizprod1 'cd /home/azureuser/server/DoWhiz && python3 << "PYEOF"
from pymongo import MongoClient
conn_str = db_name = None
with open(".env") as f:
    for line in f:
        if line.startswith("MONGODB_URI="): conn_str = line.split("=",1)[1].strip().strip(chr(34))
        if line.startswith("MONGODB_DATABASE="): db_name = line.split("=",1)[1].strip().strip(chr(34))
db = MongoClient(conn_str)[db_name]
print("Running executions:", db.task_executions.count_documents({"status": "running"}))
PYEOF'

# List running executions
ssh dowhizprod1 'cd /home/azureuser/server/DoWhiz && python3 << "PYEOF"
from pymongo import MongoClient
conn_str = db_name = None
with open(".env") as f:
    for line in f:
        if line.startswith("MONGODB_URI="): conn_str = line.split("=",1)[1].strip().strip(chr(34))
        if line.startswith("MONGODB_DATABASE="): db_name = line.split("=",1)[1].strip().strip(chr(34))
db = MongoClient(conn_str)[db_name]
for ex in db.task_executions.find({"status": "running"}).limit(10):
    print(f"task_id: {ex.get('task_id')}, created: {ex.get('created_at')}")
PYEOF'

# Query tasks for a specific account (replace <account_id>)
ssh dowhizprod1 'cd /home/azureuser/server/DoWhiz && python3 << "PYEOF"
from pymongo import MongoClient
account_id = "<account_id>"
conn_str = db_name = None
with open(".env") as f:
    for line in f:
        if line.startswith("MONGODB_URI="): conn_str = line.split("=",1)[1].strip().strip(chr(34))
        if line.startswith("MONGODB_DATABASE="): db_name = line.split("=",1)[1].strip().strip(chr(34))
db = MongoClient(conn_str)[db_name]
for task in db.tasks.find({"task_json": {"$regex": account_id}}):
    print(f"_id: {task.get('_id')}, status: {task.get('status')}")
PYEOF'

# Mark all running executions as failed
ssh dowhizprod1 'cd /home/azureuser/server/DoWhiz && python3 << "PYEOF"
from pymongo import MongoClient
from datetime import datetime
conn_str = db_name = None
with open(".env") as f:
    for line in f:
        if line.startswith("MONGODB_URI="): conn_str = line.split("=",1)[1].strip().strip(chr(34))
        if line.startswith("MONGODB_DATABASE="): db_name = line.split("=",1)[1].strip().strip(chr(34))
db = MongoClient(conn_str)[db_name]
result = db.task_executions.update_many({"status": "running"}, {"$set": {"status": "failed", "updated_at": datetime.utcnow()}})
print(f"Modified {result.modified_count} executions")
PYEOF'

# List collections
ssh dowhizprod1 'cd /home/azureuser/server/DoWhiz && python3 << "PYEOF"
from pymongo import MongoClient
conn_str = db_name = None
with open(".env") as f:
    for line in f:
        if line.startswith("MONGODB_URI="): conn_str = line.split("=",1)[1].strip().strip(chr(34))
        if line.startswith("MONGODB_DATABASE="): db_name = line.split("=",1)[1].strip().strip(chr(34))
db = MongoClient(conn_str)[db_name]
for name in db.list_collection_names():
    print(f"{name}: {db[name].count_documents({})} docs")
PYEOF'
```

## Cleanup Scripts

```bash
# Cleanup all tasks for an account (dry run first!)
ssh dowhizprod1 'cd /home/azureuser/server/DoWhiz/cleanup && python3 cleanup_tasks_by_account.py <account_id> --dry-run'

# Execute cleanup
ssh dowhizprod1 'cd /home/azureuser/server/DoWhiz/cleanup && python3 cleanup_tasks_by_account.py <account_id> --execute'
```

## TPM Sync Triggers

```bash
# Trigger TPM sync for an account
ssh dowhizprod1 'curl -s -X POST "http://localhost:9001/tpm/sync?account_id=<account_id>"'

# Check TPM status
ssh dowhizprod1 'curl -s "http://localhost:9001/tpm/status?account_id=<account_id>"'
```

## Ephemeral Shares / azcopy

```bash
# Check azcopy is installed
ssh dowhizprod1 'which azcopy && azcopy --version'

# List Azure file shares
ssh dowhizprod1 'az storage share list --account-name <storage-account> --query "[].name" -o tsv'

# Check ephemeral share exists
ssh dowhizprod1 'az storage share exists --account-name dwhzoliverdev --name <share-name>'
```

## Debugging

```bash
# Check disk usage
ssh dowhizprod1 'df -h'

# Check memory
ssh dowhizprod1 'free -h'

# Check running processes
ssh dowhizprod1 'ps aux | grep -E "(node|codex|claude)"'

# Tail system logs
ssh dowhizprod1 'tail -f /var/log/syslog'

# Check environment variables loaded
ssh dowhizprod1 'source ~/.nvm/nvm.sh && pm2 env dw_worker | grep -E "(MONGODB|AZURE|OPENAI)"'
```

## Quick Troubleshooting

### Task stuck in "running"
1. Check MongoDB for stuck executions
2. Mark as failed if container is gone
3. Re-trigger the task

### Codex 404 errors
1. Check `.codex/config.toml` in workspace for correct model
2. Verify model matches runner (codex -> gpt-*, claude -> claude-*)
3. Test Azure OpenAI endpoint with curl

### Container not starting
1. Check ACI provisioning state
2. Check workspace output files for errors
3. Verify ephemeral share was created (azcopy installed?)
