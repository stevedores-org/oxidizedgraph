//! Prelude module for oxidizedgraph
//!
//! This module re-exports the most commonly used types for convenient importing:
//!
//! ```rust,ignore
//! use oxidizedgraph::prelude::*;
//! ```

// Error types
pub use crate::error::{GraphError, NodeError, RuntimeError};

// State management
pub use crate::state::{AgentState, Message, MessageRole, SharedState, SharedStateExt, State, ToolCall};

// Graph building
pub use crate::graph::{
    transitions, BoxedNodeExecutor, CompiledGraph, EdgeType, GraphBuilder, GraphEdge, GraphNode,
    NodeExecutor, NodeOutput,
};

// Execution
pub use crate::runner::{GraphRunner, RunnerConfig, Runtime};

// Checkpointing
pub use crate::checkpoint::{
    Checkpoint, CheckpointConfig, Checkpointer, CheckpointingRunner, MemoryCheckpointer, RunResult,
};

// Built-in nodes
pub use crate::nodes::{
    ConditionalNode, ContextRouterNode, DelayNode, EchoNode, FunctionNode, LLMConfig, LLMNode,
    LLMProvider, StaticTransitionNode, Tool, ToolNode, ToolRegistry,
};

// Re-exports from dependencies
pub use async_trait::async_trait;
pub use serde::{Deserialize, Serialize};
pub use serde_json;
