//! Example: Human-in-the-loop approval workflow (Issue #26 / EPIC8)
//!
//! Run with: cargo run --example hitl_workflow

use oxidizedgraph::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let graph = GraphBuilder::new()
        .name("hitl_workflow")
        .add_node(ApprovalCheckpointNode::new("checkpoint"))
        .add_node(GrantApprovalNode::new("grant", "operator@example.com").with_rationale(
            "Reviewed risk summary — proceed with deploy",
        ))
        .add_node(EchoNode::new("ship", "Change shipped after approval"))
        .add_node(EchoNode::new("wait", "Paused for operator review"))
        .set_entry_point("checkpoint")
        .add_edge_with_key("checkpoint", "wait", "awaiting_approval")
        .add_edge_with_key("checkpoint", "ship", "approved")
        .add_edge("wait", "grant")
        .add_edge("grant", "checkpoint")
        .add_edge_to_end("ship")
        .compile()?;

    let run_ctx = RunContext::with_ids("hitl-run", "hitl-thread").tag("issue-26-epic8");
    let traced = TracedRunner::with_context(graph, run_ctx, RunnerConfig::default().verbose(true));

    let mut state = AgentState::new();
    state.set_context("change_risk_level", RiskLevel::High);
    state.set_context("change_summary", "Deploy oxidizedgraph server to GKE");

    let result = traced.invoke(state).await?;

    if let Some(explanation) = result.state.get_context::<ExplanationPayload>(CTX_EXPLANATION) {
        println!("Summary: {}", explanation.summary);
        println!("Rationale: {}", explanation.rationale);
    }

    if let Some(events) = result.state.get_context::<Vec<ApprovalEvent>>(CTX_APPROVAL_EVENTS) {
        println!("Approval events: {}", events.len());
        for event in events {
            println!("  [{}] {}", event.timestamp, event.kind);
        }
    }

    let timeline = RunTimeline::from_artifacts(
        result.transition_log.records(),
        &result
            .state
            .get_context::<Vec<ApprovalEvent>>(CTX_APPROVAL_EVENTS)
            .unwrap_or_default(),
    );
    println!("Timeline entries: {}", timeline.entries.len());

    Ok(())
}
