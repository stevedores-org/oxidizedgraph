//! Example: Multi-agent orchestration
//!
//! This example demonstrates:
//! - SubgraphNode: Execute a subgraph inline
//! - ParallelSubgraphs: Fan-out to multiple subgraphs
//! - SubgraphSpawner: Dynamic subgraph spawning
//!
//! Run with: cargo run --example multi_agent

use async_trait::async_trait;
use oxidizedgraph::prelude::*;
use std::time::Duration;

/// A research agent node that simulates gathering information
struct ResearcherNode {
    id: String,
    topic: String,
    delay_ms: u64,
}

#[async_trait]
impl NodeExecutor for ResearcherNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        // Simulate research work
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;

        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;

            // Store findings
            guard.set_context(
                "findings",
                format!("Research on '{}': Found 3 relevant sources.", self.topic),
            );
        }

        Ok(NodeOutput::finish())
    }

    fn description(&self) -> Option<&str> {
        Some("Researches a topic and stores findings")
    }
}

/// A synthesizer node that combines results
struct SynthesizerNode;

#[async_trait]
impl NodeExecutor for SynthesizerNode {
    fn id(&self) -> &str {
        "synthesizer"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let summary = {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;

            // Gather all research results (look for findings in various formats)
            let mut results = Vec::new();
            for key in guard.context.keys() {
                if key.contains("finding") || key.contains("result") {
                    if let Some(v) = guard.context.get(key) {
                        results.push(format!("  - {}: {:?}", key, v));
                    }
                }
            }
            if results.is_empty() {
                "No research findings found.".to_string()
            } else {
                results.join("\n")
            }
        };

        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("final_summary", format!("Synthesis complete:\n{}", summary));
        }

        Ok(NodeOutput::finish())
    }
}

fn create_researcher_graph(topic: &str, delay_ms: u64) -> CompiledGraph {
    GraphBuilder::new()
        .name(&format!("{}_researcher", topic))
        .add_node(ResearcherNode {
            id: "research".to_string(),
            topic: topic.to_string(),
            delay_ms,
        })
        .set_entry_point("research")
        .compile()
        .unwrap()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Multi-Agent Orchestration Example ===\n");

    // ---------------------------------------------------------
    // Pattern 1: SubgraphNode - Sequential subgraph execution
    // ---------------------------------------------------------
    println!("--- Pattern 1: SubgraphNode (Sequential) ---\n");

    let research_subgraph = create_researcher_graph("AI", 50);

    // Create a graph with an embedded subgraph node
    let coordinator_graph = GraphBuilder::new()
        .name("coordinator")
        .add_node(
            SubgraphNode::new("ai_research", research_subgraph)
                .with_result_merger(|parent, child| {
                    // Copy findings to parent with prefix
                    if let Some(findings) = child.get_context::<String>("findings") {
                        parent.set_context("ai_findings", findings);
                    }
                }),
        )
        .add_node(SynthesizerNode)
        .set_entry_point("ai_research")
        .add_edge("ai_research", "synthesizer")
        .add_edge_to_end("synthesizer")
        .compile()?;

    let runner = GraphRunner::with_defaults(coordinator_graph);
    let result = runner.invoke(AgentState::new()).await?;

    println!("SubgraphNode Result:");
    if let Some(summary) = result.get_context::<String>("final_summary") {
        println!("  {}\n", summary.replace('\n', "\n  "));
    }

    // ---------------------------------------------------------
    // Pattern 2: ParallelSubgraphs - Fan-out execution
    // ---------------------------------------------------------
    println!("--- Pattern 2: ParallelSubgraphs (Fan-out) ---\n");

    // Create parallel subgraphs with custom result mergers
    let parallel_node = ParallelSubgraphs::new("multi_research")
        .add_subgraph_with_handlers(
            "web",
            create_researcher_graph("web_sources", 30),
            |_parent| AgentState::new(), // Fresh state for each
            |parent, child| {
                if let Some(f) = child.get_context::<String>("findings") {
                    parent.set_context("web_findings", f);
                }
            },
        )
        .add_subgraph_with_handlers(
            "docs",
            create_researcher_graph("documentation", 50),
            |_parent| AgentState::new(),
            |parent, child| {
                if let Some(f) = child.get_context::<String>("findings") {
                    parent.set_context("docs_findings", f);
                }
            },
        )
        .add_subgraph_with_handlers(
            "code",
            create_researcher_graph("code_examples", 40),
            |_parent| AgentState::new(),
            |parent, child| {
                if let Some(f) = child.get_context::<String>("findings") {
                    parent.set_context("code_findings", f);
                }
            },
        )
        .with_join_strategy(JoinStrategy::WaitAll)
        .then("synthesizer");

    let parallel_graph = GraphBuilder::new()
        .name("parallel_research")
        .add_node(parallel_node)
        .add_node(SynthesizerNode)
        .set_entry_point("multi_research")
        .add_edge_to_end("synthesizer")
        .compile()?;

    let runner = GraphRunner::with_defaults(parallel_graph);
    let start = std::time::Instant::now();
    let result = runner.invoke(AgentState::new()).await?;
    let elapsed = start.elapsed();

    println!("ParallelSubgraphs Result (completed in {:?}):", elapsed);
    if let Some(summary) = result.get_context::<String>("final_summary") {
        println!("  {}\n", summary.replace('\n', "\n  "));
    }
    println!("(Note: Parallel execution ~50ms is faster than sequential 30+50+40=120ms)\n");

    // ---------------------------------------------------------
    // Pattern 3: SubgraphSpawner - Dynamic spawning
    // ---------------------------------------------------------
    println!("--- Pattern 3: SubgraphSpawner (Dynamic) ---\n");

    let spawner = SubgraphSpawner::new();

    // Dynamically decide what to research based on runtime conditions
    let topics = vec!["machine_learning", "databases", "networking"];

    println!("Spawning {} research tasks dynamically...", topics.len());

    let results = spawner
        .builder()
        .spawn("ml", create_researcher_graph("machine_learning", 25), AgentState::new())
        .spawn("db", create_researcher_graph("databases", 35), AgentState::new())
        .spawn("net", create_researcher_graph("networking", 20), AgentState::new())
        .join_all()
        .await;

    println!("\nDynamic Spawn Results:");
    for result in results {
        match result {
            SubgraphResult::Completed { subgraph_id, state } => {
                let findings = state.get_context::<String>("findings").unwrap_or_default();
                println!("  [{}] {}", subgraph_id, findings);
            }
            SubgraphResult::Failed { subgraph_id, error } => {
                println!("  [{}] FAILED: {}", subgraph_id, error);
            }
            SubgraphResult::Cancelled { subgraph_id } => {
                println!("  [{}] CANCELLED", subgraph_id);
            }
        }
    }

    println!("\n=== All patterns demonstrated successfully ===");

    Ok(())
}
