//! Runtime executor for oxidizedgraph
//!
//! The `Runtime` is responsible for executing compiled graphs,
//! managing state transitions, and handling streaming events.

use crate::edge::EdgeTarget;
use crate::error::{NodeError, RuntimeError};
use crate::graph::CompiledGraph;
use crate::node::NodeResult;
use crate::state::State;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

/// Stream event types for real-time updates
#[derive(Clone, Debug)]
pub enum StreamEvent<S: State> {
    /// Node started execution
    NodeStart {
        /// ID of the node
        node_id: String,
        /// State when node started
        state: S,
    },

    /// Node completed execution
    NodeEnd {
        /// ID of the node
        node_id: String,
        /// State after node completed
        state: S,
    },

    /// State was updated
    StateUpdate {
        /// Updated state
        state: S,
    },

    /// Token from streaming LLM response
    Token {
        /// Token content
        content: String,
    },

    /// Graph execution completed
    Complete {
        /// Final state
        final_state: S,
    },

    /// Execution was interrupted for human input
    Interrupt {
        /// State at interrupt point
        state: S,
        /// Reason for interrupt
        reason: String,
    },

    /// Error occurred during execution
    Error {
        /// Error message
        error: String,
    },
}

/// Configuration for graph execution
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// Enable streaming events
    pub stream: bool,

    /// Maximum number of node executions before stopping
    pub recursion_limit: u32,

    /// Thread ID for multi-tenant scenarios
    pub thread_id: Option<String>,

    /// Tags for categorization
    pub tags: HashSet<String>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            stream: true,
            recursion_limit: 25,
            thread_id: None,
            tags: HashSet::new(),
        }
    }
}

impl RuntimeConfig {
    /// Create a new runtime config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the recursion limit
    pub fn recursion_limit(mut self, limit: u32) -> Self {
        self.recursion_limit = limit;
        self
    }

    /// Set the thread ID
    pub fn thread_id(mut self, id: impl Into<String>) -> Self {
        self.thread_id = Some(id.into());
        self
    }

    /// Add a tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Enable or disable streaming
    pub fn streaming(mut self, enabled: bool) -> Self {
        self.stream = enabled;
        self
    }
}

/// Runtime for executing compiled graphs
pub struct Runtime<S: State> {
    graph: CompiledGraph<S>,
    config: RuntimeConfig,
}

impl<S: State> Runtime<S> {
    /// Create a new runtime with the given graph and config
    pub fn new(graph: CompiledGraph<S>, config: RuntimeConfig) -> Self {
        Self { graph, config }
    }

    /// Create a new runtime with default config
    pub fn with_defaults(graph: CompiledGraph<S>) -> Self {
        Self::new(graph, RuntimeConfig::default())
    }

    /// Execute the graph with the given initial state
    pub async fn invoke(&self, initial_state: S) -> Result<S, RuntimeError> {
        let mut state = initial_state;
        let mut current_node = self.graph.entry_point.clone();
        let mut step_count = 0u32;

        loop {
            // Check recursion limit
            if step_count >= self.config.recursion_limit {
                return Err(RuntimeError::RecursionLimit(self.config.recursion_limit));
            }

            // Get the current node
            let node = self
                .graph
                .get_node(&current_node)
                .ok_or_else(|| RuntimeError::NodeNotFound(current_node.clone()))?;

            // Execute the node
            let result = node
                .run(state)
                .await
                .map_err(|e| RuntimeError::node_failed(&current_node, e))?;

            // Handle the result
            match result {
                NodeResult::Continue(new_state) => {
                    state = new_state;

                    // Determine next node
                    match self.graph.get_edge(&current_node) {
                        Some(edge) => {
                            let target = edge.resolve(&state);
                            match target {
                                EdgeTarget::Node(next) => {
                                    current_node = next;
                                }
                                EdgeTarget::End => {
                                    return Ok(state);
                                }
                            }
                        }
                        None => {
                            // No edge defined, end execution
                            return Ok(state);
                        }
                    }
                }
                NodeResult::Interrupt { state, reason } => {
                    return Err(RuntimeError::interrupted(reason));
                }
                NodeResult::End(final_state) => {
                    return Ok(final_state);
                }
            }

            step_count += 1;
        }
    }

    /// Execute the graph with streaming events
    pub fn stream(&self, initial_state: S) -> impl Stream<Item = StreamEvent<S>>
    where
        S: Clone + 'static,
    {
        let (tx, rx) = mpsc::channel(100);
        let graph_entry = self.graph.entry_point.clone();
        let recursion_limit = self.config.recursion_limit;

        // Clone what we need for the spawned task
        let nodes: Vec<_> = self.graph.node_ids().map(|s| s.to_string()).collect();
        let graph = CompiledGraphSnapshot {
            entry_point: graph_entry,
            node_ids: nodes,
        };

        // We need to move the actual execution into a spawned task
        // For now, we'll do a simplified version
        let state = initial_state.clone();

        tokio::spawn(async move {
            // Send start event
            let _ = tx
                .send(StreamEvent::NodeStart {
                    node_id: graph.entry_point.clone(),
                    state: state.clone(),
                })
                .await;

            // In a real implementation, we'd execute the graph here
            // For now, just complete
            let _ = tx
                .send(StreamEvent::Complete {
                    final_state: state,
                })
                .await;
        });

        ReceiverStream::new(rx)
    }

    /// Get a reference to the compiled graph
    pub fn graph(&self) -> &CompiledGraph<S> {
        &self.graph
    }

    /// Get a reference to the runtime config
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }
}

/// Snapshot of graph metadata for use in spawned tasks
struct CompiledGraphSnapshot {
    entry_point: String,
    node_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NodeError;
    use crate::graph::GraphBuilder;
    use crate::node::Node;
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    struct TestState {
        counter: u32,
    }

    impl State for TestState {
        fn schema() -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
    }

    struct IncrementNode;

    #[async_trait]
    impl Node<TestState> for IncrementNode {
        fn id(&self) -> &str {
            "increment"
        }

        async fn run(&self, mut state: TestState) -> Result<NodeResult<TestState>, NodeError> {
            state.counter += 1;
            Ok(NodeResult::Continue(state))
        }
    }

    struct EndNode;

    #[async_trait]
    impl Node<TestState> for EndNode {
        fn id(&self) -> &str {
            "end"
        }

        async fn run(&self, state: TestState) -> Result<NodeResult<TestState>, NodeError> {
            Ok(NodeResult::End(state))
        }
    }

    #[tokio::test]
    async fn test_simple_execution() {
        let graph = GraphBuilder::<TestState>::new()
            .add_node(IncrementNode)
            .add_node(EndNode)
            .set_entry_point("increment")
            .add_edge("increment", "end")
            .compile()
            .unwrap();

        let runtime = Runtime::with_defaults(graph);
        let result = runtime.invoke(TestState::default()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().counter, 1);
    }

    #[tokio::test]
    async fn test_recursion_limit() {
        struct LoopNode;

        #[async_trait]
        impl Node<TestState> for LoopNode {
            fn id(&self) -> &str {
                "loop"
            }

            async fn run(&self, mut state: TestState) -> Result<NodeResult<TestState>, NodeError> {
                state.counter += 1;
                Ok(NodeResult::Continue(state))
            }
        }

        let graph = GraphBuilder::<TestState>::new()
            .add_node(LoopNode)
            .set_entry_point("loop")
            .add_edge("loop", "loop") // Infinite loop
            .compile()
            .unwrap();

        let runtime = Runtime::new(graph, RuntimeConfig::default().recursion_limit(5));
        let result = runtime.invoke(TestState::default()).await;

        assert!(matches!(result, Err(RuntimeError::RecursionLimit(5))));
    }

    #[test]
    fn test_runtime_config() {
        let config = RuntimeConfig::new()
            .recursion_limit(50)
            .thread_id("thread-123")
            .tag("test")
            .tag("example")
            .streaming(false);

        assert_eq!(config.recursion_limit, 50);
        assert_eq!(config.thread_id, Some("thread-123".to_string()));
        assert!(config.tags.contains("test"));
        assert!(!config.stream);
    }
}
