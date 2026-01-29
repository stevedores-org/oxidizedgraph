//! Checkpointing-enabled graph runner
//!
//! Provides `CheckpointingRunner` which extends `GraphRunner` with
//! persistence and resume capabilities for human-in-the-loop workflows.

use std::sync::{Arc, RwLock};
use tracing::{debug, info, instrument, warn};

use super::{Checkpoint, CheckpointConfig, Checkpointer};
use crate::error::RuntimeError;
use crate::graph::{transitions, CompiledGraph, NodeOutput};
use crate::runner::RunnerConfig;
use crate::state::AgentState;

/// Result of a checkpointing run
#[derive(Debug)]
pub enum RunResult {
    /// Execution completed successfully
    Completed(AgentState),

    /// Execution was interrupted (human-in-the-loop)
    Interrupted {
        /// The checkpoint that was saved
        checkpoint: Checkpoint,
        /// Reason for interruption
        reason: String,
    },
}

impl RunResult {
    /// Get the state regardless of completion status
    pub fn state(&self) -> &AgentState {
        match self {
            RunResult::Completed(state) => state,
            RunResult::Interrupted { checkpoint, .. } => &checkpoint.state,
        }
    }

    /// Check if execution completed
    pub fn is_completed(&self) -> bool {
        matches!(self, RunResult::Completed(_))
    }

    /// Check if execution was interrupted
    pub fn is_interrupted(&self) -> bool {
        matches!(self, RunResult::Interrupted { .. })
    }

    /// Get the checkpoint if interrupted
    pub fn checkpoint(&self) -> Option<&Checkpoint> {
        match self {
            RunResult::Interrupted { checkpoint, .. } => Some(checkpoint),
            _ => None,
        }
    }
}

/// Graph runner with checkpointing support
///
/// This runner saves state checkpoints during execution, enabling:
/// - Human-in-the-loop interrupts and resume
/// - Time-travel debugging
/// - Execution branching
///
/// # Example
///
/// ```rust,ignore
/// use oxidizedgraph::checkpoint::{CheckpointingRunner, MemoryCheckpointer};
///
/// let checkpointer = Arc::new(MemoryCheckpointer::new());
/// let runner = CheckpointingRunner::new(graph, checkpointer);
///
/// // Start execution
/// let result = runner.invoke("thread-1", initial_state).await?;
///
/// // If interrupted, resume later
/// if result.is_interrupted() {
///     // ... wait for human input ...
///     let resumed = runner.resume("thread-1").await?;
/// }
/// ```
pub struct CheckpointingRunner<C: Checkpointer> {
    graph: CompiledGraph,
    config: RunnerConfig,
    checkpoint_config: CheckpointConfig,
    checkpointer: Arc<C>,
}

impl<C: Checkpointer> CheckpointingRunner<C> {
    /// Create a new checkpointing runner
    pub fn new(graph: CompiledGraph, checkpointer: Arc<C>) -> Self {
        Self {
            graph,
            config: RunnerConfig::default(),
            checkpoint_config: CheckpointConfig::default(),
            checkpointer,
        }
    }

    /// Set the runner configuration
    pub fn with_config(mut self, config: RunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the checkpoint configuration
    pub fn with_checkpoint_config(mut self, config: CheckpointConfig) -> Self {
        self.checkpoint_config = config;
        self
    }

    /// Enable checkpointing after every node
    pub fn checkpoint_every_node(mut self) -> Self {
        self.checkpoint_config.checkpoint_every_node = true;
        self
    }

    /// Execute the graph with checkpointing
    #[instrument(skip(self, thread_id, initial_state), fields(graph_name = ?self.graph.name()))]
    pub async fn invoke(
        &self,
        thread_id: impl Into<String>,
        initial_state: AgentState,
    ) -> Result<RunResult, RuntimeError> {
        let thread_id = thread_id.into();
        let state = Arc::new(RwLock::new(initial_state));
        self.run_loop(&thread_id, state, None).await
    }

    /// Resume execution from the latest checkpoint
    #[instrument(skip(self), fields(graph_name = ?self.graph.name()))]
    pub async fn resume(&self, thread_id: &str) -> Result<RunResult, RuntimeError> {
        let checkpoint = self
            .checkpointer
            .load(thread_id)
            .await?
            .ok_or_else(|| RuntimeError::CheckpointNotFound(thread_id.to_string()))?;

        info!(
            thread_id = %thread_id,
            checkpoint_id = %checkpoint.id,
            node_id = %checkpoint.node_id,
            "Resuming from checkpoint"
        );

        let state = Arc::new(RwLock::new(checkpoint.state.clone()));
        let start_node = checkpoint.node_id.clone();

        self.run_loop(thread_id, state, Some(start_node)).await
    }

    /// Resume execution from a specific checkpoint
    #[instrument(skip(self), fields(graph_name = ?self.graph.name()))]
    pub async fn resume_from(&self, checkpoint_id: &str) -> Result<RunResult, RuntimeError> {
        let checkpoint = self
            .checkpointer
            .load_by_id(checkpoint_id)
            .await?
            .ok_or_else(|| RuntimeError::CheckpointNotFound(checkpoint_id.to_string()))?;

        info!(
            checkpoint_id = %checkpoint_id,
            thread_id = %checkpoint.thread_id,
            node_id = %checkpoint.node_id,
            "Resuming from specific checkpoint"
        );

        let thread_id = checkpoint.thread_id.clone();
        let state = Arc::new(RwLock::new(checkpoint.state.clone()));
        let start_node = checkpoint.node_id.clone();

        self.run_loop(&thread_id, state, Some(start_node)).await
    }

    /// Main execution loop with checkpointing
    async fn run_loop(
        &self,
        thread_id: &str,
        state: Arc<RwLock<AgentState>>,
        start_node: Option<String>,
    ) -> Result<RunResult, RuntimeError> {
        let mut current_node = start_node.unwrap_or_else(|| self.graph.entry_point().to_string());
        let mut iterations: u32 = 0;

        info!(
            thread_id = %thread_id,
            entry_point = %current_node,
            max_iterations = self.config.max_iterations,
            "Starting checkpointed graph execution"
        );

        loop {
            // Check iteration limit
            if iterations >= self.config.max_iterations {
                warn!(
                    iterations = iterations,
                    max = self.config.max_iterations,
                    "Maximum iterations exceeded"
                );

                // Save final checkpoint before error
                let final_state = state
                    .read()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?
                    .clone();

                let checkpoint = Checkpoint::new(thread_id, &current_node, final_state);
                self.checkpointer.save(checkpoint).await?;

                return Err(RuntimeError::RecursionLimit(self.config.max_iterations));
            }

            // Check for END node
            if current_node == transitions::END {
                info!(
                    thread_id = %thread_id,
                    iterations = iterations,
                    "Graph execution completed"
                );

                let final_state = state
                    .read()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?
                    .clone();

                return Ok(RunResult::Completed(final_state));
            }

            // Get the current node
            let node = self
                .graph
                .get_node(&current_node)
                .ok_or_else(|| RuntimeError::NodeNotFound(current_node.clone()))?;

            debug!(
                thread_id = %thread_id,
                node_id = %current_node,
                iteration = iterations,
                "Executing node"
            );

            // Save pre-execution checkpoint if configured
            if self.checkpoint_config.checkpoint_every_node {
                let current_state = state
                    .read()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?
                    .clone();

                let checkpoint = Checkpoint::new(thread_id, &current_node, current_state);
                self.checkpointer.save(checkpoint).await?;
            }

            // Execute the node
            let output = node
                .executor
                .execute(state.clone())
                .await
                .map_err(|e| RuntimeError::node_failed(&current_node, e))?;

            // Update iteration counter in state
            {
                let mut guard = state
                    .write()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                guard.increment_iteration();
            }

            iterations += 1;

            // Determine next node based on output
            let next_node = match &output {
                NodeOutput::Finish => {
                    info!(node_id = %current_node, "Node signaled finish");
                    transitions::END.to_string()
                }
                NodeOutput::Continue(Some(target)) => {
                    debug!(node_id = %current_node, target = %target, "Node specified next target");
                    target.clone()
                }
                NodeOutput::Continue(None) => {
                    // Look up edge from graph
                    let current_state = state
                        .read()
                        .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                    match self.graph.get_next_node(&current_node, &current_state) {
                        Some(next) => {
                            debug!(node_id = %current_node, next = %next, "Following graph edge");
                            next
                        }
                        None => {
                            debug!(
                                node_id = %current_node,
                                "No outgoing edge, ending execution"
                            );
                            transitions::END.to_string()
                        }
                    }
                }
                NodeOutput::Route(target) => {
                    debug!(node_id = %current_node, target = %target, "Node routing to target");
                    target.clone()
                }
            };

            current_node = next_node;
        }
    }

    /// Get checkpoint history for a thread
    pub async fn history(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<Checkpoint>, RuntimeError> {
        self.checkpointer.history(thread_id, limit).await
    }

    /// Get a reference to the underlying graph
    pub fn graph(&self) -> &CompiledGraph {
        &self.graph
    }

    /// Get a reference to the runner config
    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }

    /// Get a reference to the checkpointer
    pub fn checkpointer(&self) -> &Arc<C> {
        &self.checkpointer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::MemoryCheckpointer;
    use crate::error::NodeError;
    use crate::graph::{GraphBuilder, NodeExecutor};
    use crate::state::SharedState;
    use async_trait::async_trait;

    struct CounterNode {
        id: String,
        next: Option<String>,
    }

    #[async_trait]
    impl NodeExecutor for CounterNode {
        fn id(&self) -> &str {
            &self.id
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            {
                let mut guard = state
                    .write()
                    .map_err(|e| NodeError::execution_failed(e.to_string()))?;
                let count = guard.get_context::<i32>("count").unwrap_or(0);
                guard.set_context("count", count + 1);
            }

            match &self.next {
                Some(target) => Ok(NodeOutput::continue_to(target.clone())),
                None => Ok(NodeOutput::finish()),
            }
        }
    }

    #[tokio::test]
    async fn test_checkpointing_execution() {
        let graph = GraphBuilder::new()
            .add_node(CounterNode {
                id: "first".to_string(),
                next: Some("second".to_string()),
            })
            .add_node(CounterNode {
                id: "second".to_string(),
                next: None,
            })
            .set_entry_point("first")
            .compile()
            .unwrap();

        let checkpointer = Arc::new(MemoryCheckpointer::new());
        let runner = CheckpointingRunner::new(graph, checkpointer.clone()).checkpoint_every_node();

        let result = runner.invoke("thread-1", AgentState::new()).await.unwrap();

        assert!(result.is_completed());
        assert_eq!(result.state().get_context::<i32>("count"), Some(2));

        // Verify checkpoints were saved
        let history = checkpointer.list("thread-1").await.unwrap();
        assert_eq!(history.len(), 2);
    }

    #[tokio::test]
    async fn test_resume_from_checkpoint() {
        let graph = GraphBuilder::new()
            .add_node(CounterNode {
                id: "first".to_string(),
                next: Some("second".to_string()),
            })
            .add_node(CounterNode {
                id: "second".to_string(),
                next: Some("third".to_string()),
            })
            .add_node(CounterNode {
                id: "third".to_string(),
                next: None,
            })
            .set_entry_point("first")
            .compile()
            .unwrap();

        let checkpointer = Arc::new(MemoryCheckpointer::new());
        let runner = CheckpointingRunner::new(graph, checkpointer.clone()).checkpoint_every_node();

        // Run first
        let _ = runner.invoke("thread-1", AgentState::new()).await.unwrap();

        // Get first checkpoint (should be at "first" node)
        let history = checkpointer.list("thread-1").await.unwrap();
        let first_checkpoint = history.last().unwrap();
        assert_eq!(first_checkpoint.node_id, "first");

        // Resume from first checkpoint
        let result = runner.resume_from(&first_checkpoint.id).await.unwrap();

        assert!(result.is_completed());
        // Should have run all three nodes again
        assert_eq!(result.state().get_context::<i32>("count"), Some(3));
    }
}
