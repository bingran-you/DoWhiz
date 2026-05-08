#!/bin/bash
# Disk watchdog - runs via cron, cleans up when disk usage exceeds threshold
# Usage: Add to crontab: */15 * * * * /path/to/disk_watchdog.sh >> /var/log/disk_watchdog.log 2>&1

set -euo pipefail

THRESHOLD=95  # Trigger cleanup at 95% usage
LOG_PREFIX="[disk_watchdog $(date '+%Y-%m-%d %H:%M:%S')]"

# Get current disk usage percentage for root filesystem
USAGE=$(df / | awk 'NR==2 {gsub(/%/,""); print $5}')

echo "$LOG_PREFIX Current disk usage: ${USAGE}%"

if [ "$USAGE" -lt "$THRESHOLD" ]; then
    echo "$LOG_PREFIX Below threshold ($THRESHOLD%), no cleanup needed"
    exit 0
fi

echo "$LOG_PREFIX Usage ${USAGE}% exceeds threshold ${THRESHOLD}%, starting cleanup..."

# 1. Journal logs - vacuum to 100MB
if command -v journalctl &> /dev/null; then
    echo "$LOG_PREFIX Vacuuming journal logs..."
    sudo journalctl --vacuum-size=100M 2>/dev/null || true
fi

# 2. Azure CLI telemetry logs
if [ -d "$HOME/.azure/logs" ]; then
    echo "$LOG_PREFIX Cleaning Azure CLI logs..."
    rm -f "$HOME/.azure/logs/telemetry.log."* 2>/dev/null || true
    rm -f "$HOME/.azure/logs/az.log."* 2>/dev/null || true
fi

# 3. PM2 rotated logs (keep current, delete rotated older than 1 day)
if [ -d "$HOME/.pm2/logs" ]; then
    echo "$LOG_PREFIX Cleaning PM2 rotated logs..."
    find "$HOME/.pm2/logs" -name "*__*.log" -mtime +1 -delete 2>/dev/null || true
fi

# 4. Server rotated logs (older than 3 days)
SERVER_LOGS="$HOME/server/logs"
if [ -d "$SERVER_LOGS" ]; then
    echo "$LOG_PREFIX Cleaning server rotated logs..."
    find "$SERVER_LOGS" -name "*__*.log" -mtime +3 -delete 2>/dev/null || true
fi

# 5. Truncate large Caddy access log (>100MB)
CADDY_LOG="/var/log/caddy/api_access.log"
if [ -f "$CADDY_LOG" ]; then
    # Use stat to get size in bytes (faster than du)
    SIZE_BYTES=$(stat -c%s "$CADDY_LOG" 2>/dev/null || echo 0)
    SIZE_MB=$((SIZE_BYTES / 1024 / 1024))
    if [ "$SIZE_MB" -gt 100 ]; then
        echo "$LOG_PREFIX Truncating Caddy access log (${SIZE_MB}MB)..."
        sudo truncate -s 0 "$CADDY_LOG" 2>/dev/null || true
    fi
fi

# 6. Debug archive staging directories (older than 1 day)
if [ -d "$HOME/server" ]; then
    echo "$LOG_PREFIX Cleaning stale debug archive staging directories..."
    find "$HOME/server" -type d -name ".task_debug_archives_staging" -prune \
        -exec find {} -mindepth 1 -maxdepth 1 -type d -mtime +1 -exec rm -rf {} + \; \
        2>/dev/null || true
fi

# 7. AzCopy job plans/logs older than 2 days
if [ -d "$HOME/.azcopy" ]; then
    echo "$LOG_PREFIX Cleaning stale AzCopy artifacts..."
    find "$HOME/.azcopy" -mindepth 1 -mtime +2 -delete 2>/dev/null || true
fi

# 8. Old deploy snapshots older than 7 days
if [ -d "$HOME/server/DoWhiz" ]; then
    echo "$LOG_PREFIX Cleaning old deploy snapshots..."
    find "$HOME/server/DoWhiz" -maxdepth 1 -type d -name '.deploy_*' -mtime +7 -exec rm -rf {} + 2>/dev/null || true
fi

# Final status
NEW_USAGE=$(df / | awk 'NR==2 {gsub(/%/,""); print $5}')
echo "$LOG_PREFIX Cleanup complete. Disk usage: ${USAGE}% -> ${NEW_USAGE}%"

# Alert if still critical
if [ "$NEW_USAGE" -ge "$THRESHOLD" ]; then
    echo "$LOG_PREFIX WARNING: Disk still critical at ${NEW_USAGE}%! Manual intervention needed."
fi
