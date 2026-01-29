//! In-memory checkpointer for testing and development

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use super::{Checkpoint, Checkpointer};
use crate::error::RuntimeError;

/// In-memory checkpointer for testing and development
///
/// Stores checkpoints in memory. Data is lost when the process exits.
/// For production, use `SurrealCheckpointer` with the `persistence` feature.
pub struct MemoryCheckpointer {
    /// Checkpoints indexed by ID
    checkpoints: RwLock<HashMap<String, Checkpoint>>,
    /// Thread ID to checkpoint IDs mapping (ordered by creation time)
    threads: RwLock<HashMap<String, Vec<String>>>,
}

impl MemoryCheckpointer {
    /// Create a new in-memory checkpointer
    pub fn new() -> Self {
        Self {
            checkpoints: RwLock::new(HashMap::new()),
            threads: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryCheckpointer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Checkpointer for MemoryCheckpointer {
    async fn save(&self, checkpoint: Checkpoint) -> Result<(), RuntimeError> {
        let id = checkpoint.id.clone();
        let thread_id = checkpoint.thread_id.clone();

        // Store the checkpoint
        {
            let mut checkpoints = self
                .checkpoints
                .write()
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
            checkpoints.insert(id.clone(), checkpoint);
        }

        // Update thread index
        {
            let mut threads = self
                .threads
                .write()
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
            threads.entry(thread_id).or_default().push(id);
        }

        Ok(())
    }

    async fn load(&self, thread_id: &str) -> Result<Option<Checkpoint>, RuntimeError> {
        let threads = self
            .threads
            .read()
            .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;

        let checkpoint_ids = match threads.get(thread_id) {
            Some(ids) => ids,
            None => return Ok(None),
        };

        let latest_id = match checkpoint_ids.last() {
            Some(id) => id,
            None => return Ok(None),
        };

        let checkpoints = self
            .checkpoints
            .read()
            .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;

        Ok(checkpoints.get(latest_id).cloned())
    }

    async fn load_by_id(&self, checkpoint_id: &str) -> Result<Option<Checkpoint>, RuntimeError> {
        let checkpoints = self
            .checkpoints
            .read()
            .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;

        Ok(checkpoints.get(checkpoint_id).cloned())
    }

    async fn list(&self, thread_id: &str) -> Result<Vec<Checkpoint>, RuntimeError> {
        let threads = self
            .threads
            .read()
            .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;

        let checkpoint_ids = match threads.get(thread_id) {
            Some(ids) => ids.clone(),
            None => return Ok(Vec::new()),
        };

        let checkpoints = self
            .checkpoints
            .read()
            .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;

        // Return in reverse order (newest first)
        let result: Vec<Checkpoint> = checkpoint_ids
            .iter()
            .rev()
            .filter_map(|id| checkpoints.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn delete(&self, checkpoint_id: &str) -> Result<(), RuntimeError> {
        let checkpoint = {
            let mut checkpoints = self
                .checkpoints
                .write()
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
            checkpoints.remove(checkpoint_id)
        };

        if let Some(cp) = checkpoint {
            let mut threads = self
                .threads
                .write()
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;

            if let Some(ids) = threads.get_mut(&cp.thread_id) {
                ids.retain(|id| id != checkpoint_id);
            }
        }

        Ok(())
    }

    async fn delete_thread(&self, thread_id: &str) -> Result<(), RuntimeError> {
        let checkpoint_ids = {
            let mut threads = self
                .threads
                .write()
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
            threads.remove(thread_id).unwrap_or_default()
        };

        {
            let mut checkpoints = self
                .checkpoints
                .write()
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;

            for id in checkpoint_ids {
                checkpoints.remove(&id);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;

    #[tokio::test]
    async fn test_save_and_load() {
        let checkpointer = MemoryCheckpointer::new();

        let state = AgentState::with_user_message("Hello");
        let checkpoint = Checkpoint::new("thread-1", "node-a", state);
        let checkpoint_id = checkpoint.id.clone();

        checkpointer.save(checkpoint).await.unwrap();

        let loaded = checkpointer.load("thread-1").await.unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, checkpoint_id);
        assert_eq!(loaded.thread_id, "thread-1");
        assert_eq!(loaded.node_id, "node-a");
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let checkpointer = MemoryCheckpointer::new();
        let loaded = checkpointer.load("nonexistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_list_checkpoints() {
        let checkpointer = MemoryCheckpointer::new();

        // Save multiple checkpoints
        for i in 0..3 {
            let state = AgentState::new();
            let checkpoint = Checkpoint::new("thread-1", format!("node-{}", i), state);
            checkpointer.save(checkpoint).await.unwrap();
        }

        let list = checkpointer.list("thread-1").await.unwrap();
        assert_eq!(list.len(), 3);

        // Should be newest first
        assert_eq!(list[0].node_id, "node-2");
        assert_eq!(list[2].node_id, "node-0");
    }

    #[tokio::test]
    async fn test_delete_checkpoint() {
        let checkpointer = MemoryCheckpointer::new();

        let state = AgentState::new();
        let checkpoint = Checkpoint::new("thread-1", "node-a", state);
        let checkpoint_id = checkpoint.id.clone();

        checkpointer.save(checkpoint).await.unwrap();
        checkpointer.delete(&checkpoint_id).await.unwrap();

        let loaded = checkpointer.load_by_id(&checkpoint_id).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_delete_thread() {
        let checkpointer = MemoryCheckpointer::new();

        // Save checkpoints to two threads
        for i in 0..3 {
            let state = AgentState::new();
            let checkpoint = Checkpoint::new("thread-1", format!("node-{}", i), state);
            checkpointer.save(checkpoint).await.unwrap();
        }

        let state = AgentState::new();
        let checkpoint = Checkpoint::new("thread-2", "node-0", state);
        checkpointer.save(checkpoint).await.unwrap();

        // Delete thread-1
        checkpointer.delete_thread("thread-1").await.unwrap();

        // thread-1 should be empty
        let list = checkpointer.list("thread-1").await.unwrap();
        assert!(list.is_empty());

        // thread-2 should still have its checkpoint
        let list = checkpointer.list("thread-2").await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_history() {
        let checkpointer = MemoryCheckpointer::new();

        for i in 0..5 {
            let state = AgentState::new();
            let checkpoint = Checkpoint::new("thread-1", format!("node-{}", i), state);
            checkpointer.save(checkpoint).await.unwrap();
        }

        let history = checkpointer.history("thread-1", 3).await.unwrap();
        assert_eq!(history.len(), 3);

        // Should be newest first
        assert_eq!(history[0].node_id, "node-4");
        assert_eq!(history[1].node_id, "node-3");
        assert_eq!(history[2].node_id, "node-2");
    }
}
