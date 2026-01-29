//! Streaming graph runner with event emission

use std::sync::{Arc, RwLock};
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::checkpoint::{Checkpoint, CheckpointConfig, Checkpointer, MemoryCheckpointer};
use crate::error::RuntimeError;
use crate::graph::{transitions, CompiledGraph, NodeOutput};
use crate::runner::RunnerConfig;
use crate::state::AgentState;

use super::bus::EventBus;
use super::types::Event;

/// Result of a streaming run
#[derive(Debug)]
pub enum StreamingRunResult {
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

impl StreamingRunResult {
    /// Get the state regardless of completion status
    pub fn state(&self) -> &AgentState {
        match self {
            StreamingRunResult::Completed(state) => state,
            StreamingRunResult::Interrupted { checkpoint, .. } => &checkpoint.state,
        }
    }

    /// Check if execution completed
    pub fn is_completed(&self) -> bool {
        matches!(self, StreamingRunResult::Completed(_))
    }
}

/// Graph runner with event streaming and optional checkpointing
///
/// This runner emits events during execution for observability:
/// - Graph started/completed/error
/// - Node entered/exited/error
/// - Checkpoint saved/restored
///
/// # Example
///
/// ```rust,ignore
/// use oxidizedgraph::events::{EventBus, StreamingRunner, LoggingHandler, spawn_handler};
///
/// let bus = Arc::new(EventBus::new());
/// let runner = StreamingRunner::new(graph, bus.clone());
///
/// // Subscribe to events
/// let handler = Arc::new(LoggingHandler::new());
/// spawn_handler(handler, bus.subscribe());
///
/// // Run with events
/// let result = runner.invoke("thread-1", initial_state).await?;
/// ```
pub struct StreamingRunner<C: Checkpointer = MemoryCheckpointer> {
    graph: CompiledGraph,
    config: RunnerConfig,
    checkpoint_config: CheckpointConfig,
    checkpointer: Option<Arc<C>>,
    event_bus: Arc<EventBus>,
}

impl StreamingRunner<MemoryCheckpointer> {
    /// Create a new streaming runner without checkpointing
    pub fn new(graph: CompiledGraph, event_bus: Arc<EventBus>) -> Self {
        Self {
            graph,
            config: RunnerConfig::default(),
            checkpoint_config: CheckpointConfig::default(),
            checkpointer: None,
            event_bus,
        }
    }
}

impl<C: Checkpointer> StreamingRunner<C> {
    /// Create a new streaming runner with checkpointing
    pub fn with_checkpointer(
        graph: CompiledGraph,
        event_bus: Arc<EventBus>,
        checkpointer: Arc<C>,
    ) -> Self {
        Self {
            graph,
            config: RunnerConfig::default(),
            checkpoint_config: CheckpointConfig::default(),
            checkpointer: Some(checkpointer),
            event_bus,
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

    /// Execute the graph with event streaming
    pub async fn invoke(
        &self,
        thread_id: impl Into<String>,
        initial_state: AgentState,
    ) -> Result<StreamingRunResult, RuntimeError> {
        let thread_id = thread_id.into();
        let state = Arc::new(RwLock::new(initial_state));
        self.run_loop(&thread_id, state, None).await
    }

    /// Resume execution from the latest checkpoint
    pub async fn resume(&self, thread_id: &str) -> Result<StreamingRunResult, RuntimeError> {
        let checkpointer = self
            .checkpointer
            .as_ref()
            .ok_or(RuntimeError::NoCheckpointer)?;

        let checkpoint = checkpointer
            .load(thread_id)
            .await?
            .ok_or_else(|| RuntimeError::CheckpointNotFound(thread_id.to_string()))?;

        // Emit checkpoint restored event
        self.event_bus.publish(Event::checkpoint_restored(
            thread_id,
            checkpoint.id.clone(),
            checkpoint.node_id.clone(),
        ));

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

    /// Main execution loop with event emission
    async fn run_loop(
        &self,
        thread_id: &str,
        state: Arc<RwLock<AgentState>>,
        start_node: Option<String>,
    ) -> Result<StreamingRunResult, RuntimeError> {
        let mut current_node = start_node.unwrap_or_else(|| self.graph.entry_point().to_string());
        let mut iterations: u32 = 0;
        let graph_start = Instant::now();

        // Emit graph started event
        self.event_bus.publish(Event::graph_started(
            thread_id,
            self.graph.name().map(String::from),
            current_node.clone(),
        ));

        loop {
            // Check iteration limit
            if iterations >= self.config.max_iterations {
                warn!(
                    iterations = iterations,
                    max = self.config.max_iterations,
                    "Maximum iterations exceeded"
                );

                // Emit error event
                self.event_bus.publish(Event::graph_error(
                    thread_id,
                    format!("Maximum iterations exceeded: {}", self.config.max_iterations),
                ));

                // Save checkpoint before error
                if let Some(ref checkpointer) = self.checkpointer {
                    let final_state = state
                        .read()
                        .map_err(|e| RuntimeError::InvalidState(e.to_string()))?
                        .clone();
                    let checkpoint = Checkpoint::new(thread_id, &current_node, final_state);
                    checkpointer.save(checkpoint).await?;
                }

                return Err(RuntimeError::RecursionLimit(self.config.max_iterations));
            }

            // Check for END node
            if current_node == transitions::END {
                let duration = graph_start.elapsed();

                // Emit graph completed event
                self.event_bus.publish(Event::graph_completed(
                    thread_id,
                    iterations,
                    duration,
                ));

                info!(
                    thread_id = %thread_id,
                    iterations = iterations,
                    duration_ms = duration.as_millis(),
                    "Graph execution completed"
                );

                let final_state = state
                    .read()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?
                    .clone();

                return Ok(StreamingRunResult::Completed(final_state));
            }

            // Get the current node
            let node = self
                .graph
                .get_node(&current_node)
                .ok_or_else(|| RuntimeError::NodeNotFound(current_node.clone()))?;

            // Emit node entered event
            self.event_bus.publish(Event::node_entered(
                thread_id,
                current_node.clone(),
                iterations,
            ));

            let node_start = Instant::now();

            // Save pre-execution checkpoint if configured
            if self.checkpoint_config.checkpoint_every_node {
                if let Some(ref checkpointer) = self.checkpointer {
                    let current_state = state
                        .read()
                        .map_err(|e| RuntimeError::InvalidState(e.to_string()))?
                        .clone();

                    let checkpoint = Checkpoint::new(thread_id, &current_node, current_state);
                    let checkpoint_id = checkpoint.id.clone();
                    checkpointer.save(checkpoint).await?;

                    // Emit checkpoint saved event
                    self.event_bus.publish(Event::checkpoint_saved(
                        thread_id,
                        checkpoint_id,
                        current_node.clone(),
                    ));
                }
            }

            // Execute the node
            let output = match node.executor.execute(state.clone()).await {
                Ok(output) => output,
                Err(e) => {
                    // Emit node error event
                    self.event_bus.publish(Event::node_error(
                        thread_id,
                        current_node.clone(),
                        e.to_string(),
                    ));

                    // Emit graph error event
                    self.event_bus.publish(Event::graph_error(
                        thread_id,
                        format!("Node '{}' failed: {}", current_node, e),
                    ));

                    return Err(RuntimeError::node_failed(&current_node, e));
                }
            };

            let node_duration = node_start.elapsed();

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
                    debug!(node_id = %current_node, "Node signaled finish");
                    transitions::END.to_string()
                }
                NodeOutput::Continue(Some(target)) => {
                    debug!(node_id = %current_node, target = %target, "Node specified next target");
                    target.clone()
                }
                NodeOutput::Continue(None) => {
                    let current_state = state
                        .read()
                        .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                    match self.graph.get_next_node(&current_node, &current_state) {
                        Some(next) => {
                            debug!(node_id = %current_node, next = %next, "Following graph edge");
                            next
                        }
                        None => {
                            debug!(node_id = %current_node, "No outgoing edge, ending execution");
                            transitions::END.to_string()
                        }
                    }
                }
                NodeOutput::Route(target) => {
                    debug!(node_id = %current_node, target = %target, "Node routing to target");
                    target.clone()
                }
            };

            // Emit node exited event
            self.event_bus.publish(Event::node_exited(
                thread_id,
                current_node.clone(),
                if next_node == transitions::END {
                    None
                } else {
                    Some(next_node.clone())
                },
                node_duration,
            ));

            current_node = next_node;
        }
    }

    /// Get a reference to the event bus
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// Get a reference to the underlying graph
    pub fn graph(&self) -> &CompiledGraph {
        &self.graph
    }

    /// Get a reference to the runner config
    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    async fn test_streaming_runner() {
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

        let bus = Arc::new(EventBus::new());
        let mut receiver = bus.subscribe();

        let runner = StreamingRunner::new(graph, bus);
        let result = runner.invoke("thread-1", AgentState::new()).await.unwrap();

        assert!(result.is_completed());
        assert_eq!(result.state().get_context::<i32>("count"), Some(2));

        // Verify events were emitted
        let mut event_count = 0;
        while receiver.try_recv().is_some() {
            event_count += 1;
        }

        // Should have: graph_started, node_entered x2, node_exited x2, graph_completed
        assert!(event_count >= 6);
    }

    #[tokio::test]
    async fn test_streaming_with_checkpointing() {
        let graph = GraphBuilder::new()
            .add_node(CounterNode {
                id: "node".to_string(),
                next: None,
            })
            .set_entry_point("node")
            .compile()
            .unwrap();

        let bus = Arc::new(EventBus::new());
        let checkpointer = Arc::new(MemoryCheckpointer::new());

        let runner = StreamingRunner::with_checkpointer(graph, bus, checkpointer.clone())
            .checkpoint_every_node();

        let _ = runner.invoke("thread-1", AgentState::new()).await.unwrap();

        // Verify checkpoint was saved
        let history = checkpointer.list("thread-1").await.unwrap();
        assert_eq!(history.len(), 1);
    }
}
