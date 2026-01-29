//! SurrealDB-backed checkpointer for production persistence
//!
//! Requires the `persistence` feature:
//! ```toml
//! oxidizedgraph = { version = "0.1", features = ["persistence"] }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use surrealdb::engine::any::{connect, Any};
use surrealdb::opt::auth::Root;
use surrealdb::sql::Thing;
use surrealdb::Surreal;

use super::{Checkpoint, Checkpointer};
use crate::error::RuntimeError;

/// SurrealDB-backed checkpointer for production use
///
/// Supports multiple connection modes:
/// - In-memory: `mem://`
/// - File-based: `file://path/to/db`
/// - Remote: `ws://localhost:8000` or `wss://cloud.surrealdb.com`
///
/// # Example
///
/// ```rust,ignore
/// use oxidizedgraph::checkpoint::SurrealCheckpointer;
///
/// // In-memory for testing
/// let cp = SurrealCheckpointer::memory().await?;
///
/// // File-based for persistence
/// let cp = SurrealCheckpointer::file("./checkpoints.db").await?;
///
/// // Remote SurrealDB server
/// let cp = SurrealCheckpointer::connect("ws://localhost:8000")
///     .credentials("root", "root")
///     .namespace("oxidizedgraph")
///     .database("checkpoints")
///     .build()
///     .await?;
/// ```
pub struct SurrealCheckpointer {
    db: Surreal<Any>,
}

/// Builder for SurrealCheckpointer with remote connections
pub struct SurrealCheckpointerBuilder {
    url: String,
    username: Option<String>,
    password: Option<String>,
    namespace: String,
    database: String,
}

impl SurrealCheckpointerBuilder {
    /// Set authentication credentials
    pub fn credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    /// Set the namespace
    pub fn namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = ns.into();
        self
    }

    /// Set the database
    pub fn database(mut self, db: impl Into<String>) -> Self {
        self.database = db.into();
        self
    }

    /// Build and connect
    pub async fn build(self) -> Result<SurrealCheckpointer, RuntimeError> {
        let db: Surreal<Any> = connect(&self.url)
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB connect failed: {}", e)))?;

        if let (Some(user), Some(pass)) = (self.username, self.password) {
            db.signin(Root {
                username: &user,
                password: &pass,
            })
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB auth failed: {}", e)))?;
        }

        db.use_ns(&self.namespace)
            .use_db(&self.database)
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB use ns/db failed: {}", e)))?;

        let checkpointer = SurrealCheckpointer { db };
        checkpointer.setup_schema().await?;

        Ok(checkpointer)
    }
}

/// Internal record type for SurrealDB
#[derive(Debug, Serialize, Deserialize)]
struct CheckpointRecord {
    #[allow(dead_code)]
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Thing>,
    checkpoint_id: String,
    thread_id: String,
    node_id: String,
    state: serde_json::Value,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<String>,
    #[serde(default)]
    metadata: serde_json::Value,
}

impl From<Checkpoint> for CheckpointRecord {
    fn from(cp: Checkpoint) -> Self {
        Self {
            id: None,
            checkpoint_id: cp.id,
            thread_id: cp.thread_id,
            node_id: cp.node_id,
            state: serde_json::to_value(&cp.state).unwrap_or_default(),
            created_at: cp.created_at.to_rfc3339(),
            parent_id: cp.parent_id,
            metadata: if cp.metadata.is_null() {
                serde_json::json!({})
            } else {
                cp.metadata
            },
        }
    }
}

impl TryFrom<CheckpointRecord> for Checkpoint {
    type Error = RuntimeError;

    fn try_from(record: CheckpointRecord) -> Result<Self, Self::Error> {
        let state = serde_json::from_value(record.state)
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to deserialize state: {}", e)))?;

        let created_at = chrono::DateTime::parse_from_rfc3339(&record.created_at)
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to parse timestamp: {}", e)))?
            .with_timezone(&chrono::Utc);

        Ok(Checkpoint {
            id: record.checkpoint_id,
            thread_id: record.thread_id,
            node_id: record.node_id,
            state,
            created_at,
            parent_id: record.parent_id,
            metadata: record.metadata,
        })
    }
}

impl SurrealCheckpointer {
    /// Create an in-memory checkpointer (for testing)
    pub async fn memory() -> Result<Self, RuntimeError> {
        let db: Surreal<Any> = connect("mem://")
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB connect failed: {}", e)))?;

        db.use_ns("oxidizedgraph")
            .use_db("checkpoints")
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB use ns/db failed: {}", e)))?;

        let checkpointer = Self { db };
        checkpointer.setup_schema().await?;

        Ok(checkpointer)
    }

    /// Create a file-based checkpointer
    pub async fn file(path: impl AsRef<str>) -> Result<Self, RuntimeError> {
        let url = format!("file://{}", path.as_ref());
        let db: Surreal<Any> = connect(&url)
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB connect failed: {}", e)))?;

        db.use_ns("oxidizedgraph")
            .use_db("checkpoints")
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB use ns/db failed: {}", e)))?;

        let checkpointer = Self { db };
        checkpointer.setup_schema().await?;

        Ok(checkpointer)
    }

    /// Connect to a remote SurrealDB server
    pub fn connect_remote(url: impl Into<String>) -> SurrealCheckpointerBuilder {
        SurrealCheckpointerBuilder {
            url: url.into(),
            username: None,
            password: None,
            namespace: "oxidizedgraph".to_string(),
            database: "checkpoints".to_string(),
        }
    }

    /// Setup the database schema
    async fn setup_schema(&self) -> Result<(), RuntimeError> {
        // Create indexes for efficient queries (using schemaless for flexibility)
        self.db
            .query(
                r#"
                DEFINE INDEX IF NOT EXISTS idx_thread_id ON TABLE checkpoints COLUMNS thread_id;
                DEFINE INDEX IF NOT EXISTS idx_thread_created ON TABLE checkpoints COLUMNS thread_id, created_at;
                "#,
            )
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to setup schema: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl Checkpointer for SurrealCheckpointer {
    async fn save(&self, checkpoint: Checkpoint) -> Result<(), RuntimeError> {
        let record: CheckpointRecord = checkpoint.into();

        // Use create with the record's checkpoint_id as the record ID
        let _: Option<CheckpointRecord> = self.db
            .create(("checkpoints", record.checkpoint_id.clone()))
            .content(record)
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to save checkpoint: {}", e)))?;

        Ok(())
    }

    async fn load(&self, thread_id: &str) -> Result<Option<Checkpoint>, RuntimeError> {
        let thread_id = thread_id.to_string();
        let mut result = self
            .db
            .query("SELECT * FROM checkpoints WHERE thread_id = $thread_id ORDER BY created_at DESC LIMIT 1")
            .bind(("thread_id", thread_id))
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to load checkpoint: {}", e)))?;

        let records: Vec<CheckpointRecord> = result
            .take(0)
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to parse result: {}", e)))?;

        match records.into_iter().next() {
            Some(record) => Ok(Some(record.try_into()?)),
            None => Ok(None),
        }
    }

    async fn load_by_id(&self, checkpoint_id: &str) -> Result<Option<Checkpoint>, RuntimeError> {
        let record: Option<CheckpointRecord> = self.db
            .select(("checkpoints", checkpoint_id))
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to load checkpoint: {}", e)))?;

        match record {
            Some(r) => Ok(Some(r.try_into()?)),
            None => Ok(None),
        }
    }

    async fn list(&self, thread_id: &str) -> Result<Vec<Checkpoint>, RuntimeError> {
        let thread_id = thread_id.to_string();
        let mut result = self
            .db
            .query("SELECT * FROM checkpoints WHERE thread_id = $thread_id ORDER BY created_at DESC")
            .bind(("thread_id", thread_id))
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to list checkpoints: {}", e)))?;

        let records: Vec<CheckpointRecord> = result
            .take(0)
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to parse result: {}", e)))?;

        records.into_iter().map(|r| r.try_into()).collect()
    }

    async fn delete(&self, checkpoint_id: &str) -> Result<(), RuntimeError> {
        let _: Option<CheckpointRecord> = self.db
            .delete(("checkpoints", checkpoint_id))
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to delete checkpoint: {}", e)))?;

        Ok(())
    }

    async fn delete_thread(&self, thread_id: &str) -> Result<(), RuntimeError> {
        let thread_id = thread_id.to_string();
        self.db
            .query("DELETE FROM checkpoints WHERE thread_id = $thread_id")
            .bind(("thread_id", thread_id))
            .await
            .map_err(|e| {
                RuntimeError::InvalidState(format!("Failed to delete thread checkpoints: {}", e))
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;

    #[tokio::test]
    async fn test_memory_checkpointer() {
        let checkpointer = SurrealCheckpointer::memory().await.unwrap();

        let state = AgentState::with_user_message("Hello");
        let checkpoint = Checkpoint::new("thread-1", "node-a", state);
        let checkpoint_id = checkpoint.id.clone();

        checkpointer.save(checkpoint).await.unwrap();

        let loaded = checkpointer.load("thread-1").await.unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, checkpoint_id);
        assert_eq!(loaded.thread_id, "thread-1");
    }

    #[tokio::test]
    async fn test_list_and_history() {
        let checkpointer = SurrealCheckpointer::memory().await.unwrap();

        for i in 0..5 {
            let state = AgentState::new();
            let checkpoint = Checkpoint::new("thread-1", format!("node-{}", i), state);
            checkpointer.save(checkpoint).await.unwrap();
            // Small delay to ensure ordering
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        let list = checkpointer.list("thread-1").await.unwrap();
        assert_eq!(list.len(), 5);

        let history = checkpointer.history("thread-1", 3).await.unwrap();
        assert_eq!(history.len(), 3);
    }

    #[tokio::test]
    async fn test_delete() {
        let checkpointer = SurrealCheckpointer::memory().await.unwrap();

        let state = AgentState::new();
        let checkpoint = Checkpoint::new("thread-1", "node-a", state);
        let checkpoint_id = checkpoint.id.clone();

        checkpointer.save(checkpoint).await.unwrap();
        checkpointer.delete(&checkpoint_id).await.unwrap();

        let loaded = checkpointer.load_by_id(&checkpoint_id).await.unwrap();
        assert!(loaded.is_none());
    }
}
