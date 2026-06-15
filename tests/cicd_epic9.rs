//! Integration tests for EPIC9 multi-repo CI/CD (#27).

use oxidizedgraph::prelude::*;
use std::sync::{Arc, RwLock};

fn sample_graph() -> CrossRepoChangeGraph {
    let mut graph = CrossRepoChangeGraph::new("issue-27", "run-1");
    graph.add_change(RepoChange::new("infra", "org/infra", "Publish module"));
    graph.add_change(RepoChange::new("app", "org/app", "Deploy service").depends_on("infra"));
    graph
}

#[test]
fn epic9_coordinator_orders_cross_repo_changes() {
    let coordinator = MultiRepoCoordinator::new();
    let graph = sample_graph();
    assert_eq!(coordinator.ready_changes(&graph), vec!["infra"]);
}

#[test]
fn epic9_ci_aggregate_consolidates_per_objective() {
    let signals = vec![
        CiCheckSignal::new("org/infra", "validate", "test", CiConclusion::Success),
        CiCheckSignal::new("org/app", "validate", "test", CiConclusion::Success),
    ];
    let report = CiAggregator::aggregate("issue-27", &signals);
    assert!(report.passed);
    assert!(report.failing_repos.is_empty());
}

#[test]
fn epic9_release_blocked_on_downstream_breakage() {
    let coordinator = MultiRepoCoordinator::new();
    let mut graph = sample_graph();
    coordinator.mark_status(&mut graph, "infra", RepoChangeStatus::CiFailed);

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

#[tokio::test]
async fn epic9_graph_walks_infra_then_app() {
    let coordinator = MultiRepoCoordinatorNode::new("coordinate");
    let complete = CompleteRepoChangeNode::new("complete");

    let mut state = AgentState::new();
    state.set_context(CTX_CHANGE_GRAPH, sample_graph());
    let shared = Arc::new(RwLock::new(state));

    let first = coordinator.execute(shared.clone()).await.unwrap();
    assert_eq!(first.target(), Some("execute_change"));
    assert_eq!(
        shared
            .read()
            .unwrap()
            .get_context::<String>(CTX_CURRENT_REPO_CHANGE),
        Some("infra".to_string())
    );

    complete.execute(shared.clone()).await.unwrap();

    let second = coordinator.execute(shared.clone()).await.unwrap();
    assert_eq!(second.target(), Some("execute_change"));
    assert_eq!(
        shared
            .read()
            .unwrap()
            .get_context::<String>(CTX_CURRENT_REPO_CHANGE),
        Some("app".to_string())
    );
}
