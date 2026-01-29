//! Parallel subgraph execution

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::error::NodeError;
use crate::graph::{CompiledGraph, NodeExecutor, NodeOutput};
use crate::runner::{GraphRunner, RunnerConfig};
use crate::state::{AgentState, SharedState};

use super::handle::SubgraphResult;
use super::{clone_state, ResultMerger, StateMapper};

/// Strategy for joining parallel subgraph results
#[derive(Clone, Debug, Default)]
pub enum JoinStrategy {
    /// Wait for all subgraphs to complete (default)
    #[default]
    WaitAll,
    /// Return as soon as the first subgraph completes
    WaitFirst,
    /// Return as soon as any subgraph fails (fail-fast)
    FailFast,
    /// Wait for a specific number of subgraphs to complete
    WaitN(usize),
}

/// Configuration for a single subgraph in parallel execution
struct SubgraphConfig {
    /// Subgraph identifier
    id: String,
    /// The compiled graph
    graph: Arc<CompiledGraph>,
    /// State mapper for this subgraph
    state_mapper: StateMapper,
    /// Result merger for this subgraph
    result_merger: ResultMerger,
}

/// A node that executes multiple subgraphs in parallel
///
/// This is useful for fan-out patterns where you need to run multiple
/// independent workflows concurrently and aggregate their results.
///
/// # Example
///
/// ```rust,ignore
/// let parallel = ParallelSubgraphs::new("multi_search")
///     .add_subgraph("web", web_search_graph)
///     .add_subgraph("docs", doc_search_graph)
///     .add_subgraph("code", code_search_graph)
///     .with_join_strategy(JoinStrategy::WaitAll)
///     .with_default_merger(|parent, child| {
///         // Each child's results are stored under its subgraph ID
///         parent.set_context(&format!("{}_results", child_id), child.context);
///     });
/// ```
pub struct ParallelSubgraphs {
    /// Node identifier
    id: String,
    /// Subgraph configurations
    subgraphs: Vec<SubgraphConfig>,
    /// Runner configuration for subgraphs
    config: RunnerConfig,
    /// Join strategy
    join_strategy: JoinStrategy,
    /// Default state mapper (if not specified per-subgraph)
    #[allow(dead_code)]
    default_state_mapper: StateMapper,
    /// Default result merger (if not specified per-subgraph)
    #[allow(dead_code)]
    default_result_merger: ResultMerger,
    /// Next node to continue to
    next_node: Option<String>,
}

impl ParallelSubgraphs {
    /// Create a new parallel subgraphs node
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            subgraphs: Vec::new(),
            config: RunnerConfig::default(),
            join_strategy: JoinStrategy::default(),
            default_state_mapper: clone_state(),
            default_result_merger: Box::new(|parent, child| {
                // Default: merge all context from child
                for (key, value) in child.context {
                    parent.context.insert(key, value);
                }
            }),
            next_node: None,
        }
    }

    /// Add a subgraph to execute in parallel
    pub fn add_subgraph(mut self, id: impl Into<String>, graph: CompiledGraph) -> Self {
        let id = id.into();
        let default_merger = self.create_namespace_merger(&id);

        self.subgraphs.push(SubgraphConfig {
            id,
            graph: Arc::new(graph),
            state_mapper: clone_state(),
            result_merger: default_merger,
        });
        self
    }

    /// Add a subgraph with custom state mapper and result merger
    pub fn add_subgraph_with_handlers<SM, RM>(
        mut self,
        id: impl Into<String>,
        graph: CompiledGraph,
        state_mapper: SM,
        result_merger: RM,
    ) -> Self
    where
        SM: Fn(&AgentState) -> AgentState + Send + Sync + 'static,
        RM: Fn(&mut AgentState, AgentState) + Send + Sync + 'static,
    {
        self.subgraphs.push(SubgraphConfig {
            id: id.into(),
            graph: Arc::new(graph),
            state_mapper: Box::new(state_mapper),
            result_merger: Box::new(result_merger),
        });
        self
    }

    /// Set the join strategy
    pub fn with_join_strategy(mut self, strategy: JoinStrategy) -> Self {
        self.join_strategy = strategy;
        self
    }

    /// Set the runner configuration for subgraphs
    pub fn with_config(mut self, config: RunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the default state mapper for subgraphs
    pub fn with_default_state_mapper<F>(mut self, mapper: F) -> Self
    where
        F: Fn(&AgentState) -> AgentState + Send + Sync + 'static,
    {
        self.default_state_mapper = Box::new(mapper);
        self
    }

    /// Set the next node to continue to
    pub fn then(mut self, next_node: impl Into<String>) -> Self {
        self.next_node = Some(next_node.into());
        self
    }

    /// Create a merger that stores results under a namespace
    fn create_namespace_merger(&self, namespace: &str) -> ResultMerger {
        let ns = namespace.to_string();
        Box::new(move |parent, child| {
            if let Ok(child_json) = serde_json::to_value(&child.context) {
                parent.context.insert(ns.clone(), child_json);
            }
        })
    }

    /// Execute all subgraphs and return results
    async fn execute_all(&self, parent_state: &AgentState) -> Vec<(String, SubgraphResult)> {
        let mut handles = Vec::with_capacity(self.subgraphs.len());

        // Spawn all subgraphs
        for subgraph_config in &self.subgraphs {
            let child_state = (subgraph_config.state_mapper)(parent_state);
            let graph = subgraph_config.graph.clone();
            let config = self.config.clone();
            let subgraph_id = subgraph_config.id.clone();

            let handle = tokio::spawn(async move {
                let runner = GraphRunner::new((*graph).clone(), config);
                match runner.invoke(child_state).await {
                    Ok(state) => SubgraphResult::Completed {
                        subgraph_id,
                        state,
                    },
                    Err(error) => SubgraphResult::Failed {
                        subgraph_id,
                        error,
                    },
                }
            });

            handles.push((subgraph_config.id.clone(), handle));
        }

        // Collect results based on join strategy
        match &self.join_strategy {
            JoinStrategy::WaitAll => {
                let mut results = Vec::with_capacity(handles.len());
                for (id, handle) in handles {
                    let result = handle.await.unwrap_or_else(|e| SubgraphResult::Failed {
                        subgraph_id: id.clone(),
                        error: crate::error::RuntimeError::InvalidState(format!(
                            "Task panicked: {}",
                            e
                        )),
                    });
                    results.push((id, result));
                }
                results
            }
            JoinStrategy::WaitFirst => {
                if handles.is_empty() {
                    return Vec::new();
                }

                // Race all handles using boxed futures for Unpin
                let futures: Vec<_> = handles
                    .into_iter()
                    .map(|(id, h)| {
                        let id_clone = id.clone();
                        Box::pin(async move {
                            let result = h.await.unwrap_or_else(|e| SubgraphResult::Failed {
                                subgraph_id: id_clone.clone(),
                                error: crate::error::RuntimeError::InvalidState(format!(
                                    "Task panicked: {}",
                                    e
                                )),
                            });
                            (id_clone, result)
                        })
                    })
                    .collect();

                let (result, _, _) = futures::future::select_all(futures).await;
                vec![result]
            }
            JoinStrategy::FailFast => {
                let mut results = Vec::new();
                for (id, handle) in handles {
                    let result = handle.await.unwrap_or_else(|e| SubgraphResult::Failed {
                        subgraph_id: id.clone(),
                        error: crate::error::RuntimeError::InvalidState(format!(
                            "Task panicked: {}",
                            e
                        )),
                    });
                    let is_failed = result.is_failed();
                    results.push((id, result));
                    if is_failed {
                        break;
                    }
                }
                results
            }
            JoinStrategy::WaitN(n) => {
                let mut results = Vec::new();
                let mut completed = 0;
                for (id, handle) in handles {
                    if completed >= *n {
                        break;
                    }
                    let result = handle.await.unwrap_or_else(|e| SubgraphResult::Failed {
                        subgraph_id: id.clone(),
                        error: crate::error::RuntimeError::InvalidState(format!(
                            "Task panicked: {}",
                            e
                        )),
                    });
                    if result.is_completed() {
                        completed += 1;
                    }
                    results.push((id, result));
                }
                results
            }
        }
    }
}

#[async_trait]
impl NodeExecutor for ParallelSubgraphs {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        if self.subgraphs.is_empty() {
            warn!(node_id = %self.id, "No subgraphs configured");
            return match &self.next_node {
                Some(next) => Ok(NodeOutput::continue_to(next.clone())),
                None => Ok(NodeOutput::finish()),
            };
        }

        // Get parent state
        let parent_state = {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.clone()
        };

        info!(
            node_id = %self.id,
            subgraph_count = self.subgraphs.len(),
            strategy = ?self.join_strategy,
            "Executing parallel subgraphs"
        );

        // Execute all subgraphs
        let results = self.execute_all(&parent_state).await;

        debug!(
            node_id = %self.id,
            completed = results.iter().filter(|(_, r)| r.is_completed()).count(),
            failed = results.iter().filter(|(_, r)| r.is_failed()).count(),
            "Parallel subgraphs completed"
        );

        // Create a map of subgraph configs by ID for merger lookup
        let config_map: HashMap<&str, &SubgraphConfig> = self
            .subgraphs
            .iter()
            .map(|c| (c.id.as_str(), c))
            .collect();

        // Merge results back into parent state
        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;

            for (id, result) in results {
                if let SubgraphResult::Completed { state: child_state, .. } = result {
                    if let Some(config) = config_map.get(id.as_str()) {
                        (config.result_merger)(&mut guard, child_state);
                    }
                }
            }
        }

        match &self.next_node {
            Some(next) => Ok(NodeOutput::continue_to(next.clone())),
            None => Ok(NodeOutput::finish()),
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Executes multiple subgraphs in parallel")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GraphBuilder;
    use std::sync::RwLock;
    use std::time::Duration;

    struct DelayedSetNode {
        id: String,
        key: String,
        value: String,
        delay_ms: u64,
    }

    #[async_trait]
    impl NodeExecutor for DelayedSetNode {
        fn id(&self) -> &str {
            &self.id
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            {
                let mut guard = state
                    .write()
                    .map_err(|e| NodeError::execution_failed(e.to_string()))?;
                guard.set_context(&self.key, self.value.clone());
            }
            Ok(NodeOutput::finish())
        }
    }

    fn create_delayed_graph(id: &str, key: &str, value: &str, delay_ms: u64) -> CompiledGraph {
        GraphBuilder::new()
            .add_node(DelayedSetNode {
                id: id.to_string(),
                key: key.to_string(),
                value: value.to_string(),
                delay_ms,
            })
            .set_entry_point(id)
            .compile()
            .unwrap()
    }

    #[tokio::test]
    async fn test_parallel_subgraphs_wait_all() {
        let parallel = ParallelSubgraphs::new("parallel")
            .add_subgraph("fast", create_delayed_graph("fast", "fast_result", "fast_value", 10))
            .add_subgraph("slow", create_delayed_graph("slow", "slow_result", "slow_value", 50))
            .with_join_strategy(JoinStrategy::WaitAll);

        let state = Arc::new(RwLock::new(AgentState::new()));
        let result = parallel.execute(state.clone()).await.unwrap();

        assert!(result.is_terminal());

        // Both results should be present
        let guard = state.read().unwrap();
        assert!(guard.context.contains_key("fast"));
        assert!(guard.context.contains_key("slow"));
    }

    #[tokio::test]
    async fn test_parallel_empty() {
        let parallel = ParallelSubgraphs::new("parallel");

        let state = Arc::new(RwLock::new(AgentState::new()));
        let result = parallel.execute(state).await.unwrap();

        assert!(result.is_terminal());
    }
}
