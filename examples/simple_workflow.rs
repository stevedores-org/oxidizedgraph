//! Simple Workflow Example
//!
//! Demonstrates a basic linear workflow: Start -> Process -> End

use oxidizedgraph::prelude::*;

/// A simple processing node
struct ProcessNode;

#[async_trait]
impl NodeExecutor for ProcessNode {
    fn id(&self) -> &str {
        "process"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        println!("Processing...");
        
        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.add_assistant_message("Processed successfully!");
            guard.set_context("processed", true);
        }
        
        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Processes the input and adds a message")
    }
}

/// A finalization node
struct FinalizeNode;

#[async_trait]
impl NodeExecutor for FinalizeNode {
    fn id(&self) -> &str {
        "finalize"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        println!("Finalizing...");
        
        {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            
            let processed: bool = guard.get_context("processed").unwrap_or(false);
            println!("Was processed: {}", processed);
        }
        
        Ok(NodeOutput::finish())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== oxidizedgraph Simple Workflow Example ===\n");

    // Build the graph
    let graph = GraphBuilder::new()
        .name("Simple Workflow")
        .description("A basic linear workflow demonstration")
        .add_node(ProcessNode)
        .add_node(FinalizeNode)
        .set_entry_point("process")
        .add_edge("process", "finalize")
        .compile()?;

    // Print the graph structure
    println!("Graph Structure (Mermaid):");
    println!("```mermaid");
    println!("{}", graph.to_mermaid());
    println!("```\n");

    // Create initial state
    let initial_state = AgentState::with_user_message("Hello, process this!");

    // Create runner and execute
    let runner = GraphRunner::new(
        graph,
        RunnerConfig::new()
            .max_iterations(10)
            .verbose(true),
    );

    println!("Starting workflow execution...\n");
    let final_state = runner.invoke(initial_state).await?;

    println!("\n=== Results ===");
    println!("Total iterations: {}", final_state.iteration);
    println!("Messages: {}", final_state.messages.len());
    
    if let Some(last) = final_state.last_message() {
        println!("Last message: {}", last.content);
    }

    Ok(())
}
