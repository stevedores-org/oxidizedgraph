//! Dynamic subgraph spawner

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

use crate::events::EventBus;
use crate::graph::CompiledGraph;
use crate::runner::{GraphRunner, RunnerConfig};
use crate::state::AgentState;

use super::handle::{SubgraphHandle, SubgraphResult};
use super::StateMapper;

/// Spawner for dynamically creating subgraph executions
///
/// The spawner manages the lifecycle of spawned subgraphs and can track
/// their progress. It's useful for dynamic workflows where the number
/// and type of subgraphs isn't known at graph construction time.
///
/// # Example
///
/// ```rust,ignore
/// let spawner = SubgraphSpawner::new();
///
/// // Spawn multiple subgraphs
/// let handle1 = spawner.spawn("research-1", research_graph.clone(), state1).await?;
/// let handle2 = spawner.spawn("research-2", research_graph.clone(), state2).await?;
///
/// // Wait for all to complete
/// let results = spawner.join_all().await;
/// ```
pub struct SubgraphSpawner {
    /// Configuration for spawned runners
    config: RunnerConfig,
    /// Optional event bus for spawned graphs
    event_bus: Option<Arc<EventBus>>,
    /// Active subgraph handles (by ID)
    active: Arc<RwLock<HashMap<String, ()>>>,
    /// Counter for generating unique IDs
    counter: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for SubgraphSpawner {
    fn default() -> Self {
        Self::new()
    }
}

impl SubgraphSpawner {
    /// Create a new subgraph spawner
    pub fn new() -> Self {
        Self {
            config: RunnerConfig::default(),
            event_bus: None,
            active: Arc::new(RwLock::new(HashMap::new())),
            counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Set the runner configuration for spawned subgraphs
    pub fn with_config(mut self, config: RunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Set an event bus for spawned subgraphs to emit events to
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Generate a unique subgraph ID
    pub fn generate_id(&self, prefix: &str) -> String {
        let n = self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("{}-{}", prefix, n)
    }

    /// Spawn a subgraph with the given ID and initial state
    pub fn spawn(
        &self,
        subgraph_id: impl Into<String>,
        graph: CompiledGraph,
        initial_state: AgentState,
    ) -> SubgraphHandle {
        let subgraph_id = subgraph_id.into();
        let config = self.config.clone();
        let active = self.active.clone();

        // Track as active
        if let Ok(mut map) = active.write() {
            map.insert(subgraph_id.clone(), ());
        }

        let id_clone = subgraph_id.clone();
        let active_clone = active.clone();

        info!(subgraph_id = %subgraph_id, "Spawning subgraph");

        let handle = tokio::spawn(async move {
            let runner = GraphRunner::new(graph, config);
            let result = runner.invoke(initial_state).await;

            // Remove from active
            if let Ok(mut map) = active_clone.write() {
                map.remove(&id_clone);
            }

            match result {
                Ok(state) => {
                    debug!(subgraph_id = %id_clone, "Subgraph completed");
                    SubgraphResult::Completed {
                        subgraph_id: id_clone,
                        state,
                    }
                }
                Err(error) => {
                    debug!(subgraph_id = %id_clone, error = %error, "Subgraph failed");
                    SubgraphResult::Failed {
                        subgraph_id: id_clone,
                        error,
                    }
                }
            }
        });

        SubgraphHandle::new(subgraph_id, handle)
    }

    /// Spawn a subgraph with a state mapper applied to parent state
    pub fn spawn_with_mapper(
        &self,
        subgraph_id: impl Into<String>,
        graph: CompiledGraph,
        parent_state: &AgentState,
        mapper: &StateMapper,
    ) -> SubgraphHandle {
        let child_state = mapper(parent_state);
        self.spawn(subgraph_id, graph, child_state)
    }

    /// Get the number of currently active subgraphs
    pub fn active_count(&self) -> usize {
        self.active
            .read()
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// Check if any subgraphs are still running
    pub fn has_active(&self) -> bool {
        self.active_count() > 0
    }

    /// Get the IDs of all active subgraphs
    pub fn active_ids(&self) -> Vec<String> {
        self.active
            .read()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// Builder for spawning multiple subgraphs and collecting results
pub struct SpawnBuilder<'a> {
    spawner: &'a SubgraphSpawner,
    handles: Vec<SubgraphHandle>,
}

impl<'a> SpawnBuilder<'a> {
    /// Create a new spawn builder
    pub fn new(spawner: &'a SubgraphSpawner) -> Self {
        Self {
            spawner,
            handles: Vec::new(),
        }
    }

    /// Spawn a subgraph and add to the builder
    pub fn spawn(
        mut self,
        subgraph_id: impl Into<String>,
        graph: CompiledGraph,
        initial_state: AgentState,
    ) -> Self {
        let handle = self.spawner.spawn(subgraph_id, graph, initial_state);
        self.handles.push(handle);
        self
    }

    /// Spawn a subgraph with a state mapper
    pub fn spawn_with_mapper(
        mut self,
        subgraph_id: impl Into<String>,
        graph: CompiledGraph,
        parent_state: &AgentState,
        mapper: &StateMapper,
    ) -> Self {
        let handle = self.spawner.spawn_with_mapper(subgraph_id, graph, parent_state, mapper);
        self.handles.push(handle);
        self
    }

    /// Wait for all spawned subgraphs to complete
    pub async fn join_all(self) -> Vec<SubgraphResult> {
        let mut results = Vec::with_capacity(self.handles.len());
        for handle in self.handles {
            results.push(handle.join().await);
        }
        results
    }

    /// Wait for the first subgraph to complete (race)
    pub async fn join_first(self) -> Option<SubgraphResult> {
        if self.handles.is_empty() {
            return None;
        }

        // Convert to pinned futures we can select on
        let futures: Vec<_> = self.handles
            .into_iter()
            .map(|h| Box::pin(h.join()))
            .collect();

        let (result, _, _remaining) = futures::future::select_all(futures).await;

        // Note: Remaining futures will continue running but their results will be ignored.
        // They'll be cleaned up when the futures are dropped.

        Some(result)
    }
}

impl SubgraphSpawner {
    /// Create a builder for spawning multiple subgraphs
    pub fn builder(&self) -> SpawnBuilder<'_> {
        SpawnBuilder::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NodeError;
    use crate::graph::{GraphBuilder, NodeExecutor, NodeOutput};
    use crate::state::SharedState;
    use async_trait::async_trait;

    struct SimpleNode {
        id: String,
        value: String,
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
                guard.set_context("result", self.value.clone());
            }
            Ok(NodeOutput::finish())
        }
    }

    fn create_test_graph(value: &str) -> CompiledGraph {
        GraphBuilder::new()
            .add_node(SimpleNode {
                id: "node".to_string(),
                value: value.to_string(),
            })
            .set_entry_point("node")
            .compile()
            .unwrap()
    }

    #[tokio::test]
    async fn test_spawn_single() {
        let spawner = SubgraphSpawner::new();
        let graph = create_test_graph("hello");

        let handle = spawner.spawn("test-1", graph, AgentState::new());
        let result = handle.await;

        assert!(result.is_completed());
        let state = result.state().unwrap();
        assert_eq!(state.get_context::<String>("result"), Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_spawn_multiple() {
        let spawner = SubgraphSpawner::new();

        let results = spawner
            .builder()
            .spawn("sub-1", create_test_graph("one"), AgentState::new())
            .spawn("sub-2", create_test_graph("two"), AgentState::new())
            .spawn("sub-3", create_test_graph("three"), AgentState::new())
            .join_all()
            .await;

        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_completed()));
    }

    #[tokio::test]
    async fn test_generate_id() {
        let spawner = SubgraphSpawner::new();

        let id1 = spawner.generate_id("test");
        let id2 = spawner.generate_id("test");

        assert_ne!(id1, id2);
        assert!(id1.starts_with("test-"));
        assert!(id2.starts_with("test-"));
    }
}
