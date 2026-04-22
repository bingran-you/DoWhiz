# ACI Container Recovery

## Key Finding

**CI/CD doesn't clean up ACI containers**, so a new worker process can recover the tasks from the previous worker process if it knows the `workspace_path` that each ACI is linked to.

1. MongoDB collection stores `{container_name, workspace_path, resource_group}`
2. On recovery, read workspace for everything else

## Workspace Recovery Data

| Need | Source in Workspace |
|------|---------------------|
| Channel | `.aci_recovery_context.json` (written at ACI creation) |
| reply_to | `.aci_recovery_context.json` |
| thread_epoch | `.aci_recovery_context.json` |
| Reply content | `reply_email_draft.html` or `reply_message.txt` |

## Architecture

### ACI → Workspace Store (`aci_container_store.rs`)

MongoDB collection `aci_containers` tracks:
- `container_name`: Azure container name (e.g., `dwz-codex-123-456-0`)
- `workspace_path`: Path to workspace on Azure file share
- `resource_group`: Azure resource group
- `created_at`: Timestamp

Key functions:
- `register_aci_container_mongo()` - Called when creating ACI
- `deregister_aci_container_mongo()` - Called when deleting ACI
- `list_aci_containers()` - Returns all registered containers

### Recovery Context (`AciRecoveryContext`)

Written to `.aci_recovery_context.json` in workspace at ACI creation time:

```rust
pub struct AciRecoveryContext {
    pub channel: String,
    pub reply_to: Vec<String>,
    pub thread_epoch: Option<u64>,
}
```

Functions:
- `write_aci_recovery_context()` - Called in `codex.rs` when registering ACI
- `read_aci_recovery_context()` - Called during recovery to get outbound params

### ACI Status Polling (`codex.rs`)

```rust
pub enum AciContainerStatus {
    Running,
    Terminal(String),  // Succeeded, Failed, Terminated, Stopped
    NotFound,
    Error(String),
}
```

Functions:
- `query_aci_container_status()` - Check current state via `az container show`
- `poll_aci_container_until_terminal()` - Wait until terminal with timeout
- `delete_aci_container_by_name()` - Delete via `az container delete`

## Recovery Flow

### Normal Task Lifecycle

1. Create ACI container
2. Register in MongoDB
3. Write `.aci_recovery_context.json` to workspace
4. Poll until terminal
5. Read results from workspace
6. Send to outbound
7. Write `.outbound_sent` marker file
8. Delete ACI container from Azure
9. Deregister from MongoDB

### Recovery on Worker Startup

1. Query MongoDB for all registered containers (orphaned)
2. For each container:
   - Check if `.outbound_sent` marker exists in workspace
   - If marker exists → skip outbound, just cleanup
   - If marker doesn't exist → poll if needed, send outbound, write marker, cleanup

### Why Marker File?

There is a small but possible window where worker restarts after sending to outbound but before deregistration. The marker file prevents duplicate sends to the user.

## Recovery Module (`aci_recovery.rs`)

Entry point: `recover_orphaned_aci_containers()`

Called on worker startup when `ACI_RECOVERY_ENABLED=1`:

```rust
// In server.rs
if std::env::var("ACI_RECOVERY_ENABLED").ok().as_deref() == Some("1") {
    info!("ACI recovery enabled, checking for orphaned containers");
    task::spawn_blocking(crate::aci_recovery::recover_orphaned_aci_containers);
}
```

### Outbound Propagation

Recovery builds `SendReplyTask` from:
- Channel, reply_to, thread_epoch from `.aci_recovery_context.json`
- Reply content from `reply_email_draft.html` or `reply_message.txt`

Dispatches to appropriate adapter:
- Email → `execute_email_send()`
- Slack → `execute_slack_send()`
- Discord → `execute_discord_send()`
- Telegram → `execute_telegram_send()`
- SMS → `execute_sms_send()`
- BlueBubbles → `execute_bluebubbles_send()`
- WhatsApp → `execute_whatsapp_send()`
- WeChat → `execute_wechat_send()`
- WeChatMp → `execute_wechat_mp_send()`
- Lark → `execute_lark_send()`
- GoogleDocs/Sheets/Slides → `execute_google_docs_send()`
- Notion → `execute_notion_send()`
- Zoom → Skip (no direct reply)

## Design Decision: Recovery Context File

Initially considered deriving channel from workspace metadata files:
- `postmark_payload.json` = email
- `*_discord_meta.json` = discord
- etc.

**Problem**: Fragile - requires guessing from multiple possible file patterns.

**Solution**: Write `.aci_recovery_context.json` at ACI creation time with explicit channel, reply_to, and thread_epoch. Recovery fails cleanly if file missing rather than potentially sending to wrong recipient.

## Files Modified

- `run_task_module/src/run_task/aci_container_store.rs` - MongoDB tracking + recovery context
- `run_task_module/src/run_task/codex.rs` - ACI status polling, writes recovery context at registration
- `run_task_module/src/run_task/mod.rs` - Exports
- `scheduler_module/src/aci_recovery.rs` - Recovery logic
- `scheduler_module/src/scheduler/mod.rs` - Made `outbound` module `pub(crate)`
- `scheduler_module/src/service/server.rs` - Recovery hook on startup

## Enabling Recovery

Set environment variable:
```bash
ACI_RECOVERY_ENABLED=1
```

Recovery runs asynchronously on worker startup via `task::spawn_blocking`.
