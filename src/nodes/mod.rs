//! Node implementations for oxidizedgraph
//!
//! This module provides common node implementations including:
//! - `EchoNode` - echoes input to output
//! - `DelayNode` - adds a delay
//! - `StaticTransitionNode` - always transitions to a fixed target
//! - `ContextRouterNode` - routes based on context values

pub mod conditional;
pub mod function;
pub mod llm;
pub mod tool;

use async_trait::async_trait;
use std::time::Duration;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::SharedState;

/// A node that echoes a message to the state context
pub struct EchoNode {
    id: String,
    message: String,
    context_key: String,
}

impl EchoNode {
    /// Create a new echo node
    pub fn new(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            message: message.into(),
            context_key: "echo".to_string(),
        }
    }

    /// Set the context key to store the echo message
    pub fn with_context_key(mut self, key: impl Into<String>) -> Self {
        self.context_key = key.into();
        self
    }
}

#[async_trait]
impl NodeExecutor for EchoNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;
        guard.set_context(&self.context_key, self.message.clone());
        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Echoes a message to the state context")
    }
}

/// A node that adds a delay before continuing
pub struct DelayNode {
    id: String,
    duration: Duration,
}

impl DelayNode {
    /// Create a new delay node
    pub fn new(id: impl Into<String>, duration: Duration) -> Self {
        Self {
            id: id.into(),
            duration,
        }
    }

    /// Create a delay node with milliseconds
    pub fn from_millis(id: impl Into<String>, millis: u64) -> Self {
        Self::new(id, Duration::from_millis(millis))
    }

    /// Create a delay node with seconds
    pub fn from_secs(id: impl Into<String>, secs: u64) -> Self {
        Self::new(id, Duration::from_secs(secs))
    }
}

#[async_trait]
impl NodeExecutor for DelayNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
        tokio::time::sleep(self.duration).await;
        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Delays execution for a specified duration")
    }
}

/// A node that always transitions to a fixed target
pub struct StaticTransitionNode {
    id: String,
    target: String,
}

impl StaticTransitionNode {
    /// Create a new static transition node
    pub fn new(id: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            target: target.into(),
        }
    }

    /// Create a node that transitions to END
    pub fn to_end(id: impl Into<String>) -> Self {
        Self::new(id, crate::graph::transitions::END)
    }
}

#[async_trait]
impl NodeExecutor for StaticTransitionNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
        if self.target == crate::graph::transitions::END {
            Ok(NodeOutput::finish())
        } else {
            Ok(NodeOutput::continue_to(self.target.clone()))
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Transitions to a fixed target node")
    }
}

/// A node that routes based on context values
pub struct ContextRouterNode {
    id: String,
    context_key: String,
    routes: std::collections::HashMap<String, String>,
    default: Option<String>,
}

impl ContextRouterNode {
    /// Create a new context router node
    pub fn new(id: impl Into<String>, context_key: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            context_key: context_key.into(),
            routes: std::collections::HashMap::new(),
            default: None,
        }
    }

    /// Add a route for a context value
    pub fn route(mut self, value: impl Into<String>, target: impl Into<String>) -> Self {
        self.routes.insert(value.into(), target.into());
        self
    }

    /// Set the default target when no route matches
    pub fn default_route(mut self, target: impl Into<String>) -> Self {
        self.default = Some(target.into());
        self
    }
}

#[async_trait]
impl NodeExecutor for ContextRouterNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let guard = state
            .read()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let value: Option<String> = guard.get_context(&self.context_key);

        if let Some(v) = value {
            if let Some(target) = self.routes.get(&v) {
                return Ok(NodeOutput::continue_to(target.clone()));
            }
        }

        if let Some(ref default) = self.default {
            Ok(NodeOutput::continue_to(default.clone()))
        } else {
            Ok(NodeOutput::finish())
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Routes based on context values")
    }
}

// Re-export commonly used types from submodules
pub use conditional::ConditionalNode;
pub use function::FunctionNode;
pub use llm::{LLMConfig, LLMNode, LLMProvider};
pub use tool::{Tool, ToolNode, ToolRegistry};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;
    use std::sync::{Arc, RwLock};

    #[tokio::test]
    async fn test_echo_node() {
        let node = EchoNode::new("echo", "Hello, World!");
        let state = Arc::new(RwLock::new(AgentState::new()));

        let result = node.execute(state.clone()).await.unwrap();
        assert!(!result.is_terminal());

        let guard = state.read().unwrap();
        assert_eq!(
            guard.get_context::<String>("echo"),
            Some("Hello, World!".to_string())
        );
    }

    #[tokio::test]
    async fn test_delay_node() {
        let node = DelayNode::from_millis("delay", 10);
        let state = Arc::new(RwLock::new(AgentState::new()));

        let start = std::time::Instant::now();
        let result = node.execute(state).await.unwrap();
        let elapsed = start.elapsed();

        assert!(!result.is_terminal());
        assert!(elapsed >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_static_transition_node() {
        let node = StaticTransitionNode::new("transit", "next_node");
        let state = Arc::new(RwLock::new(AgentState::new()));

        let result = node.execute(state).await.unwrap();
        assert_eq!(result.target(), Some("next_node"));
    }

    #[tokio::test]
    async fn test_static_transition_to_end() {
        let node = StaticTransitionNode::to_end("end_transit");
        let state = Arc::new(RwLock::new(AgentState::new()));

        let result = node.execute(state).await.unwrap();
        assert!(result.is_terminal());
    }

    #[tokio::test]
    async fn test_context_router_node() {
        let node = ContextRouterNode::new("router", "action")
            .route("process", "process_node")
            .route("complete", "complete_node")
            .default_route("fallback_node");

        let state = Arc::new(RwLock::new(AgentState::new()));

        // Test with matching route
        {
            let mut guard = state.write().unwrap();
            guard.set_context("action", "process".to_string());
        }

        let result = node.execute(state.clone()).await.unwrap();
        assert_eq!(result.target(), Some("process_node"));

        // Test with default route
        {
            let mut guard = state.write().unwrap();
            guard.set_context("action", "unknown".to_string());
        }

        let result = node.execute(state).await.unwrap();
        assert_eq!(result.target(), Some("fallback_node"));
    }
}
