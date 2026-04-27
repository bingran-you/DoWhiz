use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

use chrono::Utc;
use mongodb::bson::{doc, DateTime as BsonDateTime, Document};
use mongodb::options::ClientOptions;
use mongodb::sync::{Client, Collection, Database};

const COLLECTION_NAME: &str = "aci_containers";

#[derive(Debug, Clone)]
pub struct AciContainerRecord {
    pub container_name: String,
    pub workspace_path: PathBuf,
    pub resource_group: String,
    pub created_at: chrono::DateTime<Utc>,
}

static SHARED_CLIENT: OnceLock<Option<Client>> = OnceLock::new();

fn get_client() -> Option<&'static Client> {
    SHARED_CLIENT
        .get_or_init(|| {
            let uri = env::var("MONGODB_URI")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())?;

            let mut options = ClientOptions::parse(&uri).ok()?;
            options.app_name = Some("RunTaskAciStore".to_string());
            Client::with_options(options).ok()
        })
        .as_ref()
}

fn get_database() -> Option<Database> {
    let client = get_client()?;

    let db_name = env::var("MONGODB_DATABASE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            let target = env::var("DEPLOY_TARGET")
                .ok()
                .map(|v| v.trim().to_ascii_lowercase())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "production".to_string());
            let employee = env::var("EMPLOYEE_ID")
                .ok()
                .map(|v| sanitize_fragment(&v))
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "default".to_string());
            format!("dowhiz_{}_{}", sanitize_fragment(&target), employee)
        });

    Some(client.database(&db_name))
}

fn collection() -> Option<Collection<Document>> {
    Some(get_database()?.collection::<Document>(COLLECTION_NAME))
}

fn sanitize_fragment(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut last_was_underscore = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
            last_was_underscore = false;
        } else if !last_was_underscore {
            result.push('_');
            last_was_underscore = true;
        }
    }
    result.trim_matches('_').to_string()
}

/// Recovery context written to workspace for ACI recovery after worker crash.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AciRecoveryContext {
    pub channel: String,
    pub reply_to: Vec<String>,
    pub thread_epoch: Option<u64>,
}

const ACI_RECOVERY_CONTEXT_FILE: &str = ".aci_recovery_context.json";

/// Write ACI recovery context to workspace.
/// Called when creating a new ACI container so recovery can read it.
pub fn write_aci_recovery_context(
    workspace_path: &std::path::Path,
    channel: &str,
    reply_to: &[String],
    thread_epoch: Option<u64>,
) {
    let context = AciRecoveryContext {
        channel: channel.to_string(),
        reply_to: reply_to.to_vec(),
        thread_epoch,
    };

    let path = workspace_path.join(ACI_RECOVERY_CONTEXT_FILE);
    match serde_json::to_string_pretty(&context) {
        Ok(json) => {
            if let Err(err) = std::fs::write(&path, json) {
                tracing::warn!(
                    "failed to write ACI recovery context to {}: {}",
                    path.display(),
                    err
                );
            }
        }
        Err(err) => {
            tracing::warn!("failed to serialize ACI recovery context: {}", err);
        }
    }
}

/// Read ACI recovery context from workspace.
pub fn read_aci_recovery_context(workspace_path: &std::path::Path) -> Option<AciRecoveryContext> {
    let path = workspace_path.join(ACI_RECOVERY_CONTEXT_FILE);
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Register an ACI container in MongoDB.
/// Called when creating a new ACI container.
/// Silently fails if MongoDB is not configured (MONGODB_URI not set).
pub fn register_aci_container_mongo(
    container_name: &str,
    workspace_path: &std::path::Path,
    resource_group: &str,
) {
    let Some(coll) = collection() else {
        return;
    };

    let now = Utc::now();
    let document = doc! {
        "container_name": container_name,
        "workspace_path": workspace_path.to_string_lossy().to_string(),
        "resource_group": resource_group,
        "created_at": BsonDateTime::from_chrono(now),
    };

    match coll.insert_one(document, None) {
        Ok(_) => {
            tracing::info!(
                "registered ACI container in MongoDB: container={} workspace={}",
                container_name,
                workspace_path.display()
            );
        }
        Err(err) => {
            tracing::warn!(
                "failed to register ACI container in MongoDB: container={} error={}",
                container_name,
                err
            );
        }
    }
}

/// Deregister an ACI container from MongoDB.
/// Called when deleting an ACI container.
/// Silently fails if MongoDB is not configured.
pub fn deregister_aci_container_mongo(container_name: &str) {
    let Some(coll) = collection() else {
        return;
    };

    let filter = doc! { "container_name": container_name };

    match coll.delete_one(filter, None) {
        Ok(_) => {
            tracing::info!(
                "deregistered ACI container from MongoDB: container={}",
                container_name
            );
        }
        Err(err) => {
            tracing::warn!(
                "failed to deregister ACI container from MongoDB: container={} error={}",
                container_name,
                err
            );
        }
    }
}

/// List all ACI container records from MongoDB.
/// Returns all containers that were registered but not yet deregistered.
/// Returns empty vec if MongoDB is not configured.
pub fn list_aci_containers() -> Vec<AciContainerRecord> {
    let Some(coll) = collection() else {
        return Vec::new();
    };

    // Note: avoid sorting by created_at as CosmosDB requires an index for it
    let cursor = match coll.find(doc! {}, None) {
        Ok(cursor) => cursor,
        Err(err) => {
            tracing::warn!("failed to list ACI containers from MongoDB: {}", err);
            return Vec::new();
        }
    };

    let mut records = Vec::new();
    for result in cursor {
        match result {
            Ok(doc) => {
                if let Some(record) = parse_container_record(&doc) {
                    records.push(record);
                }
            }
            Err(err) => {
                tracing::warn!("failed to parse ACI container record: {}", err);
            }
        }
    }

    records
}

/// Get workspace paths for all registered containers.
/// Returns (container_name, workspace_path, resource_group) tuples.
pub fn get_registered_container_workspaces() -> Vec<(String, PathBuf, String)> {
    list_aci_containers()
        .into_iter()
        .map(|r| (r.container_name, r.workspace_path, r.resource_group))
        .collect()
}

fn parse_container_record(doc: &Document) -> Option<AciContainerRecord> {
    let container_name = doc.get_str("container_name").ok()?.to_string();
    let workspace_path = PathBuf::from(doc.get_str("workspace_path").ok()?);
    let resource_group = doc.get_str("resource_group").ok()?.to_string();
    let created_at = doc
        .get_datetime("created_at")
        .ok()
        .map(|dt| dt.to_chrono())
        .unwrap_or_else(Utc::now);

    Some(AciContainerRecord {
        container_name,
        workspace_path,
        resource_group,
        created_at,
    })
}

/// Find an ACI container by workspace path.
/// Returns the container record if found, None otherwise.
pub fn find_aci_container_by_workspace(workspace_path: &str) -> Option<AciContainerRecord> {
    let coll = collection()?;

    let filter = doc! { "workspace_path": workspace_path };
    match coll.find_one(filter, None) {
        Ok(Some(doc)) => parse_container_record(&doc),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(
                "failed to find ACI container by workspace {}: {}",
                workspace_path,
                err
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_container_record_extracts_fields() {
        let doc = doc! {
            "container_name": "dwz-codex-123-456-0",
            "workspace_path": "/stg/run_task/users/user-123/workspace",
            "resource_group": "my-resource-group",
            "created_at": BsonDateTime::from_chrono(Utc::now()),
        };

        let record = parse_container_record(&doc).expect("should parse");
        assert_eq!(record.container_name, "dwz-codex-123-456-0");
        assert_eq!(
            record.workspace_path,
            PathBuf::from("/stg/run_task/users/user-123/workspace")
        );
        assert_eq!(record.resource_group, "my-resource-group");
    }

    #[test]
    fn sanitize_fragment_normalizes_input() {
        assert_eq!(sanitize_fragment("Hello World"), "hello_world");
        assert_eq!(sanitize_fragment("test--value"), "test_value");
        assert_eq!(sanitize_fragment("__leading__"), "leading");
    }
}
