//! Error types for oxidizedgraph
//!
//! Provides comprehensive error handling for graph construction, node execution,
//! and runtime operations.

use thiserror::Error;

/// Errors that can occur during graph construction
#[derive(Error, Debug)]
pub enum GraphError {
    /// The graph has no entry point defined
    #[error("Graph has no entry point defined")]
    NoEntryPoint,

    /// A referenced node was not found in the graph
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Invalid edge target
    #[error("Invalid edge target from '{from}' to '{to}'")]
    InvalidEdgeTarget {
        /// Source node
        from: String,
        /// Target node
        to: String,
    },

    /// Duplicate node ID was registered
    #[error("Duplicate node ID: {0}")]
    DuplicateNode(String),

    /// The graph contains a cycle that would cause infinite execution
    #[error("Graph contains a cycle: {0}")]
    CycleDetected(String),

    /// Invalid graph configuration
    #[error("Invalid graph configuration: {0}")]
    InvalidConfiguration(String),
}

/// Errors that can occur during node execution
#[derive(Error, Debug)]
pub enum NodeError {
    /// A generic execution failure
    #[error("Node execution failed: {0}")]
    ExecutionFailed(String),

    /// The node timed out
    #[error("Node timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// An LLM API error
    #[error("LLM error: {0}")]
    LLMError(String),

    /// Tool execution error
    #[error("Tool error: {0}")]
    ToolError(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// HTTP request error
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Generic error wrapper
    #[error("{0}")]
    Other(String),
}

impl NodeError {
    /// Create a new execution failed error
    pub fn execution_failed(msg: impl Into<String>) -> Self {
        Self::ExecutionFailed(msg.into())
    }

    /// Create a new LLM error
    pub fn llm_error(msg: impl Into<String>) -> Self {
        Self::LLMError(msg.into())
    }

    /// Create a new tool error
    pub fn tool_error(msg: impl Into<String>) -> Self {
        Self::ToolError(msg.into())
    }

    /// Create a new other error
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

/// Errors that can occur during runtime execution
#[derive(Error, Debug)]
pub enum RuntimeError {
    /// The graph exceeded its recursion limit
    #[error("Recursion limit exceeded: {0} iterations")]
    RecursionLimit(u32),

    /// A node was not found during execution
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Node execution failed
    #[error("Node '{node_id}' failed: {error}")]
    NodeFailed {
        /// The ID of the node that failed
        node_id: String,
        /// The underlying error
        error: NodeError,
    },

    /// Execution was interrupted for human input
    #[error("Interrupted: {reason}")]
    Interrupted {
        /// The reason for the interrupt
        reason: String,
    },

    /// No checkpointer configured but checkpoint operation requested
    #[error("No checkpointer configured")]
    NoCheckpointer,

    /// Checkpoint not found
    #[error("Checkpoint not found for thread: {0}")]
    CheckpointNotFound(String),

    /// Graph error during execution
    #[error("Graph error: {0}")]
    GraphError(#[from] GraphError),

    /// Invalid state
    #[error("Invalid state: {0}")]
    InvalidState(String),
}

impl RuntimeError {
    /// Create a node failed error
    pub fn node_failed(node_id: impl Into<String>, error: NodeError) -> Self {
        Self::NodeFailed {
            node_id: node_id.into(),
            error,
        }
    }

    /// Create an interrupted error
    pub fn interrupted(reason: impl Into<String>) -> Self {
        Self::Interrupted {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_error_display() {
        let err = GraphError::NoEntryPoint;
        assert_eq!(err.to_string(), "Graph has no entry point defined");

        let err = GraphError::NodeNotFound("my_node".to_string());
        assert_eq!(err.to_string(), "Node not found: my_node");
    }

    #[test]
    fn test_node_error_display() {
        let err = NodeError::execution_failed("Something went wrong");
        assert_eq!(err.to_string(), "Node execution failed: Something went wrong");

        let err = NodeError::Timeout(std::time::Duration::from_secs(30));
        assert!(err.to_string().contains("30"));
    }

    #[test]
    fn test_runtime_error_display() {
        let err = RuntimeError::RecursionLimit(100);
        assert_eq!(err.to_string(), "Recursion limit exceeded: 100 iterations");

        let err = RuntimeError::interrupted("Need user input");
        assert_eq!(err.to_string(), "Interrupted: Need user input");
    }
}
