//! Graph building and compilation for oxidizedgraph
//!
//! Provides the core graph primitives: `NodeExecutor` trait, `GraphBuilder`,
//! and `CompiledGraph` for building and executing agent workflows.

use async_trait::async_trait;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::error::{GraphError, NodeError};
use crate::state::{AgentState, SharedState};

/// Transition constants for graph execution
pub mod transitions {
    /// Continue to the next node based on edges
    pub const CONTINUE: &str = "__continue__";
    /// Finish execution and return the final state
    pub const FINISH: &str = "__finish__";
    /// End node marker
    pub const END: &str = "__end__";
    /// Start node marker
    pub const START: &str = "__start__";
}

/// The result of a node execution
#[derive(Clone, Debug)]
pub enum NodeOutput {
    /// Continue execution, optionally specifying the next node
    Continue(Option<String>),
    /// Finish execution with the current state
    Finish,
    /// Route to a specific node
    Route(String),
    /// Follow an outgoing edge with the specified transition key
    Transition(String),
}

impl NodeOutput {
    /// Create a continue output
    pub fn cont() -> Self {
        Self::Continue(None)
    }

    /// Create a continue output with a specific next node
    pub fn continue_to(next: impl Into<String>) -> Self {
        Self::Continue(Some(next.into()))
    }

    /// Create a finish output
    pub fn finish() -> Self {
        Self::Finish
    }

    /// Alias for `finish()` - ends graph execution
    pub fn end() -> Self {
        Self::Finish
    }

    /// Create a route output to a specific node
    pub fn route(target: impl Into<String>) -> Self {
        Self::Route(target.into())
    }

    /// Create a transition output that follows a keyed graph edge
    pub fn transition(key: impl Into<String>) -> Self {
        Self::Transition(key.into())
    }

    /// Check if this output signals completion
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Finish)
    }

    /// Get the target node name if this is a routing output
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::Continue(Some(s)) | Self::Route(s) | Self::Transition(s) => Some(s),
            _ => None,
        }
    }
}

/// Trait for executable nodes in the graph
///
/// Implement this trait to create custom nodes that can be added to a graph.
/// Each node receives shared state and returns an output indicating the next step.
#[async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Unique identifier for this node
    fn id(&self) -> &str;

    /// Execute the node logic
    ///
    /// # Arguments
    /// * `state` - Shared state that can be read and modified
    ///
    /// # Returns
    /// * `Result<NodeOutput, NodeError>` - The output determining the next step
    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError>;

    /// Optional description of what this node does
    fn description(&self) -> Option<&str> {
        None
    }
}

/// Type alias for a boxed node executor
pub type BoxedNodeExecutor = Arc<dyn NodeExecutor>;

/// A node in the graph with its executor
#[derive(Clone)]
pub struct GraphNode {
    /// The node's unique identifier
    pub id: String,
    /// The executor for this node
    pub executor: BoxedNodeExecutor,
}

impl GraphNode {
    /// Create a new graph node
    pub fn new(executor: impl NodeExecutor + 'static) -> Self {
        Self {
            id: executor.id().to_string(),
            executor: Arc::new(executor),
        }
    }
}

impl fmt::Debug for GraphNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphNode").field("id", &self.id).finish()
    }
}

/// Type of edge in the graph
#[derive(Clone)]
pub enum EdgeType {
    /// Direct edge to another node
    Direct,
    /// Conditional edge with a router function
    Conditional(Arc<dyn Fn(&AgentState) -> String + Send + Sync>),
}

impl fmt::Debug for EdgeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EdgeType::Direct => write!(f, "Direct"),
            EdgeType::Conditional(_) => write!(f, "Conditional(<fn>)"),
        }
    }
}

/// An edge connecting two nodes in the graph
#[derive(Clone)]
pub struct GraphEdge {
    /// Source node ID
    pub from: String,
    /// Target node ID (or END constant)
    pub to: String,
    /// Transition key required to follow this edge.
    ///
    /// When `None`, this edge is treated as a default path for
    /// `transitions::CONTINUE` for backwards compatibility.
    pub transition_key: Option<String>,
    /// Type of edge
    pub edge_type: EdgeType,
}

impl GraphEdge {
    /// Create a new direct edge
    pub fn direct(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            transition_key: None,
            edge_type: EdgeType::Direct,
        }
    }

    /// Create a new direct edge with an explicit transition key.
    pub fn direct_with_key(
        from: impl Into<String>,
        to: impl Into<String>,
        transition_key: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            transition_key: Some(transition_key.into()),
            edge_type: EdgeType::Direct,
        }
    }

    /// Create a new conditional edge
    pub fn conditional<F>(from: impl Into<String>, router: F) -> Self
    where
        F: Fn(&AgentState) -> String + Send + Sync + 'static,
    {
        Self {
            from: from.into(),
            to: String::new(), // Determined at runtime
            transition_key: None,
            edge_type: EdgeType::Conditional(Arc::new(router)),
        }
    }

    /// Create a new conditional edge with an explicit transition key.
    pub fn conditional_with_key<F>(
        from: impl Into<String>,
        transition_key: impl Into<String>,
        router: F,
    ) -> Self
    where
        F: Fn(&AgentState) -> String + Send + Sync + 'static,
    {
        Self {
            from: from.into(),
            to: String::new(), // Determined at runtime
            transition_key: Some(transition_key.into()),
            edge_type: EdgeType::Conditional(Arc::new(router)),
        }
    }
}

impl fmt::Debug for GraphEdge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphEdge")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("transition_key", &self.transition_key)
            .field(
                "edge_type",
                &match &self.edge_type {
                    EdgeType::Direct => "Direct",
                    EdgeType::Conditional(_) => "Conditional",
                },
            )
            .finish()
    }
}

/// Builder for constructing graph workflows
pub struct GraphBuilder {
    nodes: HashMap<String, GraphNode>,
    edges: Vec<GraphEdge>,
    entry_point: Option<String>,
    name: Option<String>,
    description: Option<String>,
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphBuilder {
    /// Create a new graph builder
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            entry_point: None,
            name: None,
            description: None,
        }
    }

    /// Set the name of the graph
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the description of the graph
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a node to the graph
    pub fn add_node(mut self, executor: impl NodeExecutor + 'static) -> Self {
        let node = GraphNode::new(executor);
        self.nodes.insert(node.id.clone(), node);
        self
    }

    /// Set the entry point of the graph
    pub fn set_entry_point(mut self, node_id: impl Into<String>) -> Self {
        self.entry_point = Some(node_id.into());
        self
    }

    /// Add a direct edge between two nodes
    pub fn add_edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push(GraphEdge::direct(from, to));
        self
    }

    /// Add a direct edge between two nodes for a specific transition key.
    pub fn add_edge_with_key(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        transition_key: impl Into<String>,
    ) -> Self {
        self.edges
            .push(GraphEdge::direct_with_key(from, to, transition_key));
        self
    }

    /// Add an edge from a node to the END
    pub fn add_edge_to_end(mut self, from: impl Into<String>) -> Self {
        self.edges
            .push(GraphEdge::direct(from, transitions::END.to_string()));
        self
    }

    /// Add a conditional edge with a router function
    pub fn add_conditional_edge<F>(mut self, from: impl Into<String>, router: F) -> Self
    where
        F: Fn(&AgentState) -> String + Send + Sync + 'static,
    {
        self.edges.push(GraphEdge::conditional(from, router));
        self
    }

    /// Add a conditional edge for a specific transition key.
    pub fn add_conditional_edge_with_key<F>(
        mut self,
        from: impl Into<String>,
        transition_key: impl Into<String>,
        router: F,
    ) -> Self
    where
        F: Fn(&AgentState) -> String + Send + Sync + 'static,
    {
        self.edges.push(GraphEdge::conditional_with_key(
            from,
            transition_key,
            router,
        ));
        self
    }

    /// Compile the graph into an executable form
    pub fn compile(self) -> Result<CompiledGraph, GraphError> {
        let entry = self.entry_point.ok_or(GraphError::NoEntryPoint)?;

        if !self.nodes.contains_key(&entry) {
            return Err(GraphError::NodeNotFound(entry));
        }

        // Build the petgraph structure using StableGraph for stable node indices
        let mut graph = StableGraph::new();
        let mut node_indices: HashMap<String, NodeIndex> = HashMap::new();

        // Add all nodes
        for (id, node) in &self.nodes {
            let idx = graph.add_node(node.clone());
            node_indices.insert(id.clone(), idx);
        }

        // Add END node
        let end_node = GraphNode {
            id: transitions::END.to_string(),
            executor: Arc::new(EndNode),
        };
        let end_idx = graph.add_node(end_node);
        node_indices.insert(transitions::END.to_string(), end_idx);

        // Add all edges
        for edge in &self.edges {
            let from_idx = node_indices
                .get(&edge.from)
                .ok_or_else(|| GraphError::NodeNotFound(edge.from.clone()))?;

            // For direct edges, add the edge to petgraph
            if let EdgeType::Direct = &edge.edge_type {
                let to_idx = node_indices
                    .get(&edge.to)
                    .ok_or_else(|| GraphError::NodeNotFound(edge.to.clone()))?;
                graph.add_edge(*from_idx, *to_idx, edge.clone());
            }
        }

        Ok(CompiledGraph {
            graph,
            node_indices,
            edge_routes: build_edge_routes(&self.edges),
            edges: self.edges,
            entry_point: entry,
            name: self.name,
            description: self.description,
        })
    }
}

/// A compiled graph ready for execution
///
/// Uses `StableGraph` internally to ensure node indices remain stable
/// across graph mutations, which is important for checkpointing and
/// resuming workflows.
#[derive(Clone)]
pub struct CompiledGraph {
    pub(crate) graph: StableGraph<GraphNode, GraphEdge>,
    pub(crate) node_indices: HashMap<String, NodeIndex>,
    pub(crate) edge_routes: HashMap<String, GraphEdgeRoutes>,
    pub(crate) edges: Vec<GraphEdge>,
    pub(crate) entry_point: String,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct GraphEdgeRoutes {
    keyed: HashMap<String, GraphEdge>,
    default: Option<GraphEdge>,
}

impl CompiledGraph {
    /// Get the entry point node ID
    pub fn entry_point(&self) -> &str {
        &self.entry_point
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> Option<&GraphNode> {
        self.node_indices
            .get(id)
            .and_then(|idx| self.graph.node_weight(*idx))
    }

    /// Get the next node ID based on edges from the given node
    pub fn get_next_node(&self, from: &str, state: &AgentState) -> Option<String> {
        self.get_next_node_for_transition(from, state, transitions::CONTINUE)
    }

    /// Get the next node ID based on the transition key and edges.
    pub fn get_next_node_for_transition(
        &self,
        from: &str,
        state: &AgentState,
        transition_key: &str,
    ) -> Option<String> {
        if let Some(routes) = self.edge_routes.get(from) {
            if let Some(edge) = routes.keyed.get(transition_key) {
                return resolve_edge(edge, state);
            }

            // Backward compatibility: unkeyed edges are default CONTINUE path.
            if transition_key == transitions::CONTINUE {
                if let Some(edge) = routes.default.as_ref() {
                    return resolve_edge(edge, state);
                }
            }
        }

        None
    }

    /// Resolve the next node id from a node's output.
    ///
    /// Single source of truth for output→next-node routing, shared by every
    /// runner (`GraphRunner`, `TracedRunner`). Keeping the five-arm match and
    /// the END fallback here means a routing fix lands once instead of having
    /// to be mirrored across each runner's loop.
    pub fn resolve_next_node(
        &self,
        current_node: &str,
        output: &NodeOutput,
        state: &AgentState,
    ) -> String {
        match output {
            NodeOutput::Finish => transitions::END.to_string(),
            NodeOutput::Continue(Some(target)) => target.clone(),
            NodeOutput::Continue(None) => self
                .get_next_node_for_transition(current_node, state, transitions::CONTINUE)
                .unwrap_or_else(|| transitions::END.to_string()),
            NodeOutput::Route(target) => target.clone(),
            NodeOutput::Transition(key) => self
                .get_next_node_for_transition(current_node, state, key)
                .unwrap_or_else(|| transitions::END.to_string()),
        }
    }

    /// Get the graph name
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Get the graph description
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Generate a Mermaid diagram of the graph
    pub fn to_mermaid(&self) -> String {
        let mut output = String::from("graph TD\n");

        // Add nodes
        for id in self.node_indices.keys() {
            if id != transitions::END {
                output.push_str(&format!("    {}[{}]\n", id.replace('-', "_"), id));
            } else {
                output.push_str(&format!("    {}(({}))\n", id.replace('-', "_"), "END"));
            }
        }

        // Add entry point marker
        output.push_str(&format!(
            "    __start__([Start]) --> {}\n",
            self.entry_point.replace('-', "_")
        ));

        // Add edges
        for edge in &self.edges {
            let from = edge.from.replace('-', "_");
            let to = edge.to.replace('-', "_");
            match &edge.edge_type {
                EdgeType::Direct => {
                    output.push_str(&format!("    {} --> {}\n", from, to));
                }
                EdgeType::Conditional(_) => {
                    output.push_str(&format!("    {} -.->|conditional| ...\n", from));
                }
            }
        }

        output
    }
}

fn resolve_edge(edge: &GraphEdge, state: &AgentState) -> Option<String> {
    match &edge.edge_type {
        EdgeType::Direct => Some(edge.to.clone()),
        EdgeType::Conditional(router) => Some(router(state)),
    }
}

fn build_edge_routes(edges: &[GraphEdge]) -> HashMap<String, GraphEdgeRoutes> {
    let mut routes: HashMap<String, GraphEdgeRoutes> = HashMap::new();

    for edge in edges {
        let entry = routes.entry(edge.from.clone()).or_default();
        match edge.transition_key.as_deref() {
            Some(key) => {
                entry
                    .keyed
                    .entry(key.to_string())
                    .or_insert_with(|| edge.clone());
            }
            None => {
                entry.default.get_or_insert_with(|| edge.clone());
            }
        }
    }

    routes
}

/// Internal END node that terminates execution
struct EndNode;

#[async_trait]
impl NodeExecutor for EndNode {
    fn id(&self) -> &str {
        transitions::END
    }

    async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
        Ok(NodeOutput::Finish)
    }

    fn description(&self) -> Option<&str> {
        Some("Terminal node that ends graph execution")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestNode {
        id: String,
    }

    #[async_trait]
    impl NodeExecutor for TestNode {
        fn id(&self) -> &str {
            &self.id
        }

        async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
            Ok(NodeOutput::cont())
        }
    }

    #[test]
    fn test_node_output() {
        let output = NodeOutput::cont();
        assert!(!output.is_terminal());
        assert!(output.target().is_none());

        let output = NodeOutput::continue_to("next");
        assert_eq!(output.target(), Some("next"));

        let output = NodeOutput::finish();
        assert!(output.is_terminal());

        let output = NodeOutput::end();
        assert!(output.is_terminal());

        let output = NodeOutput::route("target");
        assert_eq!(output.target(), Some("target"));

        let output = NodeOutput::transition("success");
        assert_eq!(output.target(), Some("success"));
    }

    #[test]
    fn test_graph_builder() {
        let graph = GraphBuilder::new()
            .name("test_graph")
            .add_node(TestNode {
                id: "node1".to_string(),
            })
            .add_node(TestNode {
                id: "node2".to_string(),
            })
            .set_entry_point("node1")
            .add_edge("node1", "node2")
            .add_edge_to_end("node2")
            .compile()
            .unwrap();

        assert_eq!(graph.entry_point(), "node1");
        assert!(graph.get_node("node1").is_some());
        assert!(graph.get_node("node2").is_some());
        assert!(graph.get_node(transitions::END).is_some());
    }

    #[test]
    fn test_graph_builder_no_entry_point() {
        let result = GraphBuilder::new()
            .add_node(TestNode {
                id: "node1".to_string(),
            })
            .compile();

        assert!(matches!(result, Err(GraphError::NoEntryPoint)));
    }

    #[test]
    fn test_graph_builder_missing_node() {
        let result = GraphBuilder::new().set_entry_point("nonexistent").compile();

        assert!(matches!(result, Err(GraphError::NodeNotFound(_))));
    }

    #[test]
    fn test_conditional_edge() {
        let graph = GraphBuilder::new()
            .add_node(TestNode {
                id: "start".to_string(),
            })
            .add_node(TestNode {
                id: "branch_a".to_string(),
            })
            .add_node(TestNode {
                id: "branch_b".to_string(),
            })
            .set_entry_point("start")
            .add_conditional_edge("start", |state: &AgentState| {
                if state.is_complete {
                    transitions::END.to_string()
                } else {
                    "branch_a".to_string()
                }
            })
            .compile()
            .unwrap();

        let state = AgentState::new();
        let next = graph.get_next_node("start", &state);
        assert_eq!(next, Some("branch_a".to_string()));

        let mut complete_state = AgentState::new();
        complete_state.is_complete = true;
        let next = graph.get_next_node("start", &complete_state);
        assert_eq!(next, Some(transitions::END.to_string()));
    }

    #[test]
    fn test_transition_key_edge_routing() {
        let graph = GraphBuilder::new()
            .add_node(TestNode {
                id: "start".to_string(),
            })
            .add_node(TestNode {
                id: "tool".to_string(),
            })
            .add_node(TestNode {
                id: "finish".to_string(),
            })
            .set_entry_point("start")
            .add_edge_with_key("start", "tool", "tool_call")
            .add_edge_with_key("start", "finish", "finish")
            .compile()
            .unwrap();

        let state = AgentState::new();
        assert_eq!(
            graph.get_next_node_for_transition("start", &state, "tool_call"),
            Some("tool".to_string())
        );
        assert_eq!(
            graph.get_next_node_for_transition("start", &state, "finish"),
            Some("finish".to_string())
        );
        assert_eq!(
            graph.get_next_node_for_transition("start", &state, "missing"),
            None
        );
    }

    #[test]
    fn test_transition_key_conditional_routing() {
        let graph = GraphBuilder::new()
            .add_node(TestNode {
                id: "start".to_string(),
            })
            .add_node(TestNode {
                id: "branch_a".to_string(),
            })
            .add_node(TestNode {
                id: "branch_b".to_string(),
            })
            .set_entry_point("start")
            .add_conditional_edge_with_key("start", "route", |state: &AgentState| {
                if state.is_complete {
                    "branch_b".to_string()
                } else {
                    "branch_a".to_string()
                }
            })
            .compile()
            .unwrap();

        let state = AgentState::new();
        assert_eq!(
            graph.get_next_node_for_transition("start", &state, "route"),
            Some("branch_a".to_string())
        );

        let mut complete_state = AgentState::new();
        complete_state.is_complete = true;
        assert_eq!(
            graph.get_next_node_for_transition("start", &complete_state, "route"),
            Some("branch_b".to_string())
        );
    }

    #[test]
    fn test_unkeyed_edges_are_default_continue_path() {
        let graph = GraphBuilder::new()
            .add_node(TestNode {
                id: "start".to_string(),
            })
            .add_node(TestNode {
                id: "next".to_string(),
            })
            .set_entry_point("start")
            .add_edge("start", "next")
            .compile()
            .unwrap();

        let state = AgentState::new();
        assert_eq!(
            graph.get_next_node_for_transition("start", &state, transitions::CONTINUE),
            Some("next".to_string())
        );
        assert_eq!(
            graph.get_next_node_for_transition("start", &state, "other"),
            None
        );
    }

    #[test]
    fn test_mermaid_output() {
        let graph = GraphBuilder::new()
            .add_node(TestNode {
                id: "node1".to_string(),
            })
            .add_node(TestNode {
                id: "node2".to_string(),
            })
            .set_entry_point("node1")
            .add_edge("node1", "node2")
            .add_edge_to_end("node2")
            .compile()
            .unwrap();

        let mermaid = graph.to_mermaid();
        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("node1"));
        assert!(mermaid.contains("node2"));
    }
}
