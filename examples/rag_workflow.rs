//! Example: RAG-augmented agent workflow
//!
//! This example demonstrates how oxidizedgraph can integrate with
//! oxidizedRAG for knowledge graph-based retrieval in agent workflows.
//!
//! The workflow:
//! 1. Receive user query
//! 2. Retrieve relevant context from knowledge graph (RAG)
//! 3. Generate response using LLM with retrieved context
//! 4. Return response
//!
//! To use with oxidizedRAG, add to Cargo.toml:
//! ```toml
//! graphrag-core = { path = "../oxidizedRAG/graphrag-core" }
//! ```

use async_trait::async_trait;
use oxidizedgraph::prelude::*;
use std::sync::Arc;

/// State for RAG-augmented workflow
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RAGState {
    /// The user's query
    pub query: String,
    /// Retrieved context from knowledge graph
    pub context: Vec<String>,
    /// Generated response
    pub response: Option<String>,
    /// Conversation messages
    pub messages: Vec<Message>,
}

impl State for RAGState {
    fn schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "context": { "type": "array", "items": { "type": "string" } },
                "response": { "type": "string" },
                "messages": { "type": "array", "channel": "append" }
            }
        })
    }
}

/// Node that retrieves context from a knowledge graph
///
/// In production, this would use graphrag-core:
/// ```rust,ignore
/// use graphrag_core::{GraphRAG, Config};
///
/// let graphrag = GraphRAG::new(Config::default())?;
/// let results = graphrag.query(&query, QueryMode::Local).await?;
/// ```
struct RAGRetrievalNode {
    // In production: graphrag: Arc<GraphRAG>
}

#[async_trait]
impl NodeExecutor for RAGRetrievalNode {
    fn id(&self) -> &str {
        "rag_retrieval"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let query = {
            let guard = state.read().map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.get_context::<String>("query").unwrap_or_default()
        };

        // Simulate RAG retrieval
        // In production, use graphrag-core:
        // let results = self.graphrag.query(&query, QueryMode::Local).await?;
        let retrieved_context = vec![
            format!("Context 1: Information relevant to '{}'", query),
            format!("Context 2: Additional details about '{}'", query),
        ];

        {
            let mut guard = state.write().map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("retrieved_context", retrieved_context);
        }

        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Retrieves relevant context from knowledge graph using GraphRAG")
    }
}

/// Node that generates a response using retrieved context
struct RAGGenerationNode;

#[async_trait]
impl NodeExecutor for RAGGenerationNode {
    fn id(&self) -> &str {
        "rag_generation"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let (query, context) = {
            let guard = state.read().map_err(|e| NodeError::execution_failed(e.to_string()))?;
            let query = guard.get_context::<String>("query").unwrap_or_default();
            let context: Vec<String> = guard.get_context("retrieved_context").unwrap_or_default();
            (query, context)
        };

        // Simulate LLM generation with context
        // In production, call your LLM API with the context
        let response = format!(
            "Based on the following context:\n{}\n\nAnswer to '{}': This is a generated response.",
            context.join("\n"),
            query
        );

        {
            let mut guard = state.write().map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("response", response.clone());
            guard.messages.push(Message::assistant(response));
        }

        Ok(NodeOutput::finish())
    }

    fn description(&self) -> Option<&str> {
        Some("Generates response using LLM with retrieved context")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Build the RAG workflow graph
    let graph = GraphBuilder::new()
        .name("rag_workflow")
        .description("RAG-augmented agent workflow")
        .add_node(RAGRetrievalNode {})
        .add_node(RAGGenerationNode)
        .set_entry_point("rag_retrieval")
        .add_edge("rag_retrieval", "rag_generation")
        .add_edge_to_end("rag_generation")
        .compile()?;

    println!("Graph structure:\n{}", graph.to_mermaid());

    // Create checkpointing runner for persistence
    let checkpointer = Arc::new(MemoryCheckpointer::new());
    let runner = CheckpointingRunner::new(graph, checkpointer.clone())
        .checkpoint_every_node();

    // Create initial state with a query
    let mut initial_state = AgentState::new();
    initial_state.set_context("query", "What is oxidizedgraph?".to_string());

    // Run the workflow
    let result = runner.invoke("rag-thread-1", initial_state).await?;

    match result {
        RunResult::Completed(state) => {
            println!("\n=== Workflow Completed ===");
            if let Some(response) = state.get_context::<String>("response") {
                println!("Response: {}", response);
            }
        }
        RunResult::Interrupted { checkpoint, reason } => {
            println!("\n=== Workflow Interrupted ===");
            println!("Reason: {}", reason);
            println!("Checkpoint ID: {}", checkpoint.id);
        }
    }

    // Show checkpoint history
    let history = checkpointer.history("rag-thread-1", 10).await?;
    println!("\n=== Checkpoint History ===");
    for cp in history {
        println!("  - {} at node '{}'", cp.id, cp.node_id);
    }

    Ok(())
}
