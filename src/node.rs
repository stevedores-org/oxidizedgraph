//! Node definitions for oxidizedgraph
//!
//! Nodes are the building blocks of your workflow. Each node:
//! - Receives the current state
//! - Performs some action (LLM call, tool execution, etc.)
//! - Returns an updated state with routing decision

use crate::error::NodeError;
use crate::state::State;
use async_trait::async_trait;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

/// Result of node execution
#[derive(Clone, Debug)]
pub enum NodeResult<S: State> {
    /// Continue to the next node with updated state.
    /// The graph will determine the next node based on edges.
    Continue(S),

    /// Interrupt execution and wait for human input.
    /// The state will be checkpointed and can be resumed later.
    Interrupt {
        /// Current state at interrupt point
        state: S,
        /// Reason for the interrupt (shown to user)
        reason: String,
    },

    /// End graph execution immediately.
    /// Use this when the task is complete, regardless of edges.
    End(S),
}

impl<S: State> NodeResult<S> {
    /// Create a Continue result
    pub fn cont(state: S) -> Self {
        Self::Continue(state)
    }

    /// Create an Interrupt result
    pub fn interrupt(state: S, reason: impl Into<String>) -> Self {
        Self::Interrupt {
            state,
            reason: reason.into(),
        }
    }

    /// Create an End result
    pub fn end(state: S) -> Self {
        Self::End(state)
    }

    /// Get the state from any result variant
    pub fn state(&self) -> &S {
        match self {
            Self::Continue(s) | Self::Interrupt { state: s, .. } | Self::End(s) => s,
        }
    }

    /// Convert to state, consuming self
    pub fn into_state(self) -> S {
        match self {
            Self::Continue(s) | Self::Interrupt { state: s, .. } | Self::End(s) => s,
        }
    }

    /// Check if this is a Continue result
    pub fn is_continue(&self) -> bool {
        matches!(self, Self::Continue(_))
    }

    /// Check if this is an Interrupt result
    pub fn is_interrupt(&self) -> bool {
        matches!(self, Self::Interrupt { .. })
    }

    /// Check if this is an End result
    pub fn is_end(&self) -> bool {
        matches!(self, Self::End(_))
    }
}

/// Configuration for automatic retries on node failure
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub multiplier: f64,
    /// Errors that should trigger a retry
    pub retry_on: RetryOn,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
            retry_on: RetryOn::All,
        }
    }
}

impl RetryConfig {
    /// Create a new retry config
    pub fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            ..Default::default()
        }
    }

    /// Set initial delay
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set max delay
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Calculate delay for a given attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = self.initial_delay.as_secs_f64() * self.multiplier.powi(attempt as i32);
        let delay = Duration::from_secs_f64(delay);
        std::cmp::min(delay, self.max_delay)
    }
}

/// Which errors should trigger a retry
#[derive(Clone, Debug)]
pub enum RetryOn {
    /// Retry on any error
    All,
    /// Retry only on specific error types
    Specific(Vec<String>),
    /// Never retry
    None,
}

/// A node in the graph workflow.
///
/// Nodes are the fundamental execution units. Each node:
/// 1. Receives the current state
/// 2. Performs computation (LLM calls, tool execution, etc.)
/// 3. Returns updated state with routing decision
#[async_trait]
pub trait Node<S: State>: Send + Sync {
    /// Unique identifier for this node.
    /// Must be unique within a graph.
    fn id(&self) -> &str;

    /// Execute the node logic.
    async fn run(&self, state: S) -> Result<NodeResult<S>, NodeError>;

    /// Optional retry configuration.
    fn retry_config(&self) -> Option<RetryConfig> {
        None
    }

    /// Optional timeout for this node's execution.
    fn timeout(&self) -> Option<Duration> {
        None
    }

    /// Human-readable description of what this node does.
    fn description(&self) -> Option<&str> {
        None
    }
}

/// A boxed node for type erasure
pub type BoxedNode<S> = Box<dyn Node<S>>;

/// A node implemented as a closure.
///
/// This provides a more ergonomic way to define simple nodes
/// without creating a new struct.
///
/// # Example
///
/// ```rust,ignore
/// let node = FnNode::new("increment", |mut state: MyState| async move {
///     state.counter += 1;
///     Ok(NodeResult::Continue(state))
/// });
/// ```
pub struct FnNode<S, F, Fut>
where
    S: State,
    F: Fn(S) -> Fut + Send + Sync,
    Fut: Future<Output = Result<NodeResult<S>, NodeError>> + Send,
{
    id: String,
    func: F,
    retry_config: Option<RetryConfig>,
    timeout: Option<Duration>,
    description: Option<String>,
    _phantom: std::marker::PhantomData<(S, Fut)>,
}

impl<S, F, Fut> FnNode<S, F, Fut>
where
    S: State,
    F: Fn(S) -> Fut + Send + Sync,
    Fut: Future<Output = Result<NodeResult<S>, NodeError>> + Send,
{
    /// Create a new function-based node
    pub fn new(id: impl Into<String>, func: F) -> Self {
        Self {
            id: id.into(),
            func,
            retry_config: None,
            timeout: None,
            description: None,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set retry configuration
    pub fn with_retry(mut self, config: RetryConfig) -> Self {
        self.retry_config = Some(config);
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

#[async_trait]
impl<S, F, Fut> Node<S> for FnNode<S, F, Fut>
where
    S: State,
    F: Fn(S) -> Fut + Send + Sync,
    Fut: Future<Output = Result<NodeResult<S>, NodeError>> + Send,
{
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(&self, state: S) -> Result<NodeResult<S>, NodeError> {
        (self.func)(state).await
    }

    fn retry_config(&self) -> Option<RetryConfig> {
        self.retry_config.clone()
    }

    fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

/// A node that wraps another node with middleware-like behavior.
pub struct WrappedNode<S: State, Inner: Node<S>> {
    inner: Inner,
    before: Option<Arc<dyn Fn(&S) + Send + Sync>>,
    after: Option<Arc<dyn Fn(&NodeResult<S>) + Send + Sync>>,
    _phantom: std::marker::PhantomData<S>,
}

impl<S: State, Inner: Node<S>> WrappedNode<S, Inner> {
    /// Create a new wrapped node
    pub fn new(inner: Inner) -> Self {
        Self {
            inner,
            before: None,
            after: None,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Add a callback to run before the node executes
    pub fn before(mut self, f: impl Fn(&S) + Send + Sync + 'static) -> Self {
        self.before = Some(Arc::new(f));
        self
    }

    /// Add a callback to run after the node executes
    pub fn after(mut self, f: impl Fn(&NodeResult<S>) + Send + Sync + 'static) -> Self {
        self.after = Some(Arc::new(f));
        self
    }
}

#[async_trait]
impl<S: State, Inner: Node<S>> Node<S> for WrappedNode<S, Inner> {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn run(&self, state: S) -> Result<NodeResult<S>, NodeError> {
        if let Some(ref before) = self.before {
            before(&state);
        }

        let result = self.inner.run(state).await?;

        if let Some(ref after) = self.after {
            after(&result);
        }

        Ok(result)
    }

    fn retry_config(&self) -> Option<RetryConfig> {
        self.inner.retry_config()
    }

    fn timeout(&self) -> Option<Duration> {
        self.inner.timeout()
    }

    fn description(&self) -> Option<&str> {
        self.inner.description()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    struct TestState {
        counter: u32,
    }

    impl State for TestState {
        fn schema() -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
    }

    #[tokio::test]
    async fn test_node_result() {
        let state = TestState { counter: 42 };

        let result = NodeResult::Continue(state.clone());
        assert!(result.is_continue());
        assert_eq!(result.state().counter, 42);

        let result = NodeResult::End(state.clone());
        assert!(result.is_end());

        let result = NodeResult::Interrupt {
            state,
            reason: "Need input".to_string(),
        };
        assert!(result.is_interrupt());
    }

    #[tokio::test]
    async fn test_retry_config() {
        let config = RetryConfig::new(5)
            .with_initial_delay(Duration::from_millis(50))
            .with_max_delay(Duration::from_secs(5));

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(50));

        let delay0 = config.delay_for_attempt(0);
        let delay1 = config.delay_for_attempt(1);

        assert!(delay1 > delay0);
    }
}
