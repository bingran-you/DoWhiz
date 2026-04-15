//! Dev Task Store for TPM workflows.
//!
//! Provides MongoDB-backed storage for development tasks that Oliver
//! manages as a TPM (assign to humans, track status, sync with Notion).
//!
//! Organization-agnostic: each store instance is scoped to an organization,
//! and all tasks are tagged with that organization for multi-tenant support.

use chrono::{DateTime, Utc};
use mongodb::bson::{doc, oid::ObjectId, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::FindOptions;
use mongodb::sync::Collection;
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};

use crate::mongo_store::{create_client_from_env, database_from_env, ensure_index_compatible};

// -----------------------------------------------------------------------------
// Types
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::P0 => "p0",
            Priority::P1 => "p1",
            Priority::P2 => "p2",
            Priority::P3 => "p3",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "p0" => Some(Priority::P0),
            "p1" => Some(Priority::P1),
            "p2" => Some(Priority::P2),
            "p3" => Some(Priority::P3),
            _ => None,
        }
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Backlog,
    InProgress,
    Review,
    Done,
    Blocked,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Backlog => "backlog",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Review => "review",
            TaskStatus::Done => "done",
            TaskStatus::Blocked => "blocked",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "backlog" => Some(TaskStatus::Backlog),
            "in_progress" => Some(TaskStatus::InProgress),
            "review" => Some(TaskStatus::Review),
            "done" => Some(TaskStatus::Done),
            "blocked" => Some(TaskStatus::Blocked),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    UserFeedback,
    Notetaker,
    MarketResearch,
    Manual,
}

impl TaskSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskSource::UserFeedback => "user_feedback",
            TaskSource::Notetaker => "notetaker",
            TaskSource::MarketResearch => "market_research",
            TaskSource::Manual => "manual",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "user_feedback" => Some(TaskSource::UserFeedback),
            "notetaker" => Some(TaskSource::Notetaker),
            "market_research" => Some(TaskSource::MarketResearch),
            "manual" => Some(TaskSource::Manual),
            _ => None,
        }
    }
}

/// A development task managed by Oliver TPM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevTask {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Organization this task belongs to (e.g., "deeptutor")
    pub organization: String,
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    pub source: TaskSource,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notion_page_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DevTask {
    pub fn new(
        organization: String,
        title: String,
        description: String,
        source: TaskSource,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            organization,
            title,
            description,
            priority: Priority::P2,
            status: TaskStatus::Backlog,
            assignee: None,
            source,
            tags: Vec::new(),
            notion_page_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_assignee(mut self, assignee: String) -> Self {
        self.assignee = Some(assignee);
        self
    }
}

// -----------------------------------------------------------------------------
// Error
// -----------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DevTaskStoreError {
    #[error("mongodb error: {0}")]
    Mongo(#[from] mongodb::error::Error),
    #[error("mongo config error: {0}")]
    Config(#[from] crate::mongo_store::MongoStoreError),
    #[error("bson serialization error: {0}")]
    BsonSer(#[from] mongodb::bson::ser::Error),
    #[error("bson deserialization error: {0}")]
    BsonDe(#[from] mongodb::bson::de::Error),
    #[error("task not found: {0}")]
    NotFound(String),
}

// -----------------------------------------------------------------------------
// Store
// -----------------------------------------------------------------------------

/// Collection name for dev tasks (shared across all organizations).
const COLLECTION_NAME: &str = "dev_tasks";

pub struct DevTaskStore {
    tasks: Collection<Document>,
    organization: String,
}

impl DevTaskStore {
    /// Create a new DevTaskStore scoped to an organization.
    pub fn new(organization: &str) -> Result<Self, DevTaskStoreError> {
        let client = create_client_from_env()?;
        let db = database_from_env(&client);
        let tasks = db.collection::<Document>(COLLECTION_NAME);

        // Ensure indexes (organization is the primary filter for all queries)
        // Note: Cosmos DB requires the sort field to be included in the index
        ensure_index_compatible(
            &tasks,
            IndexModel::builder()
                .keys(doc! { "organization": 1, "status": 1, "priority": 1 })
                .build(),
        )?;
        ensure_index_compatible(
            &tasks,
            IndexModel::builder()
                .keys(doc! { "organization": 1, "assignee": 1, "priority": 1 })
                .build(),
        )?;
        ensure_index_compatible(
            &tasks,
            IndexModel::builder()
                .keys(doc! { "organization": 1, "notion_page_id": 1 })
                .build(),
        )?;
        ensure_index_compatible(
            &tasks,
            IndexModel::builder()
                .keys(doc! { "organization": 1, "created_at": -1 })
                .build(),
        )?;

        Ok(Self {
            tasks,
            organization: organization.to_string(),
        })
    }

    /// Insert a new task.
    pub fn insert_task(&self, task: &DevTask) -> Result<ObjectId, DevTaskStoreError> {
        let doc = task_to_document(task);
        let result = self.tasks.insert_one(doc, None)?;
        match result.inserted_id {
            Bson::ObjectId(id) => Ok(id),
            _ => Err(DevTaskStoreError::NotFound(
                "failed to get inserted id".into(),
            )),
        }
    }

    /// Get a task by ID (scoped to this organization).
    pub fn get_task(&self, task_id: &ObjectId) -> Result<Option<DevTask>, DevTaskStoreError> {
        let filter = doc! {
            "_id": task_id,
            "organization": &self.organization,
        };
        let doc = self.tasks.find_one(filter, None)?;
        match doc {
            Some(d) => Ok(Some(document_to_task(d)?)),
            None => Ok(None),
        }
    }

    /// Get a task by Notion page ID (scoped to this organization).
    pub fn get_task_by_notion_page(
        &self,
        notion_page_id: &str,
    ) -> Result<Option<DevTask>, DevTaskStoreError> {
        let filter = doc! {
            "organization": &self.organization,
            "notion_page_id": notion_page_id,
        };
        let doc = self.tasks.find_one(filter, None)?;
        match doc {
            Some(d) => Ok(Some(document_to_task(d)?)),
            None => Ok(None),
        }
    }

    /// List tasks by status (sorted by priority).
    pub fn list_tasks_by_status(
        &self,
        status: TaskStatus,
    ) -> Result<Vec<DevTask>, DevTaskStoreError> {
        let filter = doc! {
            "organization": &self.organization,
            "status": status.as_str(),
        };
        let options = FindOptions::builder().sort(doc! { "priority": 1 }).build();
        let cursor = self.tasks.find(filter, options)?;
        let mut tasks = Vec::new();
        for doc in cursor {
            tasks.push(document_to_task(doc?)?);
        }
        Ok(tasks)
    }

    /// List tasks assigned to a developer (sorted by priority).
    pub fn list_tasks_by_assignee(
        &self,
        assignee: &str,
    ) -> Result<Vec<DevTask>, DevTaskStoreError> {
        let filter = doc! {
            "organization": &self.organization,
            "assignee": assignee,
        };
        let options = FindOptions::builder().sort(doc! { "priority": 1 }).build();
        let cursor = self.tasks.find(filter, options)?;
        let mut tasks = Vec::new();
        for doc in cursor {
            tasks.push(document_to_task(doc?)?);
        }
        Ok(tasks)
    }

    /// List all tasks for this organization (sorted by created_at descending).
    pub fn list_all_tasks(&self) -> Result<Vec<DevTask>, DevTaskStoreError> {
        let filter = doc! { "organization": &self.organization };
        let options = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();
        let cursor = self.tasks.find(filter, options)?;
        let mut tasks = Vec::new();
        for doc in cursor {
            tasks.push(document_to_task(doc?)?);
        }
        Ok(tasks)
    }

    /// Update task status.
    pub fn update_status(
        &self,
        task_id: &ObjectId,
        status: TaskStatus,
    ) -> Result<(), DevTaskStoreError> {
        let filter = doc! {
            "_id": task_id,
            "organization": &self.organization,
        };
        let update = doc! {
            "$set": {
                "status": status.as_str(),
                "updated_at": BsonDateTime::from_chrono(Utc::now()),
            }
        };
        let result = self.tasks.update_one(filter, update, None)?;
        if result.matched_count == 0 {
            return Err(DevTaskStoreError::NotFound(task_id.to_string()));
        }
        Ok(())
    }

    /// Update task assignee.
    pub fn update_assignee(
        &self,
        task_id: &ObjectId,
        assignee: Option<&str>,
    ) -> Result<(), DevTaskStoreError> {
        let filter = doc! {
            "_id": task_id,
            "organization": &self.organization,
        };
        let update = doc! {
            "$set": {
                "assignee": assignee,
                "updated_at": BsonDateTime::from_chrono(Utc::now()),
            }
        };
        let result = self.tasks.update_one(filter, update, None)?;
        if result.matched_count == 0 {
            return Err(DevTaskStoreError::NotFound(task_id.to_string()));
        }
        Ok(())
    }

    /// Update task priority.
    pub fn update_priority(
        &self,
        task_id: &ObjectId,
        priority: Priority,
    ) -> Result<(), DevTaskStoreError> {
        let filter = doc! {
            "_id": task_id,
            "organization": &self.organization,
        };
        let update = doc! {
            "$set": {
                "priority": priority.as_str(),
                "updated_at": BsonDateTime::from_chrono(Utc::now()),
            }
        };
        let result = self.tasks.update_one(filter, update, None)?;
        if result.matched_count == 0 {
            return Err(DevTaskStoreError::NotFound(task_id.to_string()));
        }
        Ok(())
    }

    /// Link task to a Notion page.
    pub fn link_notion_page(
        &self,
        task_id: &ObjectId,
        notion_page_id: &str,
    ) -> Result<(), DevTaskStoreError> {
        let filter = doc! {
            "_id": task_id,
            "organization": &self.organization,
        };
        let update = doc! {
            "$set": {
                "notion_page_id": notion_page_id,
                "updated_at": BsonDateTime::from_chrono(Utc::now()),
            }
        };
        let result = self.tasks.update_one(filter, update, None)?;
        if result.matched_count == 0 {
            return Err(DevTaskStoreError::NotFound(task_id.to_string()));
        }
        Ok(())
    }

    /// Delete a task.
    pub fn delete_task(&self, task_id: &ObjectId) -> Result<(), DevTaskStoreError> {
        let filter = doc! {
            "_id": task_id,
            "organization": &self.organization,
        };
        let result = self.tasks.delete_one(filter, None)?;
        if result.deleted_count == 0 {
            return Err(DevTaskStoreError::NotFound(task_id.to_string()));
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Document conversion helpers
// -----------------------------------------------------------------------------

fn task_to_document(task: &DevTask) -> Document {
    let mut doc = doc! {
        "organization": &task.organization,
        "title": &task.title,
        "description": &task.description,
        "priority": task.priority.as_str(),
        "status": task.status.as_str(),
        "source": task.source.as_str(),
        "tags": &task.tags,
        "created_at": BsonDateTime::from_chrono(task.created_at),
        "updated_at": BsonDateTime::from_chrono(task.updated_at),
    };
    if let Some(ref assignee) = task.assignee {
        doc.insert("assignee", assignee);
    }
    if let Some(ref notion_page_id) = task.notion_page_id {
        doc.insert("notion_page_id", notion_page_id);
    }
    doc
}

fn document_to_task(doc: Document) -> Result<DevTask, DevTaskStoreError> {
    let id = doc.get_object_id("_id").ok().map(|id| id.to_owned());

    let organization = doc
        .get_str("organization")
        .map_err(|e| DevTaskStoreError::NotFound(format!("missing organization: {e}")))?
        .to_string();

    let title = doc
        .get_str("title")
        .map_err(|e| DevTaskStoreError::NotFound(format!("missing title: {e}")))?
        .to_string();

    let description = doc
        .get_str("description")
        .map_err(|e| DevTaskStoreError::NotFound(format!("missing description: {e}")))?
        .to_string();

    let priority_str = doc
        .get_str("priority")
        .map_err(|e| DevTaskStoreError::NotFound(format!("missing priority: {e}")))?;
    let priority = Priority::from_str(priority_str)
        .ok_or_else(|| DevTaskStoreError::NotFound(format!("invalid priority: {priority_str}")))?;

    let status_str = doc
        .get_str("status")
        .map_err(|e| DevTaskStoreError::NotFound(format!("missing status: {e}")))?;
    let status = TaskStatus::from_str(status_str)
        .ok_or_else(|| DevTaskStoreError::NotFound(format!("invalid status: {status_str}")))?;

    let source_str = doc
        .get_str("source")
        .map_err(|e| DevTaskStoreError::NotFound(format!("missing source: {e}")))?;
    let source = TaskSource::from_str(source_str)
        .ok_or_else(|| DevTaskStoreError::NotFound(format!("invalid source: {source_str}")))?;

    let assignee = doc.get_str("assignee").ok().map(|s| s.to_string());
    let notion_page_id = doc.get_str("notion_page_id").ok().map(|s| s.to_string());

    let tags = match doc.get_array("tags") {
        Ok(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        Err(_) => Vec::new(),
    };

    let created_at = match doc.get("created_at") {
        Some(Bson::DateTime(dt)) => dt.to_chrono(),
        _ => Utc::now(),
    };

    let updated_at = match doc.get("updated_at") {
        Some(Bson::DateTime(dt)) => dt.to_chrono(),
        _ => Utc::now(),
    };

    Ok(DevTask {
        id,
        organization,
        title,
        description,
        priority,
        status,
        assignee,
        source,
        tags,
        notion_page_id,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn require_mongodb_uri(test_name: &str) -> bool {
        match env::var("MONGODB_URI") {
            Ok(value) if !value.trim().is_empty() => true,
            _ => {
                eprintln!("Skipping {test_name}; MONGODB_URI not set.");
                false
            }
        }
    }

    #[test]
    fn priority_serialization() {
        assert_eq!(Priority::P0.as_str(), "p0");
        assert_eq!(Priority::P1.as_str(), "p1");
        assert_eq!(Priority::P2.as_str(), "p2");
        assert_eq!(Priority::P3.as_str(), "p3");
    }

    #[test]
    fn task_status_serialization() {
        assert_eq!(TaskStatus::Backlog.as_str(), "backlog");
        assert_eq!(TaskStatus::InProgress.as_str(), "in_progress");
        assert_eq!(TaskStatus::Review.as_str(), "review");
        assert_eq!(TaskStatus::Done.as_str(), "done");
        assert_eq!(TaskStatus::Blocked.as_str(), "blocked");
    }

    #[test]
    fn task_source_serialization() {
        assert_eq!(TaskSource::UserFeedback.as_str(), "user_feedback");
        assert_eq!(TaskSource::Notetaker.as_str(), "notetaker");
        assert_eq!(TaskSource::MarketResearch.as_str(), "market_research");
        assert_eq!(TaskSource::Manual.as_str(), "manual");
    }

    #[test]
    fn dev_task_builder() {
        let task = DevTask::new(
            "deeptutor".to_string(),
            "Test task".to_string(),
            "Description".to_string(),
            TaskSource::Manual,
        )
        .with_priority(Priority::P1)
        .with_tags(vec!["bug".to_string(), "ui".to_string()])
        .with_assignee("dev@example.com".to_string());

        assert_eq!(task.organization, "deeptutor");
        assert_eq!(task.title, "Test task");
        assert_eq!(task.priority, Priority::P1);
        assert_eq!(task.tags, vec!["bug", "ui"]);
        assert_eq!(task.assignee, Some("dev@example.com".to_string()));
        assert_eq!(task.status, TaskStatus::Backlog);
    }

    // -------------------------------------------------------------------------
    // Integration tests (require MONGODB_URI)
    // -------------------------------------------------------------------------

    const TEST_ORG: &str = "test_org";

    #[test]
    fn integration_create_and_get_task() {
        if !require_mongodb_uri("integration_create_and_get_task") {
            return;
        }

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");

        // Create a task
        let task = DevTask::new(
            TEST_ORG.to_string(),
            "Integration test task".to_string(),
            "This is a test task created by integration tests".to_string(),
            TaskSource::Manual,
        )
        .with_priority(Priority::P1)
        .with_tags(vec!["test".to_string(), "integration".to_string()]);

        let task_id = store.insert_task(&task).expect("failed to insert task");

        // Retrieve it
        let retrieved = store
            .get_task(&task_id)
            .expect("failed to get task")
            .expect("task not found");

        assert_eq!(retrieved.title, "Integration test task");
        assert_eq!(retrieved.priority, Priority::P1);
        assert_eq!(retrieved.status, TaskStatus::Backlog);
        assert_eq!(retrieved.tags, vec!["test", "integration"]);

        // Clean up
        store.delete_task(&task_id).expect("failed to delete task");
    }

    #[test]
    fn integration_update_status() {
        if !require_mongodb_uri("integration_update_status") {
            return;
        }

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");

        let task = DevTask::new(
            TEST_ORG.to_string(),
            "Status update test".to_string(),
            "Testing status transitions".to_string(),
            TaskSource::UserFeedback,
        );

        let task_id = store.insert_task(&task).expect("failed to insert task");

        // Update status to InProgress
        store
            .update_status(&task_id, TaskStatus::InProgress)
            .expect("failed to update status");

        let updated = store
            .get_task(&task_id)
            .expect("failed to get task")
            .expect("task not found");

        assert_eq!(updated.status, TaskStatus::InProgress);

        // Update to Done
        store
            .update_status(&task_id, TaskStatus::Done)
            .expect("failed to update status");

        let done = store
            .get_task(&task_id)
            .expect("failed to get task")
            .expect("task not found");

        assert_eq!(done.status, TaskStatus::Done);

        // Clean up
        store.delete_task(&task_id).expect("failed to delete task");
    }

    #[test]
    fn integration_assign_task() {
        if !require_mongodb_uri("integration_assign_task") {
            return;
        }

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");

        let task = DevTask::new(
            TEST_ORG.to_string(),
            "Assignment test".to_string(),
            "Testing task assignment".to_string(),
            TaskSource::Notetaker,
        );

        let task_id = store.insert_task(&task).expect("failed to insert task");

        // Assign to developer
        store
            .update_assignee(&task_id, Some("dylan@deeptutor.dev"))
            .expect("failed to assign task");

        let assigned = store
            .get_task(&task_id)
            .expect("failed to get task")
            .expect("task not found");

        assert_eq!(assigned.assignee, Some("dylan@deeptutor.dev".to_string()));

        // Unassign
        store
            .update_assignee(&task_id, None)
            .expect("failed to unassign task");

        let unassigned = store
            .get_task(&task_id)
            .expect("failed to get task")
            .expect("task not found");

        assert_eq!(unassigned.assignee, None);

        // Clean up
        store.delete_task(&task_id).expect("failed to delete task");
    }

    #[test]
    fn integration_link_notion_page() {
        if !require_mongodb_uri("integration_link_notion_page") {
            return;
        }

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");

        let task = DevTask::new(
            TEST_ORG.to_string(),
            "Notion link test".to_string(),
            "Testing Notion page linking".to_string(),
            TaskSource::Manual,
        );

        let task_id = store.insert_task(&task).expect("failed to insert task");

        // Link to Notion page
        let notion_page_id = "abc123-notion-page-id";
        store
            .link_notion_page(&task_id, notion_page_id)
            .expect("failed to link notion page");

        // Retrieve by Notion page ID
        let by_notion = store
            .get_task_by_notion_page(notion_page_id)
            .expect("failed to get by notion page")
            .expect("task not found");

        assert_eq!(by_notion.title, "Notion link test");
        assert_eq!(by_notion.notion_page_id, Some(notion_page_id.to_string()));

        // Clean up
        store.delete_task(&task_id).expect("failed to delete task");
    }

    #[test]
    fn integration_list_by_status() {
        if !require_mongodb_uri("integration_list_by_status") {
            return;
        }

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");

        // Create tasks with different statuses
        let task1 = DevTask::new(
            TEST_ORG.to_string(),
            "List test 1".to_string(),
            "Backlog task".to_string(),
            TaskSource::Manual,
        );
        let task2 = DevTask::new(
            TEST_ORG.to_string(),
            "List test 2".to_string(),
            "Another backlog task".to_string(),
            TaskSource::Manual,
        )
        .with_priority(Priority::P0);

        let id1 = store.insert_task(&task1).expect("failed to insert task1");
        let id2 = store.insert_task(&task2).expect("failed to insert task2");

        // Move task2 to InProgress
        store
            .update_status(&id2, TaskStatus::InProgress)
            .expect("failed to update status");

        // List backlog tasks
        let backlog = store
            .list_tasks_by_status(TaskStatus::Backlog)
            .expect("failed to list backlog");

        assert!(backlog.iter().any(|t| t.title == "List test 1"));
        assert!(!backlog.iter().any(|t| t.title == "List test 2"));

        // List in-progress tasks
        let in_progress = store
            .list_tasks_by_status(TaskStatus::InProgress)
            .expect("failed to list in progress");

        assert!(in_progress.iter().any(|t| t.title == "List test 2"));

        // Clean up
        store.delete_task(&id1).expect("failed to delete task1");
        store.delete_task(&id2).expect("failed to delete task2");
    }

    #[test]
    fn integration_list_by_assignee() {
        if !require_mongodb_uri("integration_list_by_assignee") {
            return;
        }

        let store = DevTaskStore::new(TEST_ORG).expect("failed to create store");

        let task1 = DevTask::new(
            TEST_ORG.to_string(),
            "Assignee test 1".to_string(),
            "Task for Dylan".to_string(),
            TaskSource::Manual,
        )
        .with_assignee("dylan@deeptutor.dev".to_string());

        let task2 = DevTask::new(
            TEST_ORG.to_string(),
            "Assignee test 2".to_string(),
            "Task for Oliver".to_string(),
            TaskSource::Manual,
        )
        .with_assignee("oliver@dowhiz.com".to_string());

        let id1 = store.insert_task(&task1).expect("failed to insert task1");
        let id2 = store.insert_task(&task2).expect("failed to insert task2");

        // List Dylan's tasks
        let dylan_tasks = store
            .list_tasks_by_assignee("dylan@deeptutor.dev")
            .expect("failed to list by assignee");

        assert!(dylan_tasks.iter().any(|t| t.title == "Assignee test 1"));
        assert!(!dylan_tasks.iter().any(|t| t.title == "Assignee test 2"));

        // Clean up
        store.delete_task(&id1).expect("failed to delete task1");
        store.delete_task(&id2).expect("failed to delete task2");
    }
}
