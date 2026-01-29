//! Function-based nodes for oxidizedgraph
//!
//! Provides `FunctionNode` which allows creating nodes from closures
//! without defining a separate struct.

use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::SharedState;

/// Type alias for the async function signature
pub type AsyncNodeFn =
    Arc<dyn Fn(SharedState) -> Pin<Box<dyn Future<Output = Result<NodeOutput, NodeError>> + Send>> + Send + Sync>;

/// A node that wraps a closure for execution
pub struct FunctionNode {
    id: String,
    func: AsyncNodeFn,
    description: Option<String>,
}

impl FunctionNode {
    /// Create a new function node
    pub fn new<F, Fut>(id: impl Into<String>, func: F) -> Self
    where
        F: Fn(SharedState) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<NodeOutput, NodeError>> + Send + 'static,
    {
        let id = id.into();
        Self {
            id,
            func: Arc::new(move |state| Box::pin(func(state))),
            description: None,
        }
    }

    /// Set the description for this node
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Create a simple passthrough node that just continues
    pub fn passthrough(id: impl Into<String>) -> Self {
        Self::new(id, |_state| async move { Ok(NodeOutput::cont()) })
    }

    /// Create a node that always finishes
    pub fn finish(id: impl Into<String>) -> Self {
        Self::new(id, |_state| async move { Ok(NodeOutput::finish()) })
    }

    /// Create a node that routes to a specific target
    pub fn route_to(id: impl Into<String>, target: impl Into<String>) -> Self {
        let target = target.into();
        Self::new(id, move |_state| {
            let t = target.clone();
            async move { Ok(NodeOutput::continue_to(t)) }
        })
    }
}

#[async_trait]
impl NodeExecutor for FunctionNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        (self.func)(state).await
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;
    use std::sync::RwLock;

    #[tokio::test]
    async fn test_function_node() {
        let node = FunctionNode::new("test", |state| async move {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("executed", true);
            Ok(NodeOutput::cont())
        });

        let state = Arc::new(RwLock::new(AgentState::new()));
        let result = node.execute(state.clone()).await.unwrap();

        assert!(!result.is_terminal());

        let guard = state.read().unwrap();
        assert_eq!(guard.get_context::<bool>("executed"), Some(true));
    }

    #[tokio::test]
    async fn test_passthrough_node() {
        let node = FunctionNode::passthrough("pass");
        let state = Arc::new(RwLock::new(AgentState::new()));

        let result = node.execute(state).await.unwrap();
        assert!(!result.is_terminal());
        assert!(result.target().is_none());
    }
}
