//! Graph execution runner for oxidizedgraph
//!
//! Provides `GraphRunner` for executing compiled graphs with configurable
//! iteration limits and execution strategies.

use std::sync::{Arc, RwLock};
use tracing::{debug, info, instrument, warn};

use crate::error::RuntimeError;
use crate::graph::{transitions, CompiledGraph, NodeOutput};
use crate::state::AgentState;

/// Configuration for the graph runner
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// Maximum number of iterations before aborting
    pub max_iterations: u32,
    /// Whether to enable detailed logging
    pub verbose: bool,
    /// Tags for categorizing this run
    pub tags: Vec<String>,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            verbose: false,
            tags: Vec::new(),
        }
    }
}

impl RunnerConfig {
    /// Create a new runner config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum iterations
    pub fn max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    /// Alias for max_iterations for LangGraph compatibility
    pub fn recursion_limit(self, limit: u32) -> Self {
        self.max_iterations(limit)
    }

    /// Enable verbose logging
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Add a tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add multiple tags
    pub fn tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags.extend(tags.into_iter().map(|t| t.into()));
        self
    }
}

/// Executor for running compiled graphs
pub struct GraphRunner {
    graph: CompiledGraph,
    config: RunnerConfig,
}

impl GraphRunner {
    /// Create a new graph runner
    pub fn new(graph: CompiledGraph, config: RunnerConfig) -> Self {
        Self { graph, config }
    }

    /// Create a new graph runner with default config
    pub fn with_defaults(graph: CompiledGraph) -> Self {
        Self::new(graph, RunnerConfig::default())
    }

    /// Execute the graph with the given initial state
    #[instrument(skip(self, initial_state), fields(graph_name = ?self.graph.name()))]
    pub async fn invoke(&self, initial_state: AgentState) -> Result<AgentState, RuntimeError> {
        let state = Arc::new(RwLock::new(initial_state));
        self.run_loop(state).await
    }

    /// Execute the graph with shared state (for advanced use cases)
    #[instrument(skip(self, state), fields(graph_name = ?self.graph.name()))]
    pub async fn invoke_shared(
        &self,
        state: Arc<RwLock<AgentState>>,
    ) -> Result<AgentState, RuntimeError> {
        self.run_loop(state).await
    }

    /// Main execution loop
    async fn run_loop(
        &self,
        state: Arc<RwLock<AgentState>>,
    ) -> Result<AgentState, RuntimeError> {
        let mut current_node = self.graph.entry_point().to_string();
        let mut iterations: u32 = 0;

        info!(
            entry_point = %current_node,
            max_iterations = self.config.max_iterations,
            "Starting graph execution"
        );

        loop {
            // Check iteration limit
            if iterations >= self.config.max_iterations {
                warn!(
                    iterations = iterations,
                    max = self.config.max_iterations,
                    "Maximum iterations exceeded"
                );
                return Err(RuntimeError::RecursionLimit(self.config.max_iterations));
            }

            // Check for END node
            if current_node == transitions::END {
                info!(iterations = iterations, "Graph execution completed");
                let guard = state
                    .read()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                return Ok(guard.clone());
            }

            // Get the current node
            let node = self
                .graph
                .get_node(&current_node)
                .ok_or_else(|| RuntimeError::NodeNotFound(current_node.clone()))?;

            debug!(node_id = %current_node, iteration = iterations, "Executing node");

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

    /// Get a reference to the underlying graph
    pub fn graph(&self) -> &CompiledGraph {
        &self.graph
    }

    /// Get a reference to the runner config
    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }
}

/// Alias for GraphRunner for LangGraph compatibility
pub type Runtime = GraphRunner;

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
    async fn test_simple_execution() {
        let graph = GraphBuilder::new()
            .add_node(CounterNode {
                id: "counter".to_string(),
                next: None,
            })
            .set_entry_point("counter")
            .compile()
            .unwrap();

        let runner = GraphRunner::with_defaults(graph);
        let result = runner.invoke(AgentState::new()).await.unwrap();

        assert_eq!(result.get_context::<i32>("count"), Some(1));
    }

    #[tokio::test]
    async fn test_chained_execution() {
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

        let runner = GraphRunner::with_defaults(graph);
        let result = runner.invoke(AgentState::new()).await.unwrap();

        assert_eq!(result.get_context::<i32>("count"), Some(3));
    }

    #[tokio::test]
    async fn test_max_iterations() {
        struct InfiniteNode;

        #[async_trait]
        impl NodeExecutor for InfiniteNode {
            fn id(&self) -> &str {
                "infinite"
            }

            async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
                Ok(NodeOutput::continue_to("infinite"))
            }
        }

        let graph = GraphBuilder::new()
            .add_node(InfiniteNode)
            .set_entry_point("infinite")
            .compile()
            .unwrap();

        let runner = GraphRunner::new(graph, RunnerConfig::default().max_iterations(5));
        let result = runner.invoke(AgentState::new()).await;

        assert!(matches!(result, Err(RuntimeError::RecursionLimit(5))));
    }

    #[tokio::test]
    async fn test_edge_based_routing() {
        struct SimpleNode {
            id: String,
        }

        #[async_trait]
        impl NodeExecutor for SimpleNode {
            fn id(&self) -> &str {
                &self.id
            }

            async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
                {
                    let mut guard = state
                        .write()
                        .map_err(|e| NodeError::execution_failed(e.to_string()))?;
                    let visited = guard.get_context::<String>("visited").unwrap_or_default();
                    guard.set_context("visited", format!("{}{}", visited, self.id));
                }
                Ok(NodeOutput::cont())
            }
        }

        let graph = GraphBuilder::new()
            .add_node(SimpleNode {
                id: "a".to_string(),
            })
            .add_node(SimpleNode {
                id: "b".to_string(),
            })
            .add_node(SimpleNode {
                id: "c".to_string(),
            })
            .set_entry_point("a")
            .add_edge("a", "b")
            .add_edge("b", "c")
            .add_edge_to_end("c")
            .compile()
            .unwrap();

        let runner = GraphRunner::with_defaults(graph);
        let result = runner.invoke(AgentState::new()).await.unwrap();

        assert_eq!(
            result.get_context::<String>("visited"),
            Some("abc".to_string())
        );
    }

    #[test]
    fn test_runner_config() {
        let config = RunnerConfig::new()
            .max_iterations(50)
            .verbose(true)
            .tag("test")
            .tag("example");

        assert_eq!(config.max_iterations, 50);
        assert!(config.verbose);
        assert_eq!(config.tags, vec!["test", "example"]);
    }
}
