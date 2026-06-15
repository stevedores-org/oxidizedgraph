//! Decision memory: a queryable log of *why* changes were made.
//!
//! Each [`Decision`] records the rationale behind a non-trivial change an
//! agent run produced, tied to the artifact that change touched
//! (`change_ref` — typically a commit SHA, PR number, file path, or node
//! id). Decisions are grouped by `run_id` and tagged for cross-cutting
//! queries.
//!
//! Implementations:
//!
//! - [`MemoryDecisionStore`] — in-memory, always available. Suitable for
//!   tests and single-process agents.
//! - [`SurrealDecisionStore`] — SurrealDB-backed, gated by the
//!   `persistence` feature. Suitable for production swarms.
//!
//! # Example
//!
//! ```rust,ignore
//! use oxidizedgraph::memory::decision::{Decision, DecisionStore, MemoryDecisionStore};
//!
//! let store = MemoryDecisionStore::new();
//!
//! let decision = Decision::new(
//!     "run-42",
//!     "src/runner.rs",
//!     "Swap RwLock for parking_lot",
//!     "std RwLock can poison on panic; parking_lot avoids the recovery dance.",
//! )
//! .with_tag("perf")
//! .with_tag("locking");
//!
//! store.record(decision).await?;
//!
//! let by_run = store.list_by_run("run-42").await?;
//! assert_eq!(by_run.len(), 1);
//! # Ok::<(), oxidizedgraph::error::RuntimeError>(())
//! ```

mod memory;
#[cfg(feature = "persistence")]
mod surreal;

pub use memory::MemoryDecisionStore;
#[cfg(feature = "persistence")]
pub use surreal::SurrealDecisionStore;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::RuntimeError;

/// A single architectural or behavioral decision an agent run produced.
///
/// `change_ref` is intentionally a free-form string so callers can point at
/// whatever level of granularity makes sense (commit SHA, PR number, file
/// path, graph node id, ADR slug). Consumers that need a typed reference
/// should layer their own scheme on top of `change_ref` + `metadata`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    /// Unique identifier for this decision record.
    pub id: String,

    /// The agent run that produced this decision.
    pub run_id: String,

    /// The artifact this decision refers to (commit, PR, file, node id…).
    pub change_ref: String,

    /// Short, one-line summary of the decision.
    pub title: String,

    /// The "why" — full rationale. May be multi-paragraph markdown.
    pub rationale: String,

    /// Tags for cross-cutting queries (e.g. `"perf"`, `"security"`).
    #[serde(default)]
    pub tags: Vec<String>,

    /// When this decision was recorded.
    pub created_at: DateTime<Utc>,

    /// Free-form structured metadata (caller-defined schema).
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Decision {
    /// Create a new decision with a generated id and `created_at = now`.
    pub fn new(
        run_id: impl Into<String>,
        change_ref: impl Into<String>,
        title: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.into(),
            change_ref: change_ref.into(),
            title: title.into(),
            rationale: rationale.into(),
            tags: Vec::new(),
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Attach a tag (chainable).
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Replace tags (chainable).
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Attach structured metadata (chainable).
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Storage backend for decision memory.
///
/// Implementations must be `Send + Sync` so they can be shared across
/// tokio tasks. All listing methods return results **newest first** —
/// callers typically want the most recent rationale for an artifact.
#[async_trait]
pub trait DecisionStore: Send + Sync {
    /// Persist a decision. Overwrites any existing record with the same id.
    async fn record(&self, decision: Decision) -> Result<(), RuntimeError>;

    /// Fetch a decision by its id.
    async fn get(&self, id: &str) -> Result<Option<Decision>, RuntimeError>;

    /// Decisions produced by a specific run, newest first.
    async fn list_by_run(&self, run_id: &str) -> Result<Vec<Decision>, RuntimeError>;

    /// Decisions touching a specific artifact, newest first.
    ///
    /// Used to answer "why does this file/node/commit look the way it does?"
    async fn list_by_change(&self, change_ref: &str) -> Result<Vec<Decision>, RuntimeError>;

    /// Decisions tagged with `tag`, newest first.
    async fn list_by_tag(&self, tag: &str) -> Result<Vec<Decision>, RuntimeError>;

    /// Most recent decisions across all runs, capped at `limit`.
    async fn recent(&self, limit: usize) -> Result<Vec<Decision>, RuntimeError>;

    /// Delete a decision by id. No-op if it does not exist.
    async fn delete(&self, id: &str) -> Result<(), RuntimeError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_new_populates_fields() {
        let d = Decision::new("run-1", "src/foo.rs", "title", "rationale");
        assert!(!d.id.is_empty());
        assert_eq!(d.run_id, "run-1");
        assert_eq!(d.change_ref, "src/foo.rs");
        assert_eq!(d.title, "title");
        assert_eq!(d.rationale, "rationale");
        assert!(d.tags.is_empty());
        assert!(d.metadata.is_null());
    }

    #[test]
    fn decision_builders_chain() {
        let d = Decision::new("run-1", "PR#9", "t", "r")
            .with_tag("perf")
            .with_tag("locking")
            .with_metadata(serde_json::json!({ "owner": "alice" }));
        assert_eq!(d.tags, vec!["perf", "locking"]);
        assert_eq!(d.metadata["owner"], "alice");
    }

    #[test]
    fn decision_with_tags_replaces() {
        let d = Decision::new("run-1", "x", "t", "r")
            .with_tag("a")
            .with_tags(["b", "c"]);
        assert_eq!(d.tags, vec!["b", "c"]);
    }
}
