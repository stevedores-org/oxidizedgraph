//! A2A (Agent2Agent) JSON-RPC protocol binding for oxidizedgraph.
//!
//! Implements a focused subset of the [A2A specification](https://a2a-protocol.org/)
//! used by the sovereign graph orchestrator to dispatch build tasks to ephemeral
//! worker Jobs on Kubernetes (issue #41).

mod rpc;
mod store;
mod types;

pub use rpc::{dispatch, JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use store::TaskStore;
pub use types::{
    AgentCard, Message, MessagePart, Role, SendMessageConfiguration, SendMessageRequest,
    SendMessageResponse, Task, TaskState, TaskStatus,
};
