//! Cross-repository change graph for coordinated rollouts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Status of a single repository change in a coordinated rollout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoChangeStatus {
    /// Waiting on upstream repo changes.
    Pending,
    /// Agent work or PR creation in progress.
    InProgress,
    /// CI checks passed for this repo change.
    CiPassed,
    /// CI failed — may block downstream repos.
    CiFailed,
    /// Change merged and released.
    Released,
    /// Blocked because an upstream dependency failed.
    Blocked,
}

/// A change scoped to one repository within a multi-repo objective.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepoChange {
    /// Stable change identifier within the graph.
    pub id: String,
    /// Repository slug (e.g. `stevedores-org/oxidizedgraph`).
    pub repo: String,
    /// Human-readable summary of the change.
    pub description: String,
    /// Current lifecycle status.
    pub status: RepoChangeStatus,
    /// IDs of upstream changes that must complete first.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Optional PR URL once opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    /// Correlated autonomous run id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

impl RepoChange {
    /// Create a pending repo change.
    pub fn new(
        id: impl Into<String>,
        repo: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            repo: repo.into(),
            description: description.into(),
            status: RepoChangeStatus::Pending,
            dependencies: Vec::new(),
            pr_url: None,
            run_id: None,
        }
    }

    /// Depend on another change completing first.
    pub fn depends_on(mut self, change_id: impl Into<String>) -> Self {
        self.dependencies.push(change_id.into());
        self
    }
}

/// Graph of coordinated changes across repositories for one objective.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossRepoChangeGraph {
    /// Objective or epic identifier (e.g. `issue-27`).
    pub objective_id: String,
    /// Parent autonomous run id for provenance.
    pub run_id: String,
    /// Optional planning artifact reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    /// Changes keyed by id.
    pub changes: HashMap<String, RepoChange>,
}

impl CrossRepoChangeGraph {
    /// Create an empty change graph.
    pub fn new(
        objective_id: impl Into<String>,
        run_id: impl Into<String>,
    ) -> Self {
        Self {
            objective_id: objective_id.into(),
            run_id: run_id.into(),
            plan_id: None,
            changes: HashMap::new(),
        }
    }

    /// Attach a planning artifact id.
    pub fn with_plan_id(mut self, plan_id: impl Into<String>) -> Self {
        self.plan_id = Some(plan_id.into());
        self
    }

    /// Add a change to the graph.
    pub fn add_change(&mut self, change: RepoChange) {
        self.changes.insert(change.id.clone(), change);
    }

    /// Get a change by id.
    pub fn get_change(&self, id: &str) -> Option<&RepoChange> {
        self.changes.get(id)
    }

    /// Get a mutable change by id.
    pub fn get_change_mut(&mut self, id: &str) -> Option<&mut RepoChange> {
        self.changes.get_mut(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_graph_stores_dependencies() {
        let mut graph = CrossRepoChangeGraph::new("obj-1", "run-1");
        graph.add_change(RepoChange::new("infra", "org/infra", "Bump module"));
        graph.add_change(
            RepoChange::new("app", "org/app", "Consume new module").depends_on("infra"),
        );

        let app = graph.get_change("app").unwrap();
        assert_eq!(app.dependencies, vec!["infra"]);
    }
}
