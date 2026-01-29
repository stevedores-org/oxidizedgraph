//! Conditional node implementation for oxidizedgraph
//!
//! Provides `ConditionalNode` for routing based on state predicates.

use async_trait::async_trait;
use std::sync::Arc;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::{AgentState, SharedState};

/// Type alias for a condition function
pub type ConditionFn = Arc<dyn Fn(&AgentState) -> bool + Send + Sync>;

/// A node that routes based on a condition
pub struct ConditionalNode {
    id: String,
    condition: ConditionFn,
    true_target: String,
    false_target: String,
}

impl ConditionalNode {
    /// Create a new conditional node
    pub fn new<F>(
        id: impl Into<String>,
        condition: F,
        true_target: impl Into<String>,
        false_target: impl Into<String>,
    ) -> Self
    where
        F: Fn(&AgentState) -> bool + Send + Sync + 'static,
    {
        Self {
            id: id.into(),
            condition: Arc::new(condition),
            true_target: true_target.into(),
            false_target: false_target.into(),
        }
    }

    /// Create a conditional that checks if the agent is complete
    pub fn is_complete(
        id: impl Into<String>,
        complete_target: impl Into<String>,
        continue_target: impl Into<String>,
    ) -> Self {
        Self::new(
            id,
            |state| state.is_complete,
            complete_target,
            continue_target,
        )
    }

    /// Create a conditional that checks for pending tool calls
    pub fn has_tool_calls(
        id: impl Into<String>,
        has_calls_target: impl Into<String>,
        no_calls_target: impl Into<String>,
    ) -> Self {
        Self::new(
            id,
            |state| state.has_pending_tool_calls(),
            has_calls_target,
            no_calls_target,
        )
    }

    /// Create a conditional that checks a context boolean
    pub fn context_bool(
        id: impl Into<String>,
        key: impl Into<String>,
        true_target: impl Into<String>,
        false_target: impl Into<String>,
    ) -> Self {
        let key = key.into();
        Self::new(
            id,
            move |state| state.get_context::<bool>(&key).unwrap_or(false),
            true_target,
            false_target,
        )
    }
}

#[async_trait]
impl NodeExecutor for ConditionalNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let guard = state
            .read()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let target = if (self.condition)(&guard) {
            self.true_target.clone()
        } else {
            self.false_target.clone()
        };

        if target == crate::graph::transitions::END {
            Ok(NodeOutput::finish())
        } else {
            Ok(NodeOutput::continue_to(target))
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Routes based on a condition")
    }
}

/// A multi-branch conditional node
pub struct BranchNode {
    id: String,
    branches: Vec<(ConditionFn, String)>,
    default: String,
}

impl BranchNode {
    /// Create a new branch node with a default target
    pub fn new(id: impl Into<String>, default: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            branches: Vec::new(),
            default: default.into(),
        }
    }

    /// Add a branch
    pub fn branch<F>(mut self, condition: F, target: impl Into<String>) -> Self
    where
        F: Fn(&AgentState) -> bool + Send + Sync + 'static,
    {
        self.branches.push((Arc::new(condition), target.into()));
        self
    }

    /// Add a branch that checks a context value
    pub fn branch_on_context<T: for<'de> serde::Deserialize<'de> + PartialEq + Send + Sync + 'static>(
        self,
        key: impl Into<String>,
        expected: T,
        target: impl Into<String>,
    ) -> Self {
        let key = key.into();
        self.branch(
            move |state| state.get_context::<T>(&key).map(|v| v == expected).unwrap_or(false),
            target,
        )
    }
}

#[async_trait]
impl NodeExecutor for BranchNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let guard = state
            .read()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        for (condition, target) in &self.branches {
            if condition(&guard) {
                if target == crate::graph::transitions::END {
                    return Ok(NodeOutput::finish());
                }
                return Ok(NodeOutput::continue_to(target.clone()));
            }
        }

        if self.default == crate::graph::transitions::END {
            Ok(NodeOutput::finish())
        } else {
            Ok(NodeOutput::continue_to(self.default.clone()))
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Routes based on multiple conditions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;

    #[tokio::test]
    async fn test_conditional_node_true() {
        let node = ConditionalNode::new(
            "cond",
            |state| state.is_complete,
            "done",
            "continue",
        );

        let mut state = AgentState::new();
        state.is_complete = true;
        let shared = Arc::new(RwLock::new(state));

        let result = node.execute(shared).await.unwrap();
        assert_eq!(result.target(), Some("done"));
    }

    #[tokio::test]
    async fn test_conditional_node_false() {
        let node = ConditionalNode::new(
            "cond",
            |state| state.is_complete,
            "done",
            "continue",
        );

        let state = AgentState::new();
        let shared = Arc::new(RwLock::new(state));

        let result = node.execute(shared).await.unwrap();
        assert_eq!(result.target(), Some("continue"));
    }

    #[tokio::test]
    async fn test_is_complete_helper() {
        let node = ConditionalNode::is_complete("check", "finished", "processing");

        let mut state = AgentState::new();
        state.is_complete = true;
        let shared = Arc::new(RwLock::new(state));

        let result = node.execute(shared).await.unwrap();
        assert_eq!(result.target(), Some("finished"));
    }

    #[tokio::test]
    async fn test_has_tool_calls_helper() {
        use crate::state::ToolCall;

        let node = ConditionalNode::has_tool_calls("check", "execute_tools", "respond");

        let mut state = AgentState::new();
        state.tool_calls.push(ToolCall::new("1", "test", serde_json::json!({})));
        let shared = Arc::new(RwLock::new(state));

        let result = node.execute(shared).await.unwrap();
        assert_eq!(result.target(), Some("execute_tools"));
    }

    #[tokio::test]
    async fn test_branch_node() {
        let node = BranchNode::new("branch", "default")
            .branch(|s| s.get_context::<i32>("count").unwrap_or(0) > 10, "high")
            .branch(|s| s.get_context::<i32>("count").unwrap_or(0) > 5, "medium");

        // Test high branch
        let mut state = AgentState::new();
        state.set_context("count", 15);
        let shared = Arc::new(RwLock::new(state));
        let result = node.execute(shared).await.unwrap();
        assert_eq!(result.target(), Some("high"));

        // Test medium branch
        let mut state = AgentState::new();
        state.set_context("count", 7);
        let shared = Arc::new(RwLock::new(state));
        let result = node.execute(shared).await.unwrap();
        assert_eq!(result.target(), Some("medium"));

        // Test default
        let mut state = AgentState::new();
        state.set_context("count", 3);
        let shared = Arc::new(RwLock::new(state));
        let result = node.execute(shared).await.unwrap();
        assert_eq!(result.target(), Some("default"));
    }
}
