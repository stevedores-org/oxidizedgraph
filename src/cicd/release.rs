//! Release batching and rollout gating.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::change_graph::{CrossRepoChangeGraph, RepoChangeStatus};
use super::ci_aggregate::CiAggregateReport;

/// A batch of repo changes staged for coordinated release.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReleaseBatch {
    /// Unique batch identifier.
    pub batch_id: String,
    /// Parent objective.
    pub objective_id: String,
    /// Change ids included in this batch.
    pub change_ids: Vec<String>,
    /// Correlated run id for provenance.
    pub run_id: String,
}

impl ReleaseBatch {
    /// Create a release batch.
    pub fn new(
        objective_id: impl Into<String>,
        run_id: impl Into<String>,
        change_ids: Vec<String>,
    ) -> Self {
        Self {
            batch_id: Uuid::new_v4().to_string(),
            objective_id: objective_id.into(),
            change_ids,
            run_id: run_id.into(),
        }
    }
}

/// Result of evaluating whether a rollout may proceed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseGateResult {
    /// Whether release is allowed.
    pub can_release: bool,
    /// Human-readable reason when blocked.
    pub reason: String,
    /// Batch id when release is approved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
}

impl ReleaseGateResult {
    /// Blocked release.
    pub fn blocked(reason: impl Into<String>) -> Self {
        Self {
            can_release: false,
            reason: reason.into(),
            batch_id: None,
        }
    }

    /// Approved release.
    pub fn approved(batch_id: impl Into<String>) -> Self {
        Self {
            can_release: true,
            reason: String::new(),
            batch_id: Some(batch_id.into()),
        }
    }
}

/// Evaluates cross-repo change state and CI aggregates for release.
#[derive(Clone, Debug, Default)]
pub struct ReleaseOrchestrator;

impl ReleaseOrchestrator {
    /// Create an orchestrator.
    pub fn new() -> Self {
        Self
    }

    /// Evaluate whether a coordinated rollout may proceed.
    pub fn evaluate(
        graph: &CrossRepoChangeGraph,
        ci_report: &CiAggregateReport,
    ) -> ReleaseGateResult {
        if graph.objective_id != ci_report.objective_id {
            return ReleaseGateResult::blocked(format!(
                "objective mismatch: graph={} ci={}",
                graph.objective_id, ci_report.objective_id
            ));
        }

        if !ci_report.passed {
            if !ci_report.failing_repos.is_empty() {
                return ReleaseGateResult::blocked(format!(
                    "CI failed for: {}",
                    ci_report.failing_repos.join(", ")
                ));
            }
            return ReleaseGateResult::blocked(format!(
                "CI pending for: {}",
                ci_report.pending_repos.join(", ")
            ));
        }

        let blocked = graph.changes.values().any(|c| {
            matches!(
                c.status,
                RepoChangeStatus::CiFailed | RepoChangeStatus::Blocked
            )
        });
        if blocked {
            return ReleaseGateResult::blocked(
                "downstream repo changes blocked by upstream failure",
            );
        }

        let not_ready = graph.changes.values().any(|c| {
            !matches!(
                c.status,
                RepoChangeStatus::CiPassed | RepoChangeStatus::Released
            )
        });
        if not_ready {
            return ReleaseGateResult::blocked("not all repo changes have passed CI");
        }

        let batch = Self::create_batch(graph, &ready_change_ids(graph));
        ReleaseGateResult::approved(batch.batch_id)
    }

    /// Create a release batch from CI-passed changes.
    pub fn create_batch(graph: &CrossRepoChangeGraph, change_ids: &[String]) -> ReleaseBatch {
        ReleaseBatch::new(&graph.objective_id, &graph.run_id, change_ids.to_vec())
    }
}

fn ready_change_ids(graph: &CrossRepoChangeGraph) -> Vec<String> {
    let mut ids: Vec<_> = graph
        .changes
        .values()
        .filter(|c| c.status == RepoChangeStatus::CiPassed)
        .map(|c| c.id.clone())
        .collect();
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cicd::change_graph::RepoChange;
    use crate::cicd::ci_aggregate::{CiAggregator, CiCheckSignal, CiConclusion};

    #[test]
    fn blocks_release_on_ci_failure() {
        let mut graph = CrossRepoChangeGraph::new("issue-27", "run-1");
        graph.add_change(RepoChange::new("infra", "org/infra", "mod"));
        graph.get_change_mut("infra").unwrap().status = RepoChangeStatus::CiPassed;

        let ci = CiAggregator::aggregate(
            "issue-27",
            &[CiCheckSignal::new(
                "org/infra",
                "validate",
                "test",
                CiConclusion::Failure,
            )],
        );

        let gate = ReleaseOrchestrator::evaluate(&graph, &ci);
        assert!(!gate.can_release);
    }

    #[test]
    fn approves_when_ci_and_graph_ready() {
        let mut graph = CrossRepoChangeGraph::new("issue-27", "run-1");
        graph.add_change(RepoChange::new("infra", "org/infra", "mod"));
        graph.get_change_mut("infra").unwrap().status = RepoChangeStatus::CiPassed;

        let ci = CiAggregator::aggregate(
            "issue-27",
            &[CiCheckSignal::new(
                "org/infra",
                "validate",
                "test",
                CiConclusion::Success,
            )],
        );

        let gate = ReleaseOrchestrator::evaluate(&graph, &ci);
        assert!(gate.can_release);
        assert!(gate.batch_id.is_some());
    }
}
