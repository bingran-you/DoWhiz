#!/bin/bash
set -euo pipefail

# Ensure standard paths are available (login shell may reset PATH)
export PATH="/app/bin:/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin:$PATH"

# Warm pool worker script.
# Polls Azure Queue for tasks, processes them, and signals completion.
#
# Required env vars:
#   TASK_QUEUE_NAME       - Queue to poll for tasks
#   COMPLETION_QUEUE_NAME - Queue to signal completion
#   QUEUE_STORAGE_ACCOUNT - Azure Storage account name
#   QUEUE_STORAGE_KEY     - Azure Storage account key
#
# Optional env vars:
#   POLL_INTERVAL         - Seconds between polls (default: 2)
#   WORKSPACE_LOCAL_DIR   - Local workspace directory (default: /app/.workspace/task)

TASK_QUEUE="${TASK_QUEUE_NAME:?TASK_QUEUE_NAME not set}"
COMPLETION_QUEUE="${COMPLETION_QUEUE_NAME:?COMPLETION_QUEUE_NAME not set}"
STORAGE_ACCOUNT="${QUEUE_STORAGE_ACCOUNT:?QUEUE_STORAGE_ACCOUNT not set}"
STORAGE_KEY="${QUEUE_STORAGE_KEY:?QUEUE_STORAGE_KEY not set}"
POLL_INTERVAL="${POLL_INTERVAL:-2}"
export WORKSPACE_LOCAL_DIR="${WORKSPACE_LOCAL_DIR:-/app/.workspace/task}"

echo "[warm_worker] Starting, polling queue: $TASK_QUEUE"
echo "[warm_worker] Storage account: $STORAGE_ACCOUNT"
echo "[warm_worker] Poll interval: $POLL_INTERVAL seconds"

POLL_COUNT=0
while true; do
    POLL_COUNT=$((POLL_COUNT + 1))

    # Peek at queue to see what's available
    QUEUE_PEEK=$(az storage message peek \
        --queue-name "$TASK_QUEUE" \
        --account-name "$STORAGE_ACCOUNT" \
        --account-key "$STORAGE_KEY" \
        --num-messages 10 \
        --output json 2>/dev/null || echo "[]")
    QUEUE_COUNT=$(echo "$QUEUE_PEEK" | jq 'length')
    echo "[warm_worker] Poll #$POLL_COUNT - queue has $QUEUE_COUNT message(s)"
    if [ "$QUEUE_COUNT" -gt 0 ]; then
        echo "[warm_worker] Queue contents: $QUEUE_PEEK"
    fi

    # Get message from queue (capture stdout and stderr separately)
    AZ_STDOUT=$(az storage message get \
        --queue-name "$TASK_QUEUE" \
        --account-name "$STORAGE_ACCOUNT" \
        --account-key "$STORAGE_KEY" \
        --output json 2>/dev/null)
    AZ_EXIT_CODE=$?

    if [ $AZ_EXIT_CODE -ne 0 ]; then
        echo "[warm_worker] az command failed with exit code $AZ_EXIT_CODE" >&2
        sleep "$POLL_INTERVAL"
        continue
    fi

    # Parse the JSON response
    MSG=$(echo "$AZ_STDOUT" | jq -r '.[0] // empty' 2>/dev/null || echo "")

    if [ -n "$MSG" ]; then
        echo "[warm_worker] Poll #$POLL_COUNT - MESSAGE FOUND"
        MESSAGE_ID=$(echo "$MSG" | jq -r '.id')
        POP_RECEIPT=$(echo "$MSG" | jq -r '.popReceipt')
        CONTENT=$(echo "$MSG" | jq -r '.content' | base64 -d)
        echo "[warm_worker] Message ID: $MESSAGE_ID"

        TASK_ID=$(echo "$CONTENT" | jq -r '.task_id')
        echo "[warm_worker] Received task: $TASK_ID"

        # True dequeue: delete immediately to avoid visibility timeout issues
        az storage message delete \
            --queue-name "$TASK_QUEUE" \
            --account-name "$STORAGE_ACCOUNT" \
            --account-key "$STORAGE_KEY" \
            --id "$MESSAGE_ID" \
            --pop-receipt "$POP_RECEIPT" \
            --output none

        # Extract task info and export for workspace_sync.sh
        export WORKSPACE_SHARE_URL=$(echo "$CONTENT" | jq -r '.share_url')
        export WORKSPACE_SAS_TOKEN=$(echo "$CONTENT" | jq -r '.sas_token')
        AGENT_COMMAND=$(echo "$CONTENT" | jq -r '.agent_command')

        # Process task
        echo "[warm_worker] Downloading workspace..."
        workspace_sync.sh download

        echo "[warm_worker] Running agent..."
        AGENT_EXIT_CODE=0
        eval "$AGENT_COMMAND" || AGENT_EXIT_CODE=$?
        echo "[warm_worker] Agent exited with code: $AGENT_EXIT_CODE"

        echo "[warm_worker] Uploading results..."
        workspace_sync.sh upload

        # Signal completion (scheduler handles retry logic based on exit_code)
        COMPLETION_MSG=$(jq -n \
            --arg tid "$TASK_ID" \
            --arg cname "${CONTAINER_NAME:-unknown}" \
            --argjson code "$AGENT_EXIT_CODE" \
            '{task_id: $tid, container_name: $cname, exit_code: $code}' | base64 -w0)

        az storage message put \
            --queue-name "$COMPLETION_QUEUE" \
            --account-name "$STORAGE_ACCOUNT" \
            --account-key "$STORAGE_KEY" \
            --content "$COMPLETION_MSG" \
            --output none

        echo "[warm_worker] Task $TASK_ID complete, exiting"
        exit "$AGENT_EXIT_CODE"
    else
        # No message - log occasionally to show we're alive
        if [ $POLL_COUNT -le 3 ] || [ $((POLL_COUNT % 30)) -eq 0 ]; then
            echo "[warm_worker] Poll #$POLL_COUNT - no messages, waiting..."
        fi
    fi

    sleep "$POLL_INTERVAL"
done
