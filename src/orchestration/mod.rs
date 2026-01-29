//! Multi-graph orchestration for oxidizedgraph
//!
//! This module enables composing complex workflows by:
//! - Spawning child graphs from within a parent graph
//! - Running multiple graphs in parallel
//! - Passing state between parent and child graphs
//! - Coordinating results from multiple subgraphs
//!
//! # Patterns
//!
//! ## Sequential Subgraph
//! Execute a subgraph and wait for results before continuing.
//!
//! ```rust,ignore
//! let node = SubgraphNode::new("research", research_graph)
//!     .with_state_mapper(|parent| {
//!         // Extract relevant state for child
//!         ChildState::from_parent(parent)
//!     })
//!     .with_result_merger(|parent, child_result| {
//!         // Merge child results back into parent
//!         parent.context.insert("research", child_result);
//!     });
//! ```
//!
//! ## Parallel Subgraphs
//! Execute multiple subgraphs concurrently and join results.
//!
//! ```rust,ignore
//! let node = ParallelSubgraphs::new("multi_research")
//!     .add_subgraph("web", web_graph)
//!     .add_subgraph("docs", docs_graph)
//!     .add_subgraph("code", code_graph)
//!     .with_join_strategy(JoinStrategy::WaitAll);
//! ```
//!
//! ## Dynamic Spawning
//! Spawn subgraphs dynamically based on runtime decisions.
//!
//! ```rust,ignore
//! let spawner = SubgraphSpawner::new();
//! let handle = spawner.spawn("child-1", graph, initial_state).await?;
//! let result = handle.await?;
//! ```

mod handle;
mod parallel;
mod spawner;
mod subgraph_node;

pub use handle::{SubgraphHandle, SubgraphResult};
pub use parallel::{JoinStrategy, ParallelSubgraphs};
pub use spawner::SubgraphSpawner;
pub use subgraph_node::SubgraphNode;

use crate::state::AgentState;

/// Function type for mapping parent state to child state
pub type StateMapper = Box<dyn Fn(&AgentState) -> AgentState + Send + Sync>;

/// Function type for merging child results back into parent state
pub type ResultMerger = Box<dyn Fn(&mut AgentState, AgentState) + Send + Sync>;

/// Create a state mapper that clones the entire parent state
pub fn clone_state() -> StateMapper {
    Box::new(|parent| parent.clone())
}

/// Create a state mapper that extracts specific context keys
pub fn extract_context(keys: Vec<String>) -> StateMapper {
    Box::new(move |parent| {
        let mut child = AgentState::new();
        for key in &keys {
            if let Some(value) = parent.context.get(key) {
                child.context.insert(key.clone(), value.clone());
            }
        }
        // Copy messages if present
        child.messages = parent.messages.clone();
        child
    })
}

/// Create a result merger that copies all context from child to parent
pub fn merge_all_context() -> ResultMerger {
    Box::new(|parent, child| {
        for (key, value) in child.context {
            parent.context.insert(key, value);
        }
    })
}

/// Create a result merger that copies specific keys from child to parent
pub fn merge_context_keys(keys: Vec<String>) -> ResultMerger {
    Box::new(move |parent, child| {
        for key in &keys {
            if let Some(value) = child.context.get(key) {
                parent.context.insert(key.clone(), value.clone());
            }
        }
    })
}

/// Create a result merger that stores child state under a namespace
pub fn merge_under_namespace(namespace: String) -> ResultMerger {
    Box::new(move |parent, child| {
        if let Ok(child_json) = serde_json::to_value(&child.context) {
            parent.context.insert(namespace.clone(), child_json);
        }
    })
}
