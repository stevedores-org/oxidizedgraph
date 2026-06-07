//! Run context and transition logging for traceable execution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::graph::NodeOutput;
use crate::state::AgentState;

/// Correlation context for a single graph run.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RunContext {
    /// Unique identifier for this run
    pub run_id: String,
    /// Thread/session identifier for checkpointing
    pub thread_id: String,
    /// Optional seed for deterministic tool stubs during replay
    pub seed: Option<u64>,
    /// Tags for categorizing runs (e.g. epic, phase)
    pub tags: Vec<String>,
}

impl RunContext {
    /// Create a new run context with generated IDs.
    pub fn new() -> Self {
        Self {
            run_id: Uuid::new_v4().to_string(),
            thread_id: Uuid::new_v4().to_string(),
            seed: None,
            tags: Vec::new(),
        }
    }

    /// Create a run context with explicit IDs.
    pub fn with_ids(run_id: impl Into<String>, thread_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            thread_id: thread_id.into(),
            seed: None,
            tags: Vec::new(),
        }
    }

    /// Set a deterministic seed for replay tooling.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Add a tag to the run context.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

impl Default for RunContext {
    fn default() -> Self {
        Self::new()
    }
}

/// A single node transition in the execution trace.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TransitionRecord {
    /// Run this transition belongs to
    pub run_id: String,
    /// Zero-based iteration within the run
    pub iteration: u32,
    /// Node that was executed
    pub node_id: String,
    /// Serialized node output variant
    pub output_kind: String,
    /// Next node after routing (if known)
    pub next_node: Option<String>,
    /// Transition key or explicit route target from the node output
    pub output_target: Option<String>,
    /// State iteration counter after this step
    pub state_iteration: usize,
    /// Timestamp when the transition was recorded
    pub recorded_at: DateTime<Utc>,
}

impl TransitionRecord {
    /// Build a transition record from execution artifacts.
    pub fn from_step(
        run_id: &str,
        iteration: u32,
        node_id: &str,
        output: &NodeOutput,
        next_node: Option<&str>,
        state_iteration: usize,
    ) -> Self {
        Self {
            run_id: run_id.to_string(),
            iteration,
            node_id: node_id.to_string(),
            output_kind: output_kind_label(output).to_string(),
            next_node: next_node.map(|s| s.to_string()),
            output_target: output.target().map(|s| s.to_string()),
            state_iteration,
            recorded_at: Utc::now(),
        }
    }
}

fn output_kind_label(output: &NodeOutput) -> &'static str {
    match output {
        NodeOutput::Finish => "finish",
        NodeOutput::Continue(None) => "continue",
        NodeOutput::Continue(Some(_)) => "continue_to",
        NodeOutput::Route(_) => "route",
        NodeOutput::Transition(_) => "transition",
    }
}

/// Append-only log of transitions for a run.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TransitionLog {
    records: Vec<TransitionRecord>,
}

impl TransitionLog {
    /// Create an empty transition log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a transition.
    pub fn push(&mut self, record: TransitionRecord) {
        self.records.push(record);
    }

    /// All recorded transitions.
    pub fn records(&self) -> &[TransitionRecord] {
        &self.records
    }

    /// Number of transitions recorded.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Thread-safe shared transition log.
pub type SharedTransitionLog = Arc<RwLock<TransitionLog>>;

/// Create a new shared transition log.
pub fn shared_transition_log() -> SharedTransitionLog {
    Arc::new(RwLock::new(TransitionLog::new()))
}

/// Store run context on agent state for downstream nodes.
pub fn attach_run_context(state: &mut AgentState, ctx: &RunContext) {
    state.set_context("run_id", ctx.run_id.clone());
    state.set_context("thread_id", ctx.thread_id.clone());
    if let Some(seed) = ctx.seed {
        state.set_context("run_seed", seed);
    }
    if !ctx.tags.is_empty() {
        state.set_context("run_tags", ctx.tags.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transition_record_from_step() {
        let record = TransitionRecord::from_step(
            "run-1",
            0,
            "quality_gate",
            &NodeOutput::transition("failed"),
            Some("human_review"),
            1,
        );
        assert_eq!(record.run_id, "run-1");
        assert_eq!(record.output_kind, "transition");
        assert_eq!(record.next_node.as_deref(), Some("human_review"));
    }

    #[test]
    fn test_transition_log_append() {
        let mut log = TransitionLog::new();
        log.push(TransitionRecord::from_step(
            "r",
            0,
            "a",
            &NodeOutput::cont(),
            None,
            0,
        ));
        assert_eq!(log.len(), 1);
    }
}
