//! Example: Multi-repo CI/CD orchestration (Issue #27 / EPIC9)
//!
//! Run with: cargo run --example multirepo_cicd_workflow

use oxidizedgraph::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let graph = GraphBuilder::new()
        .name("multirepo_cicd")
        .add_node(MultiRepoCoordinatorNode::new("coordinate"))
        .add_node(CompleteRepoChangeNode::new("complete_change"))
        .add_node(CiAggregateNode::new("aggregate_ci"))
        .add_node(ReleaseGateNode::new("release_gate"))
        .add_node(EchoNode::new("released", "Coordinated rollout approved"))
        .add_node(EchoNode::new("blocked", "Rollout blocked"))
        .set_entry_point("coordinate")
        .add_edge_with_key("coordinate", "complete_change", "execute_change")
        .add_edge_with_key("coordinate", "released", "complete")
        .add_edge_with_key("coordinate", "blocked", "blocked")
        .add_edge("complete_change", "coordinate")
        .add_edge("coordinate", "aggregate_ci")
        .add_edge_with_key("aggregate_ci", "release_gate", "ci_passed")
        .add_edge_with_key("aggregate_ci", "blocked", "ci_failed")
        .add_edge_with_key("release_gate", "released", "release_approved")
        .add_edge_with_key("release_gate", "blocked", "release_blocked")
        .add_edge_to_end("released")
        .add_edge_to_end("blocked")
        .compile()?;

    let mut change_graph = CrossRepoChangeGraph::new("issue-27", "run-epic9")
        .with_plan_id("plan-cross-repo");
    change_graph.add_change(RepoChange::new(
        "infra",
        "stevedores-org/infra-code",
        "Publish shared Terraform module",
    ));
    change_graph.add_change(
        RepoChange::new(
            "graph",
            "stevedores-org/oxidizedgraph",
            "Consume updated deploy overlay",
        )
        .depends_on("infra"),
    );

    let ci_signals = vec![
        CiCheckSignal::new(
            "stevedores-org/infra-code",
            "validate",
            "kustomize build",
            CiConclusion::Success,
        ),
        CiCheckSignal::new(
            "stevedores-org/oxidizedgraph",
            "validate",
            "cargo test",
            CiConclusion::Success,
        ),
    ];

    let run_ctx = RunContext::with_ids("epic9-run", "epic9-thread").tag("issue-27");
    let traced = TracedRunner::with_context(graph, run_ctx, RunnerConfig::default().verbose(true));

    let mut state = AgentState::new();
    state.set_context(CTX_CHANGE_GRAPH, change_graph);
    state.set_context("ci_signals", ci_signals);

    let result = traced.invoke(state).await?;

    if let Some(ci) = result.state.get_context::<CiAggregateReport>(CTX_CI_AGGREGATE) {
        println!(
            "CI aggregate: passed={} failing={:?} pending={:?}",
            ci.passed, ci.failing_repos, ci.pending_repos
        );
    }
    if let Some(gate) = result.state.get_context::<ReleaseGateResult>(CTX_RELEASE_GATE) {
        println!("Release gate: can_release={} reason={}", gate.can_release, gate.reason);
    }

    Ok(())
}
