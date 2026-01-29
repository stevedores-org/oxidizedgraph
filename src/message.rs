//! Message types for LLM conversations
//!
//! This module provides types for representing chat messages,
//! tool calls, and tool results in LLM-based workflows.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Role of a message sender
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System prompt message
    System,
    /// User input message
    User,
    /// Assistant (LLM) response message
    Assistant,
    /// Tool execution result message
    Tool,
}

/// Content block within a message
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text content
    Text { text: String },
    /// Tool use request
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

impl ContentBlock {
    /// Create a text content block
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Create a tool use content block
    pub fn tool_use(id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        Self::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    /// Create a tool result content block
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    /// Create a tool error content block
    pub fn tool_error(tool_use_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: error.into(),
            is_error: true,
        }
    }

    /// Get text content if this is a text block
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// A tool call request from the LLM
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call
    pub id: String,
    /// Name of the tool to call
    pub name: String,
    /// Arguments to pass to the tool
    pub arguments: serde_json::Value,
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

/// Result from a tool execution
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// ID of the tool call this result is for
    pub tool_call_id: String,
    /// Result content
    pub content: String,
    /// Whether this is an error result
    pub is_error: bool,
}

impl ToolResult {
    /// Create a successful tool result
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    /// Create an error tool result
    pub fn error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: error.into(),
            is_error: true,
        }
    }
}

/// A message in a conversation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: Role,
    /// Content blocks
    pub content: Vec<ContentBlock>,
    /// Optional name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    /// Create a new system message
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: vec![ContentBlock::text(text)],
            name: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new user message
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::text(text)],
            name: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new assistant message
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::text(text)],
            name: None,
            metadata: HashMap::new(),
        }
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tools(text: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        let mut content: Vec<ContentBlock> = Vec::new();

        if let Some(t) = text {
            content.push(ContentBlock::text(t));
        }

        for call in tool_calls {
            content.push(ContentBlock::tool_use(call.id, call.name, call.arguments));
        }

        Self {
            role: Role::Assistant,
            content,
            name: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentBlock::tool_result(tool_call_id, content)],
            name: None,
            metadata: HashMap::new(),
        }
    }

    /// Get all text content from this message
    pub fn text(&self) -> Option<&str> {
        self.content.iter().find_map(|c| c.as_text())
    }

    /// Get all text content concatenated
    pub fn all_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Extract tool calls from this message
    pub fn tool_uses(&self) -> Vec<ToolCall> {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::ToolUse { id, name, input } => {
                    Some(ToolCall::new(id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Check if this message has tool calls
    pub fn has_tool_uses(&self) -> bool {
        self.content.iter().any(|c| matches!(c, ContentBlock::ToolUse { .. }))
    }
}

/// A collection of messages (conversation history)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Messages(pub Vec<Message>);

impl Messages {
    /// Create an empty message collection
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Create a message collection with initial messages
    pub fn with_messages(messages: Vec<Message>) -> Self {
        Self(messages)
    }

    /// Add a message to the collection
    pub fn push(&mut self, msg: Message) {
        self.0.push(msg);
    }

    /// Get the last message
    pub fn last(&self) -> Option<&Message> {
        self.0.last()
    }

    /// Get the last assistant message
    pub fn last_assistant(&self) -> Option<&Message> {
        self.0.iter().rev().find(|m| m.role == Role::Assistant)
    }

    /// Get the last user message
    pub fn last_user(&self) -> Option<&Message> {
        self.0.iter().rev().find(|m| m.role == Role::User)
    }

    /// Get number of messages
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over messages
    pub fn iter(&self) -> impl Iterator<Item = &Message> {
        self.0.iter()
    }
}

impl std::ops::Deref for Messages {
    type Target = Vec<Message>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Messages {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for Messages {
    type Item = Message;
    type IntoIter = std::vec::IntoIter<Message>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Messages {
    type Item = &'a Message;
    type IntoIter = std::slice::Iter<'a, Message>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_constructors() {
        let msg = Message::system("You are helpful");
        assert_eq!(msg.role, Role::System);
        assert_eq!(msg.text(), Some("You are helpful"));

        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);

        let msg = Message::assistant("Hi there");
        assert_eq!(msg.role, Role::Assistant);
    }

    #[test]
    fn test_assistant_with_tools() {
        let tool_calls = vec![
            ToolCall::new("call_1", "get_weather", serde_json::json!({"city": "NYC"})),
        ];
        let msg = Message::assistant_with_tools(Some("Let me check".into()), tool_calls);

        assert_eq!(msg.role, Role::Assistant);
        assert!(msg.has_tool_uses());
        assert_eq!(msg.tool_uses().len(), 1);
    }

    #[test]
    fn test_messages_collection() {
        let mut messages = Messages::new();
        messages.push(Message::user("Hello"));
        messages.push(Message::assistant("Hi!"));

        assert_eq!(messages.len(), 2);
        assert_eq!(messages.last_user().unwrap().text(), Some("Hello"));
        assert_eq!(messages.last_assistant().unwrap().text(), Some("Hi!"));
    }

    #[test]
    fn test_tool_call() {
        let call = ToolCall::new("id1", "search", serde_json::json!({"query": "rust"}));
        assert_eq!(call.id, "id1");
        assert_eq!(call.name, "search");
    }
}
