//! Graph nodes for multi-repo CI/CD orchestration.

use async_trait::async_trait;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::SharedState;

use super::change_graph::{CrossRepoChangeGraph, RepoChangeStatus};
use super::ci_aggregate::{CiAggregateReport, CiAggregator, CiCheckSignal};
use super::coordinator::MultiRepoCoordinator;
use super::release::ReleaseOrchestrator;

/// Context key for the cross-repo change graph.
pub const CTX_CHANGE_GRAPH: &str = "change_graph";
/// Context key for consolidated CI report.
pub const CTX_CI_AGGREGATE: &str = "ci_aggregate";
/// Context key for release gate evaluation.
pub const CTX_RELEASE_GATE: &str = "release_gate";
/// Context key for the currently executing repo change id.
pub const CTX_CURRENT_REPO_CHANGE: &str = "current_repo_change";

/// Schedules the next ready repository change in dependency order.
#[derive(Clone, Debug, Default)]
pub struct MultiRepoCoordinatorNode {
    id: String,
    coordinator: MultiRepoCoordinator,
}

impl MultiRepoCoordinatorNode {
    /// Create a coordinator node.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            coordinator: MultiRepoCoordinator::new(),
        }
    }
}

#[async_trait]
impl NodeExecutor for MultiRepoCoordinatorNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let mut graph: CrossRepoChangeGraph = guard
            .get_context(CTX_CHANGE_GRAPH)
            .ok_or_else(|| NodeError::execution_failed("missing change_graph".to_string()))?;

        if self.coordinator.has_cycles(&graph) {
            return Err(NodeError::execution_failed(
                "change graph contains dependency cycles".to_string(),
            ));
        }

        if self.coordinator.all_released(&graph) {
            return Ok(NodeOutput::transition("complete"));
        }

        if self.coordinator.has_rollout_failure(&graph) {
            return Ok(NodeOutput::transition("blocked"));
        }

        let ready = self.coordinator.ready_changes(&graph);
        if ready.is_empty() {
            return Ok(NodeOutput::cont());
        }

        let next_id = ready[0].clone();
        self.coordinator
            .mark_status(&mut graph, &next_id, RepoChangeStatus::InProgress);
        guard.set_context(CTX_CHANGE_GRAPH, graph);
        guard.set_context(CTX_CURRENT_REPO_CHANGE, next_id);

        Ok(NodeOutput::transition("execute_change"))
    }

    fn description(&self) -> Option<&str> {
        Some("Schedules cross-repo changes in dependency order")
    }
}

/// Aggregates CI signals stored in context into an objective-level report.
#[derive(Clone, Debug, Default)]
pub struct CiAggregateNode {
    id: String,
    aggregator: CiAggregator,
}

impl CiAggregateNode {
    /// Create a CI aggregate node.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            aggregator: CiAggregator::new(),
        }
    }
}

#[async_trait]
impl NodeExecutor for CiAggregateNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let graph: CrossRepoChangeGraph = guard
            .get_context(CTX_CHANGE_GRAPH)
            .ok_or_else(|| NodeError::execution_failed("missing change_graph".to_string()))?;

        let signals: Vec<CiCheckSignal> = guard.get_context("ci_signals").unwrap_or_default();
        let report = CiAggregator::aggregate(&graph.objective_id, &signals);
        guard.set_context(CTX_CI_AGGREGATE, report.clone());

        if self.aggregator.blocks_release(&report) {
            Ok(NodeOutput::transition("ci_failed"))
        } else {
            Ok(NodeOutput::transition("ci_passed"))
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Aggregates per-repo CI signals into an objective-level report")
    }
}

/// Evaluates release readiness from change graph + CI aggregate.
#[derive(Clone, Debug, Default)]
pub struct ReleaseGateNode {
    id: String,
}

impl ReleaseGateNode {
    /// Create a release gate node.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl NodeExecutor for ReleaseGateNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let graph: CrossRepoChangeGraph = guard
            .get_context(CTX_CHANGE_GRAPH)
            .ok_or_else(|| NodeError::execution_failed("missing change_graph".to_string()))?;
        let ci_report: CiAggregateReport = guard
            .get_context(CTX_CI_AGGREGATE)
            .ok_or_else(|| NodeError::execution_failed("missing ci_aggregate".to_string()))?;

        let gate = ReleaseOrchestrator::evaluate(&graph, &ci_report);
        let transition = if gate.can_release {
            "release_approved"
        } else {
            "release_blocked"
        };
        guard.set_context(CTX_RELEASE_GATE, gate);

        Ok(NodeOutput::transition(transition))
    }

    fn description(&self) -> Option<&str> {
        Some("Gates coordinated release on CI aggregate and change graph health")
    }
}

/// Marks the current repo change as CI-passed after simulated work.
#[derive(Clone, Debug)]
pub struct CompleteRepoChangeNode {
    id: String,
}

impl CompleteRepoChangeNode {
    /// Create a node that marks the active change CI-passed.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl NodeExecutor for CompleteRepoChangeNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let change_id: String = guard.get_context(CTX_CURRENT_REPO_CHANGE).ok_or_else(|| {
            NodeError::execution_failed("missing current_repo_change".to_string())
        })?;

        let mut graph: CrossRepoChangeGraph = guard
            .get_context(CTX_CHANGE_GRAPH)
            .ok_or_else(|| NodeError::execution_failed("missing change_graph".to_string()))?;

        MultiRepoCoordinator::new().mark_status(&mut graph, &change_id, RepoChangeStatus::CiPassed);
        guard.set_context(CTX_CHANGE_GRAPH, graph);
        guard.remove_context(CTX_CURRENT_REPO_CHANGE);

        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Marks the active repo change as CI-passed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cicd::change_graph::RepoChange;
    use crate::cicd::ci_aggregate::{CiCheckSignal, CiConclusion};
    use crate::state::AgentState;
    use std::sync::{Arc, RwLock};

    #[tokio::test]
    async fn coordinator_node_schedules_first_change() {
        let mut graph = CrossRepoChangeGraph::new("issue-27", "run-1");
        graph.add_change(RepoChange::new("infra", "org/infra", "mod"));
        let mut state = AgentState::new();
        state.set_context(CTX_CHANGE_GRAPH, graph);
        let shared = Arc::new(RwLock::new(state));

        let output = MultiRepoCoordinatorNode::new("coord")
            .execute(shared.clone())
            .await
            .unwrap();
        assert_eq!(output.target(), Some("execute_change"));

        let guard = shared.read().unwrap();
        assert_eq!(
            guard.get_context::<String>(CTX_CURRENT_REPO_CHANGE),
            Some("infra".to_string())
        );
    }

    #[tokio::test]
    async fn release_gate_blocks_on_ci_failure() {
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

        let mut state = AgentState::new();
        state.set_context(CTX_CHANGE_GRAPH, graph);
        state.set_context(CTX_CI_AGGREGATE, ci);
        let shared = Arc::new(RwLock::new(state));

        let output = ReleaseGateNode::new("gate").execute(shared).await.unwrap();
        assert_eq!(output.target(), Some("release_blocked"));
    }
}
