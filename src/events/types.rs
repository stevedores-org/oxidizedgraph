//! Event type definitions

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A graph execution event
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Unique event ID
    pub id: String,

    /// When the event occurred
    pub timestamp: DateTime<Utc>,

    /// Thread/conversation ID this event belongs to
    pub thread_id: String,

    /// The specific event data
    pub kind: EventKind,
}

impl Event {
    /// Create a new event
    pub fn new(thread_id: impl Into<String>, kind: EventKind) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            thread_id: thread_id.into(),
            kind,
        }
    }

    /// Create a graph started event
    pub fn graph_started(thread_id: impl Into<String>, graph_name: Option<String>, entry_point: String) -> Self {
        Self::new(thread_id, EventKind::Graph(GraphEvent::Started {
            graph_name,
            entry_point,
        }))
    }

    /// Create a graph completed event
    pub fn graph_completed(thread_id: impl Into<String>, iterations: u32, duration: Duration) -> Self {
        Self::new(thread_id, EventKind::Graph(GraphEvent::Completed {
            iterations,
            duration_ms: duration.as_millis() as u64,
        }))
    }

    /// Create a graph error event
    pub fn graph_error(thread_id: impl Into<String>, error: String) -> Self {
        Self::new(thread_id, EventKind::Graph(GraphEvent::Error { error }))
    }

    /// Create a node entered event
    pub fn node_entered(thread_id: impl Into<String>, node_id: String, iteration: u32) -> Self {
        Self::new(thread_id, EventKind::Node(NodeEvent::Entered {
            node_id,
            iteration,
        }))
    }

    /// Create a node exited event
    pub fn node_exited(
        thread_id: impl Into<String>,
        node_id: String,
        next_node: Option<String>,
        duration: Duration,
    ) -> Self {
        Self::new(thread_id, EventKind::Node(NodeEvent::Exited {
            node_id,
            next_node,
            duration_ms: duration.as_millis() as u64,
        }))
    }

    /// Create a node error event
    pub fn node_error(thread_id: impl Into<String>, node_id: String, error: String) -> Self {
        Self::new(thread_id, EventKind::Node(NodeEvent::Error { node_id, error }))
    }

    /// Create a checkpoint saved event
    pub fn checkpoint_saved(
        thread_id: impl Into<String>,
        checkpoint_id: String,
        node_id: String,
    ) -> Self {
        Self::new(thread_id, EventKind::Checkpoint(CheckpointEvent::Saved {
            checkpoint_id,
            node_id,
        }))
    }

    /// Create a checkpoint restored event
    pub fn checkpoint_restored(
        thread_id: impl Into<String>,
        checkpoint_id: String,
        node_id: String,
    ) -> Self {
        Self::new(thread_id, EventKind::Checkpoint(CheckpointEvent::Restored {
            checkpoint_id,
            node_id,
        }))
    }

    /// Create a state updated event
    pub fn state_updated(
        thread_id: impl Into<String>,
        node_id: String,
        keys_changed: Vec<String>,
    ) -> Self {
        Self::new(thread_id, EventKind::State(StateEvent::Updated {
            node_id,
            keys_changed,
        }))
    }

    /// Check if this is a graph event
    pub fn is_graph_event(&self) -> bool {
        matches!(self.kind, EventKind::Graph(_))
    }

    /// Check if this is a node event
    pub fn is_node_event(&self) -> bool {
        matches!(self.kind, EventKind::Node(_))
    }

    /// Check if this is a checkpoint event
    pub fn is_checkpoint_event(&self) -> bool {
        matches!(self.kind, EventKind::Checkpoint(_))
    }

    /// Get the node ID if this is a node event
    pub fn node_id(&self) -> Option<&str> {
        match &self.kind {
            EventKind::Node(NodeEvent::Entered { node_id, .. }) => Some(node_id),
            EventKind::Node(NodeEvent::Exited { node_id, .. }) => Some(node_id),
            EventKind::Node(NodeEvent::Error { node_id, .. }) => Some(node_id),
            EventKind::Checkpoint(CheckpointEvent::Saved { node_id, .. }) => Some(node_id),
            EventKind::Checkpoint(CheckpointEvent::Restored { node_id, .. }) => Some(node_id),
            EventKind::State(StateEvent::Updated { node_id, .. }) => Some(node_id),
            _ => None,
        }
    }
}

/// The kind of event
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "category", content = "data")]
pub enum EventKind {
    /// Graph-level events
    Graph(GraphEvent),
    /// Node-level events
    Node(NodeEvent),
    /// Checkpoint events
    Checkpoint(CheckpointEvent),
    /// State mutation events
    State(StateEvent),
    /// Custom user-defined events
    Custom {
        /// Event name
        name: String,
        /// Event payload
        payload: serde_json::Value,
    },
}

/// Graph lifecycle events
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GraphEvent {
    /// Graph execution started
    Started {
        /// Name of the graph
        graph_name: Option<String>,
        /// Entry point node
        entry_point: String,
    },
    /// Graph execution completed successfully
    Completed {
        /// Total iterations executed
        iterations: u32,
        /// Total execution time in milliseconds
        duration_ms: u64,
    },
    /// Graph execution failed
    Error {
        /// Error message
        error: String,
    },
    /// Graph execution was interrupted (human-in-the-loop)
    Interrupted {
        /// Reason for interruption
        reason: String,
        /// Node where interruption occurred
        node_id: String,
    },
}

/// Node execution events
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NodeEvent {
    /// Node execution started
    Entered {
        /// Node ID
        node_id: String,
        /// Current iteration number
        iteration: u32,
    },
    /// Node execution completed
    Exited {
        /// Node ID
        node_id: String,
        /// Next node to execute (if any)
        next_node: Option<String>,
        /// Node execution time in milliseconds
        duration_ms: u64,
    },
    /// Node execution failed
    Error {
        /// Node ID
        node_id: String,
        /// Error message
        error: String,
    },
    /// Node is retrying after failure
    Retrying {
        /// Node ID
        node_id: String,
        /// Retry attempt number
        attempt: u32,
        /// Delay before retry in milliseconds
        delay_ms: u64,
    },
}

/// Checkpoint events
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CheckpointEvent {
    /// Checkpoint was saved
    Saved {
        /// Checkpoint ID
        checkpoint_id: String,
        /// Node where checkpoint was taken
        node_id: String,
    },
    /// Checkpoint was restored (resume)
    Restored {
        /// Checkpoint ID
        checkpoint_id: String,
        /// Node to resume from
        node_id: String,
    },
    /// Checkpoint was deleted
    Deleted {
        /// Checkpoint ID
        checkpoint_id: String,
    },
}

/// State mutation events
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StateEvent {
    /// State was updated
    Updated {
        /// Node that made the update
        node_id: String,
        /// Context keys that were changed
        keys_changed: Vec<String>,
    },
    /// Message was added to conversation
    MessageAdded {
        /// Role of the message
        role: String,
        /// Length of the message content
        content_length: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::graph_started("thread-1", Some("my_graph".to_string()), "start".to_string());

        assert_eq!(event.thread_id, "thread-1");
        assert!(event.is_graph_event());
        assert!(!event.is_node_event());
    }

    #[test]
    fn test_node_event() {
        let event = Event::node_entered("thread-1", "my_node".to_string(), 5);

        assert!(event.is_node_event());
        assert_eq!(event.node_id(), Some("my_node"));
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::node_exited(
            "thread-1",
            "processor".to_string(),
            Some("next".to_string()),
            Duration::from_millis(150),
        );

        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.thread_id, event.thread_id);
    }
}
