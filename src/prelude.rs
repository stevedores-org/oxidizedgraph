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
pub use crate::state::{
    AgentState, Message, MessageRole, SharedState, SharedStateExt, State, ToolCall,
};

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

// Events and streaming
pub use crate::events::{
    spawn_handler, Event, EventBus, EventHandler, EventKind, GraphEvent, LoggingHandler,
    MetricsHandler, NodeEvent, StreamingRunResult, StreamingRunner,
};

// Multi-graph orchestration
pub use crate::orchestration::{
    clone_state, extract_context, merge_all_context, merge_context_keys, merge_under_namespace,
    JoinStrategy, ParallelSubgraphs, SubgraphHandle, SubgraphNode, SubgraphResult, SubgraphSpawner,
};

// Deterministic execution (roadmap EPIC1)
pub use crate::execution::{
    ReplayReport, ReplayRunner, RunContext, StateValidator, TracedRunResult, TracedRunner,
    TransitionLog, TransitionRecord,
};

// Tool policy and sandbox (roadmap EPIC3)
pub use crate::tools::{
    Capability, PolicyDecision, SandboxConfig, SandboxExecutor, SubprocessSandbox,
    ToolExecutionPolicy, ToolPolicyEngine,
};

// Quality guardrails (roadmap EPIC4)
pub use crate::guardrails::{
    ChangeRisk, CommandRunner, CommandSpec, FindingSeverity, GateResult, MergeBlocker,
    QualityGateConfig, QualityGateNode, ReviewFinding, RiskClassifier, RiskLevel,
};

// Governance and role orchestration (roadmap EPIC2, issues #35/#36)
pub use crate::governance::{
    agent_role_from_state, apply_role_guidance, compose_guidance, load_manifest,
    role_system_prompt, tool_policy_for_role, tool_policy_for_state, AgentRole, GovernanceConfig,
    GovernanceError, GovernanceNode, RoleGuidance, RoleHandoffNode, RoleRouterNode,
    RoleTargetRouterNode, CTX_AGENT_ROLE, CTX_GOVERNANCE_GUIDANCE, CTX_ROLE_SYSTEM_PROMPT,
    CTX_TOOL_POLICY_ROLE,
};

// Planning and autonomy (roadmap EPIC6)
pub use crate::planning::{
    EpicPlan, PlanProgress, PlanningNode, Scheduler, SchedulerNode, Task, TaskStatus,
};

// Built-in nodes
pub use crate::nodes::tool::{AsyncFunctionTool, FunctionTool};
pub use crate::nodes::{
    ConditionalNode, ContextRouterNode, DelayNode, EchoNode, FunctionNode, LLMConfig, LLMNode,
    LLMProvider, StaticTransitionNode, Tool, ToolNode, ToolNodeConfig, ToolRegistry,
};

// Re-exports from dependencies
pub use async_trait::async_trait;
pub use serde::{Deserialize, Serialize};
pub use serde_json;
