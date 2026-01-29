//! Example: Streaming events for observability
//!
//! This example demonstrates how to use the event system for:
//! - Logging graph execution
//! - Collecting metrics
//! - Building custom observability
//!
//! Run with: cargo run --example streaming_events

use async_trait::async_trait;
use oxidizedgraph::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// A node that simulates work
struct WorkerNode {
    id: String,
    work_duration_ms: u64,
    next: Option<String>,
}

#[async_trait]
impl NodeExecutor for WorkerNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        // Simulate work
        tokio::time::sleep(Duration::from_millis(self.work_duration_ms)).await;

        // Update state
        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            let count = guard.get_context::<i32>("processed").unwrap_or(0);
            guard.set_context("processed", count + 1);
        }

        match &self.next {
            Some(target) => Ok(NodeOutput::continue_to(target.clone())),
            None => Ok(NodeOutput::finish()),
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Simulates work by sleeping")
    }
}

/// Custom event handler that tracks timing
struct TimingHandler {
    start_times: std::sync::RwLock<std::collections::HashMap<String, std::time::Instant>>,
}

impl TimingHandler {
    fn new() -> Self {
        Self {
            start_times: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait]
impl EventHandler for TimingHandler {
    async fn handle(&self, event: &Event) {
        match &event.kind {
            EventKind::Node(NodeEvent::Entered { node_id, .. }) => {
                if let Ok(mut times) = self.start_times.write() {
                    times.insert(node_id.clone(), std::time::Instant::now());
                }
                println!("⏱️  Started: {}", node_id);
            }
            EventKind::Node(NodeEvent::Exited { node_id, duration_ms, .. }) => {
                println!("✅ Finished: {} ({}ms)", node_id, duration_ms);
            }
            EventKind::Graph(GraphEvent::Completed { iterations, duration_ms }) => {
                println!("\n📊 Graph completed:");
                println!("   Iterations: {}", iterations);
                println!("   Total time: {}ms", duration_ms);
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build a multi-node workflow
    let graph = GraphBuilder::new()
        .name("worker_pipeline")
        .description("Simulated work pipeline with metrics")
        .add_node(WorkerNode {
            id: "fetch".to_string(),
            work_duration_ms: 50,
            next: Some("process".to_string()),
        })
        .add_node(WorkerNode {
            id: "process".to_string(),
            work_duration_ms: 100,
            next: Some("transform".to_string()),
        })
        .add_node(WorkerNode {
            id: "transform".to_string(),
            work_duration_ms: 75,
            next: Some("save".to_string()),
        })
        .add_node(WorkerNode {
            id: "save".to_string(),
            work_duration_ms: 25,
            next: None,
        })
        .set_entry_point("fetch")
        .compile()?;

    println!("=== Streaming Events Example ===\n");
    println!("Graph:\n{}", graph.to_mermaid());

    // Create event bus
    let bus = Arc::new(EventBus::new());

    // Set up multiple handlers

    // 1. Custom timing handler
    let timing_handler = Arc::new(TimingHandler::new());
    let timing_receiver = bus.subscribe();
    let timing_handle = spawn_handler(timing_handler, timing_receiver);

    // 2. Metrics collector
    let metrics = Arc::new(MetricsHandler::new());
    let metrics_receiver = bus.subscribe();
    let metrics_handle = spawn_handler(metrics.clone(), metrics_receiver);

    // Create streaming runner
    let runner = StreamingRunner::new(graph, bus.clone());

    println!("\n--- Execution Log ---\n");

    // Run the workflow
    let result = runner.invoke("demo-thread", AgentState::new()).await?;

    // Give handlers time to process final events
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Print metrics summary
    let summary = metrics.summary();
    println!("\n--- Metrics Summary ---\n");
    println!("Total events: {}", summary.total_events);
    println!("Graphs completed: {}", summary.graphs_completed);
    println!("\nNode Performance:");
    for (node_id, node_metrics) in &summary.nodes {
        println!(
            "  {}: {} executions, avg {:.1}ms",
            node_id,
            node_metrics.executions,
            node_metrics.avg_duration_ms.unwrap_or(0.0)
        );
    }

    // Print final state
    println!("\n--- Final State ---");
    println!("Items processed: {}", result.state().get_context::<i32>("processed").unwrap_or(0));

    // Cleanup
    drop(bus); // This will close the channel
    let _ = timing_handle.await;
    let _ = metrics_handle.await;

    Ok(())
}
