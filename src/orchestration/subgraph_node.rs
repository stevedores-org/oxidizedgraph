//! Subgraph execution as a node

use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info};

use crate::error::NodeError;
use crate::graph::{CompiledGraph, NodeExecutor, NodeOutput};
use crate::runner::{GraphRunner, RunnerConfig};
use crate::state::SharedState;

use super::{clone_state, merge_all_context, ResultMerger, StateMapper};

/// A node that executes a subgraph
///
/// This node allows composing graphs hierarchically. When executed,
/// it runs the contained subgraph and optionally merges the results
/// back into the parent state.
///
/// # Example
///
/// ```rust,ignore
/// // Create a subgraph node that runs a research workflow
/// let research_node = SubgraphNode::new("research", research_graph)
///     .with_state_mapper(|parent| {
///         let mut child = AgentState::new();
///         child.set_context("query", parent.get_context::<String>("query").unwrap());
///         child
///     })
///     .with_result_merger(|parent, child| {
///         if let Some(findings) = child.get_context::<String>("findings") {
///             parent.set_context("research_findings", findings);
///         }
///     });
///
/// // Use in parent graph
/// let parent_graph = GraphBuilder::new()
///     .add_node(research_node)
///     .set_entry_point("research")
///     .compile()?;
/// ```
pub struct SubgraphNode {
    /// Node identifier
    id: String,
    /// The subgraph to execute
    graph: Arc<CompiledGraph>,
    /// Configuration for the subgraph runner
    config: RunnerConfig,
    /// Function to map parent state to child state
    state_mapper: StateMapper,
    /// Function to merge child results back into parent
    result_merger: ResultMerger,
    /// Next node to continue to after subgraph completes
    next_node: Option<String>,
}

impl SubgraphNode {
    /// Create a new subgraph node
    pub fn new(id: impl Into<String>, graph: CompiledGraph) -> Self {
        Self {
            id: id.into(),
            graph: Arc::new(graph),
            config: RunnerConfig::default(),
            state_mapper: clone_state(),
            result_merger: merge_all_context(),
            next_node: None,
        }
    }

    /// Set the runner configuration for the subgraph
    pub fn with_config(mut self, config: RunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Set a custom state mapper
    ///
    /// The mapper transforms the parent state into the initial state
    /// for the subgraph execution.
    pub fn with_state_mapper<F>(mut self, mapper: F) -> Self
    where
        F: Fn(&crate::state::AgentState) -> crate::state::AgentState + Send + Sync + 'static,
    {
        self.state_mapper = Box::new(mapper);
        self
    }

    /// Set a custom result merger
    ///
    /// The merger takes the parent state (mutable) and the completed
    /// child state, allowing you to copy relevant data back.
    pub fn with_result_merger<F>(mut self, merger: F) -> Self
    where
        F: Fn(&mut crate::state::AgentState, crate::state::AgentState) + Send + Sync + 'static,
    {
        self.result_merger = Box::new(merger);
        self
    }

    /// Set the next node to continue to after subgraph completes
    pub fn then(mut self, next_node: impl Into<String>) -> Self {
        self.next_node = Some(next_node.into());
        self
    }

    /// Set to finish after subgraph completes
    pub fn then_finish(mut self) -> Self {
        self.next_node = None;
        self
    }
}

#[async_trait]
impl NodeExecutor for SubgraphNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        // Map parent state to child state
        let child_state = {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            (self.state_mapper)(&guard)
        };

        info!(
            node_id = %self.id,
            subgraph_name = ?self.graph.name(),
            "Executing subgraph"
        );

        // Run the subgraph
        let runner = GraphRunner::new((*self.graph).clone(), self.config.clone());
        let child_result = runner.invoke(child_state).await.map_err(|e| {
            NodeError::execution_failed(format!("Subgraph '{}' failed: {}", self.id, e))
        })?;

        debug!(
            node_id = %self.id,
            iterations = child_result.iteration,
            "Subgraph completed"
        );

        // Merge results back into parent state
        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            (self.result_merger)(&mut guard, child_result);
        }

        // Return next node or finish
        match &self.next_node {
            Some(next) => Ok(NodeOutput::continue_to(next.clone())),
            None => Ok(NodeOutput::finish()),
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Executes a subgraph")
    }
}

/// Builder for creating subgraph nodes with fluent API
#[allow(dead_code)]
pub struct SubgraphNodeBuilder {
    id: String,
    graph: Option<CompiledGraph>,
    config: RunnerConfig,
    state_mapper: Option<StateMapper>,
    result_merger: Option<ResultMerger>,
    next_node: Option<String>,
}

#[allow(dead_code)]
impl SubgraphNodeBuilder {
    /// Create a new builder
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            graph: None,
            config: RunnerConfig::default(),
            state_mapper: None,
            result_merger: None,
            next_node: None,
        }
    }

    /// Set the subgraph
    pub fn graph(mut self, graph: CompiledGraph) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Set the runner config
    pub fn config(mut self, config: RunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the state mapper
    pub fn state_mapper<F>(mut self, mapper: F) -> Self
    where
        F: Fn(&crate::state::AgentState) -> crate::state::AgentState + Send + Sync + 'static,
    {
        self.state_mapper = Some(Box::new(mapper));
        self
    }

    /// Set the result merger
    pub fn result_merger<F>(mut self, merger: F) -> Self
    where
        F: Fn(&mut crate::state::AgentState, crate::state::AgentState) + Send + Sync + 'static,
    {
        self.result_merger = Some(Box::new(merger));
        self
    }

    /// Set the next node
    pub fn then(mut self, next_node: impl Into<String>) -> Self {
        self.next_node = Some(next_node.into());
        self
    }

    /// Build the subgraph node
    pub fn build(self) -> Result<SubgraphNode, &'static str> {
        let graph = self.graph.ok_or("Graph is required")?;

        let mut node = SubgraphNode::new(self.id, graph).with_config(self.config);

        if let Some(mapper) = self.state_mapper {
            node.state_mapper = mapper;
        }

        if let Some(merger) = self.result_merger {
            node.result_merger = merger;
        }

        node.next_node = self.next_node;

        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GraphBuilder;
    use crate::state::AgentState;
    use std::sync::{Arc, RwLock};

    struct SetValueNode {
        id: String,
        key: String,
        value: String,
    }

    #[async_trait]
    impl NodeExecutor for SetValueNode {
        fn id(&self) -> &str {
            &self.id
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            {
                let mut guard = state
                    .write()
                    .map_err(|e| NodeError::execution_failed(e.to_string()))?;
                guard.set_context(&self.key, self.value.clone());
            }
            Ok(NodeOutput::finish())
        }
    }

    #[tokio::test]
    async fn test_subgraph_node_basic() {
        // Create a simple subgraph
        let subgraph = GraphBuilder::new()
            .add_node(SetValueNode {
                id: "set".to_string(),
                key: "child_result".to_string(),
                value: "from_child".to_string(),
            })
            .set_entry_point("set")
            .compile()
            .unwrap();

        // Create the subgraph node
        let node = SubgraphNode::new("subgraph", subgraph);

        // Execute
        let state = Arc::new(RwLock::new(AgentState::new()));
        let result = node.execute(state.clone()).await.unwrap();

        assert!(result.is_terminal());

        // Check that child result was merged
        let guard = state.read().unwrap();
        assert_eq!(
            guard.get_context::<String>("child_result"),
            Some("from_child".to_string())
        );
    }

    #[tokio::test]
    async fn test_subgraph_node_with_mapper() {
        // Subgraph that reads "input" and writes to "output"
        struct ProcessNode;

        #[async_trait]
        impl NodeExecutor for ProcessNode {
            fn id(&self) -> &str {
                "process"
            }

            async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
                let input: String = {
                    let guard = state
                        .read()
                        .map_err(|e| NodeError::execution_failed(e.to_string()))?;
                    guard.get_context("input").unwrap_or_default()
                };

                {
                    let mut guard = state
                        .write()
                        .map_err(|e| NodeError::execution_failed(e.to_string()))?;
                    guard.set_context("output", format!("processed: {}", input));
                }

                Ok(NodeOutput::finish())
            }
        }

        let subgraph = GraphBuilder::new()
            .add_node(ProcessNode)
            .set_entry_point("process")
            .compile()
            .unwrap();

        let node = SubgraphNode::new("subgraph", subgraph)
            .with_state_mapper(|parent| {
                let mut child = AgentState::new();
                if let Some(data) = parent.get_context::<String>("data") {
                    child.set_context("input", data);
                }
                child
            })
            .with_result_merger(|parent, child| {
                if let Some(output) = child.get_context::<String>("output") {
                    parent.set_context("result", output);
                }
            });

        // Set up parent state
        let state = Arc::new(RwLock::new(AgentState::new()));
        {
            let mut guard = state.write().unwrap();
            guard.set_context("data", "hello".to_string());
        }

        // Execute
        node.execute(state.clone()).await.unwrap();

        // Check result
        let guard = state.read().unwrap();
        assert_eq!(
            guard.get_context::<String>("result"),
            Some("processed: hello".to_string())
        );
    }
}
