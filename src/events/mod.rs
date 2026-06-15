//! Event streaming and observability for oxidizedgraph
//!
//! This module provides a publish-subscribe event system for monitoring
//! graph execution, implementing hooks, and building observability.
//!
//! # Event Types
//!
//! - `GraphStarted` / `GraphCompleted` - Graph lifecycle
//! - `NodeEntered` / `NodeExited` - Node execution boundaries
//! - `NodeError` - Node execution failures
//! - `CheckpointSaved` - Checkpoint persistence
//! - `StateUpdated` - State mutations
//!
//! # Example
//!
//! ```rust,ignore
//! use oxidizedgraph::events::{EventBus, Event, EventHandler};
//!
//! // Create event bus
//! let bus = EventBus::new();
//!
//! // Subscribe to events
//! bus.subscribe(|event: &Event| {
//!     println!("Event: {:?}", event);
//! });
//!
//! // Use with streaming runner
//! let runner = StreamingRunner::new(graph, bus.clone());
//! ```

mod bus;
mod handler;
mod runner;
mod types;

pub use bus::{EventBus, EventReceiver, EventSubscription};
pub use handler::{
    spawn_handler, EventHandler, LoggingHandler, MetricsHandler, MetricsSummary, NodeMetrics,
};
pub use runner::{StreamingRunResult, StreamingRunner};
pub use types::{CheckpointEvent, Event, EventKind, GraphEvent, NodeEvent, StateEvent};

use std::sync::Arc;

/// Create a new event bus with default configuration
pub fn new_event_bus() -> Arc<EventBus> {
    Arc::new(EventBus::new())
}

/// Create an event bus with a custom channel capacity
pub fn new_event_bus_with_capacity(capacity: usize) -> Arc<EventBus> {
    Arc::new(EventBus::with_capacity(capacity))
}
