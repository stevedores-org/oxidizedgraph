//! State trait and definitions for oxidizedgraph
//!
//! The `State` trait is the core abstraction for workflow state.
//! All state types must implement this trait.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Marker trait for graph state types
///
/// All state types used with `Graph` and `Runtime` must implement this trait.
/// The trait ensures the state can be:
/// - Cloned (for checkpointing and passing between nodes)
/// - Serialized/deserialized (for persistence)
/// - Sent between threads safely
///
/// # Example
///
/// ```rust,ignore
/// use oxidizedgraph::prelude::*;
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Clone, Debug, Default, Serialize, Deserialize)]
/// struct MyState {
///     messages: Vec<String>,
///     counter: u32,
/// }
///
/// impl State for MyState {
///     fn schema() -> serde_json::Value {
///         serde_json::json!({
///             "type": "object",
///             "properties": {
///                 "messages": { "type": "array", "channel": "append" },
///                 "counter": { "type": "integer" }
///             }
///         })
///     }
/// }
/// ```
pub trait State: Clone + Send + Sync + Serialize + DeserializeOwned + 'static {
    /// Get the JSON schema for this state type
    ///
    /// The schema is used for:
    /// - Validation
    /// - Documentation
    /// - Channel configuration (via "channel" property)
    fn schema() -> serde_json::Value {
        serde_json::json!({})
    }
}

// Implement State for common types that might be used as simple state
impl State for () {
    fn schema() -> serde_json::Value {
        serde_json::json!({"type": "null"})
    }
}

impl State for String {
    fn schema() -> serde_json::Value {
        serde_json::json!({"type": "string"})
    }
}

impl State for serde_json::Value {
    fn schema() -> serde_json::Value {
        serde_json::json!({})
    }
}

/// A message in the conversation history
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// Role of the message sender (system, user, assistant, tool)
    pub role: MessageRole,
    /// Content of the message
    pub content: String,
    /// Optional name for tool messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional tool call ID for tool result messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    /// Create a new tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            name: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// Role of a message sender
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System prompt message
    System,
    /// User input message
    User,
    /// Assistant (LLM) response message
    Assistant,
    /// Tool execution result message
    Tool,
}

/// A tool call request from the LLM
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// Unique identifier for this tool call
    pub id: String,
    /// Name of the tool to call
    pub name: String,
    /// Arguments to pass to the tool as JSON
    pub arguments: serde_json::Value,
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

/// The main state structure for agent execution
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentState {
    /// Conversation history (append-only channel)
    pub messages: Vec<Message>,

    /// Pending tool calls from the last LLM response
    pub tool_calls: Vec<ToolCall>,

    /// Arbitrary context data for passing information between nodes
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,

    /// Current iteration count
    #[serde(default)]
    pub iteration: usize,

    /// Whether the agent has completed its task
    #[serde(default)]
    pub is_complete: bool,
}

impl State for AgentState {
    fn schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "messages": { "type": "array", "channel": "append" },
                "tool_calls": { "type": "array" },
                "context": { "type": "object" },
                "iteration": { "type": "integer" },
                "is_complete": { "type": "boolean" }
            }
        })
    }
}

impl AgentState {
    /// Create a new empty agent state
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new agent state with an initial user message
    pub fn with_user_message(message: impl Into<String>) -> Self {
        Self {
            messages: vec![Message::user(message)],
            ..Default::default()
        }
    }

    /// Create a new agent state with a system prompt and user message
    pub fn with_system_and_user(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            messages: vec![Message::system(system), Message::user(user)],
            ..Default::default()
        }
    }

    /// Add an assistant message to the conversation
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.messages.push(Message::assistant(content));
    }

    /// Add a user message to the conversation
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.messages.push(Message::user(content));
    }

    /// Add a tool result message to the conversation
    pub fn add_tool_result(&mut self, tool_call_id: impl Into<String>, content: impl Into<String>) {
        self.messages
            .push(Message::tool_result(tool_call_id, content));
    }

    /// Get a context value by key
    pub fn get_context<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.context
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Set a context value by key
    pub fn set_context<T: Serialize>(&mut self, key: impl Into<String>, value: T) {
        if let Ok(v) = serde_json::to_value(value) {
            self.context.insert(key.into(), v);
        }
    }

    /// Remove a context value by key
    pub fn remove_context(&mut self, key: &str) -> Option<serde_json::Value> {
        self.context.remove(key)
    }

    /// Check if a context key exists
    pub fn has_context(&self, key: &str) -> bool {
        self.context.contains_key(key)
    }

    /// Get the last message in the conversation
    pub fn last_message(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Get the last assistant message
    pub fn last_assistant_message(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
    }

    /// Get the last user message
    pub fn last_user_message(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
    }

    /// Clear all pending tool calls
    pub fn clear_tool_calls(&mut self) {
        self.tool_calls.clear();
    }

    /// Check if there are pending tool calls
    pub fn has_pending_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Mark the agent as complete
    pub fn mark_complete(&mut self) {
        self.is_complete = true;
    }

    /// Increment the iteration counter
    pub fn increment_iteration(&mut self) {
        self.iteration += 1;
    }
}

/// Thread-safe shared state wrapper
pub type SharedState = Arc<RwLock<AgentState>>;

/// Extension trait for creating and working with SharedState
pub trait SharedStateExt {
    /// Create a new shared state from an AgentState
    fn new_shared(state: AgentState) -> SharedState;

    /// Create a new empty shared state
    fn new_shared_empty() -> SharedState;
}

impl SharedStateExt for SharedState {
    fn new_shared(state: AgentState) -> SharedState {
        Arc::new(RwLock::new(state))
    }

    fn new_shared_empty() -> SharedState {
        Self::new_shared(AgentState::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    struct TestState {
        value: i32,
    }

    impl State for TestState {
        fn schema() -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "value": { "type": "integer" }
                }
            })
        }
    }

    #[test]
    fn test_state_schema() {
        let schema = TestState::schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_unit_state() {
        let schema = <()>::schema();
        assert_eq!(schema["type"], "null");
    }

    #[test]
    fn test_message_constructors() {
        let msg = Message::system("You are helpful");
        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(msg.content, "You are helpful");

        let msg = Message::user("Hello");
        assert_eq!(msg.role, MessageRole::User);

        let msg = Message::assistant("Hi there");
        assert_eq!(msg.role, MessageRole::Assistant);

        let msg = Message::tool_result("call_1", "result");
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_call_id, Some("call_1".to_string()));
    }

    #[test]
    fn test_agent_state_messages() {
        let mut state = AgentState::new();
        state.add_user_message("Hello");
        state.add_assistant_message("Hi!");

        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.last_message().unwrap().role, MessageRole::Assistant);
        assert_eq!(
            state.last_user_message().unwrap().content,
            "Hello".to_string()
        );
    }

    #[test]
    fn test_agent_state_context() {
        let mut state = AgentState::new();
        state.set_context("count", 42i32);
        state.set_context("name", "test".to_string());

        assert_eq!(state.get_context::<i32>("count"), Some(42));
        assert_eq!(
            state.get_context::<String>("name"),
            Some("test".to_string())
        );
        assert!(state.has_context("count"));
        assert!(!state.has_context("missing"));

        state.remove_context("count");
        assert!(!state.has_context("count"));
    }

    #[test]
    fn test_tool_call() {
        let call = ToolCall::new("id1", "get_weather", serde_json::json!({"city": "NYC"}));
        assert_eq!(call.id, "id1");
        assert_eq!(call.name, "get_weather");
    }
}
