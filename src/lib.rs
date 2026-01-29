//! # oxidizedgraph
//!
//! A humble attempt at LangGraph in Rust - high-performance agent orchestration framework.
//!
//! oxidizedgraph provides graph-based agent workflows with:
//! - **Type-safe state management** with `AgentState` and `SharedState`
//! - **Async execution** powered by Tokio
//! - **Flexible routing** with conditional edges
//! - **Built-in nodes** for common patterns (LLM, tools, routing)
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use oxidizedgraph::prelude::*;
//!
//! // Define a simple node
//! struct MyNode;
//!
//! #[async_trait]
//! impl NodeExecutor for MyNode {
//!     fn id(&self) -> &str { "my_node" }
//!
//!     async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
//!         // Do work with state
//!         Ok(NodeOutput::cont())
//!     }
//! }
//!
//! // Build and run the graph
//! let graph = GraphBuilder::new()
//!     .add_node(MyNode)
//!     .set_entry_point("my_node")
//!     .add_edge_to_end("my_node")
//!     .compile()?;
//!
//! let runner = GraphRunner::with_defaults(graph);
//! let result = runner.invoke(AgentState::new()).await?;
//! ```

#![warn(missing_docs)]

// Core modules
pub mod checkpoint;
pub mod error;
pub mod events;
pub mod git;
pub mod graph;
pub mod nodes;
pub mod runner;
pub mod state;

/// Convenient re-exports for common usage
pub mod prelude;

#[cfg(test)]
mod tests {
    use super::prelude::*;

    struct TestNode {
        id: String,
    }

    #[async_trait]
    impl NodeExecutor for TestNode {
        fn id(&self) -> &str {
            &self.id
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("test", "value");
            Ok(NodeOutput::cont())
        }
    }

    #[tokio::test]
    async fn test_simple_graph() {
        let graph = GraphBuilder::new()
            .name("test")
            .add_node(TestNode {
                id: "node1".to_string(),
            })
            .set_entry_point("node1")
            .add_edge_to_end("node1")
            .compile()
            .unwrap();

        let runner = GraphRunner::with_defaults(graph);
        let result = runner.invoke(AgentState::new()).await.unwrap();

        assert_eq!(
            result.get_context::<String>("test"),
            Some("value".to_string())
        );
    }
}
