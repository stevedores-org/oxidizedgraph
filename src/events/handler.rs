//! Event handlers for common observability patterns

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tracing::{debug, error, info, warn};

use super::types::{Event, EventKind, GraphEvent, NodeEvent, CheckpointEvent};
use super::bus::EventReceiver;

/// Trait for handling events
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an event
    async fn handle(&self, event: &Event);

    /// Called when the handler is started
    async fn on_start(&self) {}

    /// Called when the handler is stopped
    async fn on_stop(&self) {}
}

/// Logging event handler
///
/// Logs all events using the `tracing` crate at appropriate levels.
#[derive(Default)]
pub struct LoggingHandler {
    /// Whether to include detailed event data in logs
    pub verbose: bool,
}

impl LoggingHandler {
    /// Create a new logging handler
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a verbose logging handler
    pub fn verbose() -> Self {
        Self { verbose: true }
    }
}

#[async_trait]
impl EventHandler for LoggingHandler {
    async fn handle(&self, event: &Event) {
        match &event.kind {
            EventKind::Graph(graph_event) => match graph_event {
                GraphEvent::Started { graph_name, entry_point } => {
                    info!(
                        thread_id = %event.thread_id,
                        graph_name = ?graph_name,
                        entry_point = %entry_point,
                        "Graph execution started"
                    );
                }
                GraphEvent::Completed { iterations, duration_ms } => {
                    info!(
                        thread_id = %event.thread_id,
                        iterations = iterations,
                        duration_ms = duration_ms,
                        "Graph execution completed"
                    );
                }
                GraphEvent::Error { error } => {
                    error!(
                        thread_id = %event.thread_id,
                        error = %error,
                        "Graph execution failed"
                    );
                }
                GraphEvent::Interrupted { reason, node_id } => {
                    warn!(
                        thread_id = %event.thread_id,
                        node_id = %node_id,
                        reason = %reason,
                        "Graph execution interrupted"
                    );
                }
            },
            EventKind::Node(node_event) => match node_event {
                NodeEvent::Entered { node_id, iteration } => {
                    debug!(
                        thread_id = %event.thread_id,
                        node_id = %node_id,
                        iteration = iteration,
                        "Entering node"
                    );
                }
                NodeEvent::Exited { node_id, next_node, duration_ms } => {
                    debug!(
                        thread_id = %event.thread_id,
                        node_id = %node_id,
                        next_node = ?next_node,
                        duration_ms = duration_ms,
                        "Exited node"
                    );
                }
                NodeEvent::Error { node_id, error } => {
                    error!(
                        thread_id = %event.thread_id,
                        node_id = %node_id,
                        error = %error,
                        "Node execution failed"
                    );
                }
                NodeEvent::Retrying { node_id, attempt, delay_ms } => {
                    warn!(
                        thread_id = %event.thread_id,
                        node_id = %node_id,
                        attempt = attempt,
                        delay_ms = delay_ms,
                        "Retrying node execution"
                    );
                }
            },
            EventKind::Checkpoint(checkpoint_event) => match checkpoint_event {
                CheckpointEvent::Saved { checkpoint_id, node_id } => {
                    debug!(
                        thread_id = %event.thread_id,
                        checkpoint_id = %checkpoint_id,
                        node_id = %node_id,
                        "Checkpoint saved"
                    );
                }
                CheckpointEvent::Restored { checkpoint_id, node_id } => {
                    info!(
                        thread_id = %event.thread_id,
                        checkpoint_id = %checkpoint_id,
                        node_id = %node_id,
                        "Checkpoint restored"
                    );
                }
                CheckpointEvent::Deleted { checkpoint_id } => {
                    debug!(
                        thread_id = %event.thread_id,
                        checkpoint_id = %checkpoint_id,
                        "Checkpoint deleted"
                    );
                }
            },
            EventKind::State(state_event) => {
                if self.verbose {
                    debug!(
                        thread_id = %event.thread_id,
                        event = ?state_event,
                        "State updated"
                    );
                }
            }
            EventKind::Custom { name, payload } => {
                debug!(
                    thread_id = %event.thread_id,
                    name = %name,
                    payload = ?payload,
                    "Custom event"
                );
            }
        }
    }
}

/// Metrics collection handler
///
/// Collects execution metrics for monitoring and analysis.
#[derive(Default)]
pub struct MetricsHandler {
    /// Total events processed
    pub events_processed: AtomicU64,
    /// Node execution counts
    node_executions: RwLock<HashMap<String, u64>>,
    /// Node execution times (total ms)
    node_durations: RwLock<HashMap<String, u64>>,
    /// Error counts by node
    node_errors: RwLock<HashMap<String, u64>>,
    /// Graph completions
    pub graphs_completed: AtomicU64,
    /// Graph errors
    pub graphs_errored: AtomicU64,
    /// Checkpoints saved
    pub checkpoints_saved: AtomicU64,
}

impl MetricsHandler {
    /// Create a new metrics handler
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total events processed
    pub fn total_events(&self) -> u64 {
        self.events_processed.load(Ordering::Relaxed)
    }

    /// Get node execution count
    pub fn node_execution_count(&self, node_id: &str) -> u64 {
        self.node_executions
            .read()
            .ok()
            .and_then(|m| m.get(node_id).copied())
            .unwrap_or(0)
    }

    /// Get node average execution time in milliseconds
    pub fn node_avg_duration_ms(&self, node_id: &str) -> Option<f64> {
        let executions = self.node_executions.read().ok()?;
        let durations = self.node_durations.read().ok()?;

        let count = *executions.get(node_id)?;
        let total = *durations.get(node_id)?;

        if count > 0 {
            Some(total as f64 / count as f64)
        } else {
            None
        }
    }

    /// Get node error count
    pub fn node_error_count(&self, node_id: &str) -> u64 {
        self.node_errors
            .read()
            .ok()
            .and_then(|m| m.get(node_id).copied())
            .unwrap_or(0)
    }

    /// Get all node IDs that have been executed
    pub fn node_ids(&self) -> Vec<String> {
        self.node_executions
            .read()
            .ok()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Get a summary of all metrics
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            total_events: self.total_events(),
            graphs_completed: self.graphs_completed.load(Ordering::Relaxed),
            graphs_errored: self.graphs_errored.load(Ordering::Relaxed),
            checkpoints_saved: self.checkpoints_saved.load(Ordering::Relaxed),
            nodes: self
                .node_ids()
                .into_iter()
                .map(|id| {
                    let executions = self.node_execution_count(&id);
                    let avg_duration = self.node_avg_duration_ms(&id);
                    let errors = self.node_error_count(&id);
                    (
                        id,
                        NodeMetrics {
                            executions,
                            avg_duration_ms: avg_duration,
                            errors,
                        },
                    )
                })
                .collect(),
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.events_processed.store(0, Ordering::Relaxed);
        self.graphs_completed.store(0, Ordering::Relaxed);
        self.graphs_errored.store(0, Ordering::Relaxed);
        self.checkpoints_saved.store(0, Ordering::Relaxed);

        if let Ok(mut m) = self.node_executions.write() {
            m.clear();
        }
        if let Ok(mut m) = self.node_durations.write() {
            m.clear();
        }
        if let Ok(mut m) = self.node_errors.write() {
            m.clear();
        }
    }
}

#[async_trait]
impl EventHandler for MetricsHandler {
    async fn handle(&self, event: &Event) {
        self.events_processed.fetch_add(1, Ordering::Relaxed);

        match &event.kind {
            EventKind::Graph(GraphEvent::Completed { .. }) => {
                self.graphs_completed.fetch_add(1, Ordering::Relaxed);
            }
            EventKind::Graph(GraphEvent::Error { .. }) => {
                self.graphs_errored.fetch_add(1, Ordering::Relaxed);
            }
            EventKind::Node(NodeEvent::Exited { node_id, duration_ms, .. }) => {
                if let Ok(mut m) = self.node_executions.write() {
                    *m.entry(node_id.clone()).or_insert(0) += 1;
                }
                if let Ok(mut m) = self.node_durations.write() {
                    *m.entry(node_id.clone()).or_insert(0) += duration_ms;
                }
            }
            EventKind::Node(NodeEvent::Error { node_id, .. }) => {
                if let Ok(mut m) = self.node_errors.write() {
                    *m.entry(node_id.clone()).or_insert(0) += 1;
                }
            }
            EventKind::Checkpoint(CheckpointEvent::Saved { .. }) => {
                self.checkpoints_saved.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

/// Summary of collected metrics
#[derive(Clone, Debug)]
pub struct MetricsSummary {
    /// Total events processed
    pub total_events: u64,
    /// Graphs completed successfully
    pub graphs_completed: u64,
    /// Graphs that errored
    pub graphs_errored: u64,
    /// Checkpoints saved
    pub checkpoints_saved: u64,
    /// Per-node metrics
    pub nodes: HashMap<String, NodeMetrics>,
}

/// Metrics for a single node
#[derive(Clone, Debug)]
pub struct NodeMetrics {
    /// Number of executions
    pub executions: u64,
    /// Average execution time in milliseconds
    pub avg_duration_ms: Option<f64>,
    /// Number of errors
    pub errors: u64,
}

/// Run an event handler on a receiver in a background task
pub fn spawn_handler<H: EventHandler + 'static>(
    handler: Arc<H>,
    mut receiver: EventReceiver,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        handler.on_start().await;

        while let Some(event) = receiver.recv().await {
            handler.handle(&event).await;
        }

        handler.on_stop().await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_logging_handler() {
        let handler = LoggingHandler::new();

        let event = Event::graph_started("thread-1", Some("test".to_string()), "start".to_string());
        handler.handle(&event).await;

        let event = Event::node_entered("thread-1", "node-a".to_string(), 1);
        handler.handle(&event).await;
    }

    #[tokio::test]
    async fn test_metrics_handler() {
        let handler = MetricsHandler::new();

        // Simulate node execution
        let event = Event::node_exited(
            "thread-1",
            "processor".to_string(),
            Some("next".to_string()),
            Duration::from_millis(100),
        );
        handler.handle(&event).await;
        handler.handle(&event).await;

        assert_eq!(handler.node_execution_count("processor"), 2);
        assert_eq!(handler.node_avg_duration_ms("processor"), Some(100.0));

        // Simulate error
        let event = Event::node_error("thread-1", "processor".to_string(), "failed".to_string());
        handler.handle(&event).await;

        assert_eq!(handler.node_error_count("processor"), 1);

        // Check summary
        let summary = handler.summary();
        assert_eq!(summary.total_events, 3);
        assert!(summary.nodes.contains_key("processor"));
    }

    #[tokio::test]
    async fn test_metrics_reset() {
        let handler = MetricsHandler::new();

        let event = Event::graph_completed("thread-1", 10, Duration::from_secs(1));
        handler.handle(&event).await;

        assert_eq!(handler.graphs_completed.load(Ordering::Relaxed), 1);

        handler.reset();

        assert_eq!(handler.graphs_completed.load(Ordering::Relaxed), 0);
    }
}
