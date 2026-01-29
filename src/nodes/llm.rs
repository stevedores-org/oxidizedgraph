//! LLM node implementation for oxidizedgraph
//!
//! Provides `LLMNode` for calling Large Language Model APIs and
//! `LLMProvider` trait for implementing custom LLM integrations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::{Message, SharedState, ToolCall};

/// Configuration for LLM API calls
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LLMConfig {
    /// The model to use (e.g., "claude-3-opus-20240229", "gpt-4")
    pub model: String,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature for sampling (0.0 - 1.0)
    pub temperature: Option<f32>,
    /// System prompt to prepend to messages
    pub system_prompt: Option<String>,
    /// Stop sequences
    pub stop_sequences: Option<Vec<String>>,
    /// API base URL override
    pub api_base: Option<String>,
    /// API key (if not using environment variable)
    pub api_key: Option<String>,
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            model: "claude-3-sonnet-20240229".to_string(),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            system_prompt: None,
            stop_sequences: None,
            api_base: None,
            api_key: None,
        }
    }
}

impl LLMConfig {
    /// Create a new LLM config with the given model
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set temperature
    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set system prompt
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set API base URL
    pub fn api_base(mut self, url: impl Into<String>) -> Self {
        self.api_base = Some(url.into());
        self
    }

    /// Set API key
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
}

/// Response from an LLM provider
#[derive(Clone, Debug)]
pub struct LLMResponse {
    /// The generated text content
    pub content: String,
    /// Tool calls requested by the model
    pub tool_calls: Vec<ToolCall>,
    /// Whether the model finished or was cut off
    pub stop_reason: StopReason,
    /// Token usage information
    pub usage: Option<TokenUsage>,
}

impl LLMResponse {
    /// Create a simple text response
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tool_calls: vec![],
            stop_reason: StopReason::EndTurn,
            usage: None,
        }
    }

    /// Create a response with tool calls
    pub fn with_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            content: content.into(),
            tool_calls,
            stop_reason: StopReason::ToolUse,
            usage: None,
        }
    }
}

/// Reason why the LLM stopped generating
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// Natural end of response
    EndTurn,
    /// Hit max tokens limit
    MaxTokens,
    /// Hit a stop sequence
    StopSequence,
    /// Model wants to use tools
    ToolUse,
    /// Unknown/other reason
    Other(String),
}

/// Token usage information
#[derive(Clone, Debug, Default)]
pub struct TokenUsage {
    /// Input tokens
    pub input_tokens: u32,
    /// Output tokens
    pub output_tokens: u32,
}

/// Trait for implementing LLM providers
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Generate a response from the LLM
    async fn generate(
        &self,
        messages: &[Message],
        config: &LLMConfig,
    ) -> Result<LLMResponse, NodeError>;

    /// Get the provider name
    fn name(&self) -> &str;
}

/// A node that calls an LLM provider
pub struct LLMNode<P: LLMProvider> {
    id: String,
    provider: P,
    config: LLMConfig,
}

impl<P: LLMProvider> LLMNode<P> {
    /// Create a new LLM node
    pub fn new(id: impl Into<String>, provider: P, config: LLMConfig) -> Self {
        Self {
            id: id.into(),
            provider,
            config,
        }
    }

    /// Create with default config
    pub fn with_provider(id: impl Into<String>, provider: P) -> Self {
        Self::new(id, provider, LLMConfig::default())
    }
}

#[async_trait]
impl<P: LLMProvider + 'static> NodeExecutor for LLMNode<P> {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        // Read messages from state
        let messages = {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.messages.clone()
        };

        // Call the LLM
        let response = self.provider.generate(&messages, &self.config).await?;

        // Update state with response
        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;

            // Add assistant message
            guard.add_assistant_message(&response.content);

            // Store tool calls if any
            if !response.tool_calls.is_empty() {
                guard.tool_calls = response.tool_calls;
            }

            // Mark complete if no tool calls
            if guard.tool_calls.is_empty() {
                guard.mark_complete();
            }
        }

        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Calls an LLM provider to generate a response")
    }
}

/// A mock LLM provider for testing
pub struct MockLLMProvider {
    responses: std::collections::VecDeque<LLMResponse>,
}

impl MockLLMProvider {
    /// Create a new mock provider
    pub fn new() -> Self {
        Self {
            responses: std::collections::VecDeque::new(),
        }
    }

    /// Add a response to the queue
    pub fn with_response(mut self, response: LLMResponse) -> Self {
        self.responses.push_back(response);
        self
    }

    /// Add a text response
    pub fn with_text_response(self, text: impl Into<String>) -> Self {
        self.with_response(LLMResponse::text(text))
    }
}

impl Default for MockLLMProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LLMProvider for MockLLMProvider {
    async fn generate(
        &self,
        _messages: &[Message],
        _config: &LLMConfig,
    ) -> Result<LLMResponse, NodeError> {
        // In a real implementation, this would pop from a RefCell or similar
        // For now, just return a default response
        Ok(LLMResponse::text("Mock response"))
    }

    fn name(&self) -> &str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;
    use std::sync::{Arc, RwLock};

    #[test]
    fn test_llm_config() {
        let config = LLMConfig::new("gpt-4")
            .max_tokens(1000)
            .temperature(0.5)
            .system_prompt("You are helpful");

        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.max_tokens, Some(1000));
        assert_eq!(config.temperature, Some(0.5));
        assert_eq!(config.system_prompt, Some("You are helpful".to_string()));
    }

    #[test]
    fn test_llm_response() {
        let response = LLMResponse::text("Hello");
        assert_eq!(response.content, "Hello");
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.stop_reason, StopReason::EndTurn);

        let tool_call = ToolCall::new("1", "get_weather", serde_json::json!({}));
        let response = LLMResponse::with_tool_calls("Checking weather", vec![tool_call]);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.stop_reason, StopReason::ToolUse);
    }

    #[tokio::test]
    async fn test_llm_node_with_mock() {
        let provider = MockLLMProvider::new();
        let node = LLMNode::with_provider("llm", provider);

        let mut state = AgentState::new();
        state.add_user_message("Hello");
        let shared = Arc::new(RwLock::new(state));

        let result = node.execute(shared.clone()).await.unwrap();
        assert!(!result.is_terminal());

        let guard = shared.read().unwrap();
        assert_eq!(guard.messages.len(), 2); // user + assistant
        assert!(guard.is_complete); // No tool calls, so marked complete
    }
}
