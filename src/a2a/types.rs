//! Core A2A data types (JSON-RPC binding subset).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Task lifecycle state (A2A v1 enum strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskState {
    /// Task accepted, worker not yet running.
    TaskStateSubmitted,
    /// Worker Job is active.
    TaskStateWorking,
    /// Task finished successfully.
    TaskStateCompleted,
    /// Task failed.
    TaskStateFailed,
    /// Task canceled by client.
    TaskStateCanceled,
    /// Task rejected by orchestrator.
    TaskStateRejected,
    /// Awaiting additional user input.
    TaskStateInputRequired,
    /// Awaiting authorization credentials.
    TaskStateAuthRequired,
}

impl TaskState {
    /// Whether the task has reached a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::TaskStateCompleted
                | Self::TaskStateFailed
                | Self::TaskStateCanceled
                | Self::TaskStateRejected
        )
    }
}

/// Message role in an A2A conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    /// End-user or upstream client.
    RoleUser,
    /// Orchestrator or worker agent.
    RoleAgent,
}

/// One content part inside a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePart {
    /// Text payload.
    pub text: String,
}

/// An A2A message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Sender role.
    pub role: Role,
    /// Message body parts.
    pub parts: Vec<MessagePart>,
    /// Unique message id.
    pub message_id: String,
    /// Optional task this message belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Optional conversation context id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

impl Message {
    /// Build a user message from plain text.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::RoleUser,
            parts: vec![MessagePart {
                text: text.into(),
            }],
            message_id: Uuid::new_v4().to_string(),
            task_id: None,
            context_id: None,
        }
    }
}

/// Current status of a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    /// Lifecycle state.
    pub state: TaskState,
    /// RFC3339 timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Optional status message from the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
}

/// An A2A task tracked by the orchestrator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Task id.
    pub id: String,
    /// Conversation context id.
    pub context_id: String,
    /// Current status.
    pub status: TaskStatus,
    /// Associated worker Job name, when spawned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_job: Option<String>,
    /// Original user message that created the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Message>,
}

impl Task {
    /// Create a newly submitted task from an inbound message.
    pub fn submitted(message: &Message) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            context_id: message
                .context_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            status: TaskStatus {
                state: TaskState::TaskStateSubmitted,
                timestamp: Some(now),
                message: None,
            },
            worker_job: None,
            input: Some(message.clone()),
        }
    }

    /// Mark the task as working and attach the worker Job name.
    pub fn mark_working(&mut self, job_name: impl Into<String>) {
        self.worker_job = Some(job_name.into());
        self.status = TaskStatus {
            state: TaskState::TaskStateWorking,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            message: None,
        };
    }

    /// Mark the task failed with a reason.
    pub fn mark_failed(&mut self, reason: impl Into<String>) {
        self.status = TaskStatus {
            state: TaskState::TaskStateFailed,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            message: Some(Message {
                role: Role::RoleAgent,
                parts: vec![MessagePart {
                    text: reason.into(),
                }],
                message_id: Uuid::new_v4().to_string(),
                task_id: Some(self.id.clone()),
                context_id: Some(self.context_id.clone()),
            }),
        };
    }
}

/// SendMessage configuration flags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageConfiguration {
    /// When true, return immediately after task creation (orchestrator default).
    #[serde(default)]
    pub return_immediately: bool,
}

/// SendMessage request params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Inbound message.
    pub message: Message,
    /// Optional configuration.
    #[serde(default)]
    pub configuration: SendMessageConfiguration,
}

/// SendMessage response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResponse {
    /// Created or updated task.
    pub task: Task,
}

/// Agent Card advertised at `/.well-known/agent-card.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    /// Agent display name.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Protocol version.
    pub protocol_version: String,
    /// Supported JSON-RPC methods.
    pub capabilities: AgentCapabilities,
    /// Public JSON-RPC endpoint (relative or absolute).
    pub url: String,
}

/// Capability flags for the orchestrator agent card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    /// Supports SendMessage.
    pub send_message: bool,
    /// Supports GetTask.
    pub get_task: bool,
    /// Supports CancelTask.
    pub cancel_task: bool,
}

impl AgentCard {
    /// Default orchestrator card for issue #41.
    pub fn orchestrator(base_url: &str) -> Self {
        Self {
            name: "oxidizedgraph-orchestrator".into(),
            description: "Sovereign graph orchestrator — dispatches build tasks to ephemeral AKS worker Jobs via A2A JSON-RPC".into(),
            protocol_version: "0.3".into(),
            capabilities: AgentCapabilities {
                send_message: true,
                get_task: true,
                cancel_task: true,
            },
            url: format!("{base_url}/rpc"),
        }
    }
}
