//! Checkpointing and persistence for oxidizedgraph
//!
//! This module provides state persistence for human-in-the-loop workflows,
//! allowing graph execution to be paused, saved, and resumed.
//!
//! # Features
//!
//! Enable the `persistence` feature to use SurrealDB-backed checkpointing:
//!
//! ```toml
//! oxidizedgraph = { version = "0.1", features = ["persistence"] }
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use oxidizedgraph::checkpoint::{Checkpointer, MemoryCheckpointer};
//!
//! let checkpointer = MemoryCheckpointer::new();
//! let checkpoint = Checkpoint::new("thread-1", "node-a", state);
//! checkpointer.save(checkpoint).await?;
//!
//! // Later, resume from checkpoint
//! let restored = checkpointer.load("thread-1").await?;
//! ```

mod memory;
mod runner;
#[cfg(feature = "persistence")]
mod surreal;

pub use memory::MemoryCheckpointer;
pub use runner::{CheckpointingRunner, RunResult};
#[cfg(feature = "persistence")]
pub use surreal::SurrealCheckpointer;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::RuntimeError;
use crate::state::AgentState;

/// A checkpoint representing saved graph execution state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique identifier for this checkpoint
    pub id: String,

    /// Thread/conversation identifier (groups related checkpoints)
    pub thread_id: String,

    /// The node that was executing when checkpoint was created
    pub node_id: String,

    /// The saved agent state
    pub state: AgentState,

    /// When this checkpoint was created
    pub created_at: DateTime<Utc>,

    /// Optional parent checkpoint ID (for branching)
    pub parent_id: Option<String>,

    /// Metadata for this checkpoint
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(thread_id: impl Into<String>, node_id: impl Into<String>, state: AgentState) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.into(),
            node_id: node_id.into(),
            state,
            created_at: Utc::now(),
            parent_id: None,
            metadata: serde_json::Value::Null,
        }
    }

    /// Create a checkpoint with a parent reference
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Add metadata to the checkpoint
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Configuration for checkpoint behavior
#[derive(Clone, Debug, Default)]
pub struct CheckpointConfig {
    /// Save checkpoint after every node execution
    pub checkpoint_every_node: bool,

    /// Maximum number of checkpoints to keep per thread
    pub max_checkpoints_per_thread: Option<usize>,

    /// Whether to enable branching (multiple execution paths)
    pub enable_branching: bool,
}

impl CheckpointConfig {
    /// Create a new config with checkpointing after every node
    pub fn every_node() -> Self {
        Self {
            checkpoint_every_node: true,
            ..Default::default()
        }
    }

    /// Set maximum checkpoints per thread
    pub fn max_per_thread(mut self, max: usize) -> Self {
        self.max_checkpoints_per_thread = Some(max);
        self
    }

    /// Enable execution branching
    pub fn with_branching(mut self) -> Self {
        self.enable_branching = true;
        self
    }
}

/// Trait for checkpoint storage backends
#[async_trait]
pub trait Checkpointer: Send + Sync {
    /// Save a checkpoint
    async fn save(&self, checkpoint: Checkpoint) -> Result<(), RuntimeError>;

    /// Load the latest checkpoint for a thread
    async fn load(&self, thread_id: &str) -> Result<Option<Checkpoint>, RuntimeError>;

    /// Load a specific checkpoint by ID
    async fn load_by_id(&self, checkpoint_id: &str) -> Result<Option<Checkpoint>, RuntimeError>;

    /// List all checkpoints for a thread (newest first)
    async fn list(&self, thread_id: &str) -> Result<Vec<Checkpoint>, RuntimeError>;

    /// Delete a checkpoint
    async fn delete(&self, checkpoint_id: &str) -> Result<(), RuntimeError>;

    /// Delete all checkpoints for a thread
    async fn delete_thread(&self, thread_id: &str) -> Result<(), RuntimeError>;

    /// Get checkpoint history (for time-travel debugging)
    async fn history(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<Checkpoint>, RuntimeError> {
        let mut checkpoints = self.list(thread_id).await?;
        checkpoints.truncate(limit);
        Ok(checkpoints)
    }
}

/// Extension trait for resumable graph execution
#[async_trait]
pub trait Resumable {
    /// Resume execution from the latest checkpoint for a thread
    async fn resume(&self, thread_id: &str) -> Result<AgentState, RuntimeError>;

    /// Resume execution from a specific checkpoint
    async fn resume_from(&self, checkpoint_id: &str) -> Result<AgentState, RuntimeError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_creation() {
        let state = AgentState::new();
        let checkpoint = Checkpoint::new("thread-1", "node-a", state);

        assert!(!checkpoint.id.is_empty());
        assert_eq!(checkpoint.thread_id, "thread-1");
        assert_eq!(checkpoint.node_id, "node-a");
        assert!(checkpoint.parent_id.is_none());
    }

    #[test]
    fn test_checkpoint_with_parent() {
        let state = AgentState::new();
        let checkpoint = Checkpoint::new("thread-1", "node-a", state).with_parent("parent-123");

        assert_eq!(checkpoint.parent_id, Some("parent-123".to_string()));
    }

    #[test]
    fn test_checkpoint_config() {
        let config = CheckpointConfig::every_node().max_per_thread(10).with_branching();

        assert!(config.checkpoint_every_node);
        assert_eq!(config.max_checkpoints_per_thread, Some(10));
        assert!(config.enable_branching);
    }
}
