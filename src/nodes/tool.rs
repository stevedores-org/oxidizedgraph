//! Tool node implementation for oxidizedgraph
//!
//! Provides `ToolNode` for executing tools and `ToolRegistry` for managing
//! available tools.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::{Message, SharedState, ToolCall};

/// Result of a tool execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolResult {
    /// The tool call ID this result corresponds to
    pub tool_call_id: String,
    /// The result content (success case)
    pub content: Option<String>,
    /// Error message (failure case)
    pub error: Option<String>,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: Some(content.into()),
            error: None,
        }
    }

    /// Create an error result
    pub fn error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: None,
            error: Some(error.into()),
        }
    }

    /// Check if this is a success
    pub fn is_success(&self) -> bool {
        self.content.is_some() && self.error.is_none()
    }

    /// Get the content or error as a string
    pub fn as_str(&self) -> &str {
        self.content
            .as_deref()
            .or(self.error.as_deref())
            .unwrap_or("")
    }
}

/// Trait for implementing tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Get the JSON schema for the tool's parameters
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments
    async fn execute(&self, arguments: serde_json::Value) -> Result<String, NodeError>;
}

/// Type alias for a boxed tool
pub type BoxedTool = Arc<dyn Tool>;

/// A registry of available tools
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, BoxedTool>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool
    pub fn register(mut self, tool: impl Tool + 'static) -> Self {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
        self
    }

    /// Register a boxed tool
    pub fn register_boxed(mut self, tool: BoxedTool) -> Self {
        self.tools.insert(tool.name().to_string(), tool);
        self
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&BoxedTool> {
        self.tools.get(name)
    }

    /// Check if a tool exists
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all tool names
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get tool definitions for LLM context
    pub fn definitions(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "name": t.name(),
                    "description": t.description(),
                    "parameters": t.parameters_schema()
                })
            })
            .collect()
    }

    /// Execute a tool call
    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        match self.get(&call.name) {
            Some(tool) => match tool.execute(call.arguments.clone()).await {
                Ok(result) => ToolResult::success(&call.id, result),
                Err(e) => ToolResult::error(&call.id, e.to_string()),
            },
            None => ToolResult::error(&call.id, format!("Tool '{}' not found", call.name)),
        }
    }
}

/// A node that executes pending tool calls
pub struct ToolNode {
    id: String,
    registry: ToolRegistry,
}

impl ToolNode {
    /// Create a new tool node
    pub fn new(id: impl Into<String>, registry: ToolRegistry) -> Self {
        Self {
            id: id.into(),
            registry,
        }
    }
}

#[async_trait]
impl NodeExecutor for ToolNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        // Get pending tool calls
        let tool_calls = {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.tool_calls.clone()
        };

        if tool_calls.is_empty() {
            return Ok(NodeOutput::cont());
        }

        // Execute each tool call
        let mut results = Vec::new();
        for call in &tool_calls {
            let result = self.registry.execute(call).await;
            results.push(result);
        }

        // Update state with results
        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;

            // Add tool results as messages
            for result in results {
                guard.messages.push(Message::tool_result(
                    &result.tool_call_id,
                    result.as_str(),
                ));
            }

            // Clear pending tool calls
            guard.clear_tool_calls();
        }

        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Executes pending tool calls")
    }
}

/// A simple function-based tool implementation
pub struct FunctionTool<F>
where
    F: Fn(serde_json::Value) -> Result<String, String> + Send + Sync,
{
    name: String,
    description: String,
    parameters_schema: serde_json::Value,
    func: F,
}

impl<F> FunctionTool<F>
where
    F: Fn(serde_json::Value) -> Result<String, String> + Send + Sync,
{
    /// Create a new function tool
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: serde_json::Value,
        func: F,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters_schema,
            func,
        }
    }
}

#[async_trait]
impl<F> Tool for FunctionTool<F>
where
    F: Fn(serde_json::Value) -> Result<String, String> + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters_schema.clone()
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, NodeError> {
        (self.func)(arguments).map_err(NodeError::tool_error)
    }
}

/// An async function-based tool implementation
pub struct AsyncFunctionTool<F, Fut>
where
    F: Fn(serde_json::Value) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<String, String>> + Send,
{
    name: String,
    description: String,
    parameters_schema: serde_json::Value,
    func: F,
}

impl<F, Fut> AsyncFunctionTool<F, Fut>
where
    F: Fn(serde_json::Value) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<String, String>> + Send,
{
    /// Create a new async function tool
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: serde_json::Value,
        func: F,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters_schema,
            func,
        }
    }
}

#[async_trait]
impl<F, Fut> Tool for AsyncFunctionTool<F, Fut>
where
    F: Fn(serde_json::Value) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<String, String>> + Send,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters_schema.clone()
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, NodeError> {
        (self.func)(arguments).await.map_err(NodeError::tool_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;
    use std::sync::RwLock;

    #[test]
    fn test_tool_result() {
        let result = ToolResult::success("call_1", "Success!");
        assert!(result.is_success());
        assert_eq!(result.as_str(), "Success!");

        let result = ToolResult::error("call_2", "Failed");
        assert!(!result.is_success());
        assert_eq!(result.as_str(), "Failed");
    }

    #[tokio::test]
    async fn test_function_tool() {
        let tool = FunctionTool::new(
            "add",
            "Adds two numbers",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number" },
                    "b": { "type": "number" }
                }
            }),
            |args| {
                let a = args["a"].as_f64().unwrap_or(0.0);
                let b = args["b"].as_f64().unwrap_or(0.0);
                Ok(format!("{}", a + b))
            },
        );

        let result = tool
            .execute(serde_json::json!({"a": 2, "b": 3}))
            .await
            .unwrap();
        assert_eq!(result, "5");
    }

    #[tokio::test]
    async fn test_tool_registry() {
        let tool = FunctionTool::new(
            "greet",
            "Greets someone",
            serde_json::json!({}),
            |args| {
                let name = args["name"].as_str().unwrap_or("World");
                Ok(format!("Hello, {}!", name))
            },
        );

        let registry = ToolRegistry::new().register(tool);

        assert!(registry.has("greet"));
        assert!(!registry.has("unknown"));

        let call = ToolCall::new("1", "greet", serde_json::json!({"name": "Rust"}));
        let result = registry.execute(&call).await;
        assert!(result.is_success());
        assert_eq!(result.as_str(), "Hello, Rust!");
    }

    #[tokio::test]
    async fn test_tool_node() {
        let tool = FunctionTool::new(
            "echo",
            "Echoes input",
            serde_json::json!({}),
            |args| Ok(args["message"].as_str().unwrap_or("").to_string()),
        );

        let registry = ToolRegistry::new().register(tool);
        let node = ToolNode::new("tools", registry);

        let mut state = AgentState::new();
        state
            .tool_calls
            .push(ToolCall::new("1", "echo", serde_json::json!({"message": "Hello"})));

        let shared = Arc::new(RwLock::new(state));
        let result = node.execute(shared.clone()).await.unwrap();
        assert!(!result.is_terminal());

        let guard = shared.read().unwrap();
        assert!(guard.tool_calls.is_empty()); // Cleared
        assert_eq!(guard.messages.len(), 1); // Tool result added
    }
}
