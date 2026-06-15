//! Example: Human-in-the-loop approval workflow (Issue #26 / EPIC8)
//!
//! Demonstrates intervention API, diff summaries, edit+resume without state loss.
//!
//! Run with: cargo run --example hitl_workflow

use oxidizedgraph::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let graph = GraphBuilder::new()
        .name("hitl_workflow")
        .add_node(ApprovalCheckpointNode::new("checkpoint"))
        .add_node(EditInterventionNode::new("apply_edits"))
        .add_node(
            GrantApprovalNode::new("grant", "operator@example.com")
                .with_rationale("Reviewed risk summary — proceed with deploy"),
        )
        .add_node(EchoNode::new("ship", "Change shipped after approval"))
        .add_node(EchoNode::new("wait", "Paused for operator review"))
        .set_entry_point("checkpoint")
        .add_edge_with_key("checkpoint", "wait", "awaiting_approval")
        .add_edge_with_key("checkpoint", "ship", "approved")
        .add_edge("wait", "apply_edits")
        .add_edge("apply_edits", "grant")
        .add_edge("grant", "checkpoint")
        .add_edge_to_end("ship")
        .compile()?;

    let run_ctx = RunContext::with_ids("hitl-run", "hitl-thread").tag("issue-26-epic8");
    let traced = TracedRunner::with_context(graph, run_ctx, RunnerConfig::default().verbose(true));

    let mut state = AgentState::new();
    state.set_context("change_risk_level", RiskLevel::High);
    state.set_context("change_summary", "Deploy oxidizedgraph server to GKE");
    state.set_context(
        CTX_PROPOSED_DIFF,
        "+ deploy/overlays/gke-autopilot/replicas: 2\n+ oxidizedgraph image pin",
    );
    state.set_context(
        CTX_RECENT_TOOL_ACTIONS,
        vec![
            "kubectl diff -k deploy/overlays/gke-autopilot".to_string(),
            "cargo test --all-targets".to_string(),
        ],
    );

    // Simulate operator intervention before graph run: queue edit, then approve via API.
    let controller = HitlController::new();
    let shared = std::sync::Arc::new(std::sync::RwLock::new(state));
    {
        let mut guard = shared.write().unwrap();
        controller.pause(&mut guard, "High-risk deploy", RiskLevel::High);
        controller
            .queue_edits(
                &mut guard,
                InterventionEdit {
                    context_patches: [(
                        "change_summary".to_string(),
                        serde_json::json!(
                            "Deploy oxidizedgraph server to GKE (2 replicas, pinned image)"
                        ),
                    )]
                    .into(),
                },
            )
            .map_err(|e| anyhow::anyhow!(e))?;
        controller
            .approve(
                &mut guard,
                "operator@example.com",
                Some("Reviewed diff and rollout plan".to_string()),
            )
            .map_err(|e| anyhow::anyhow!(e))?;
    }

    let state = shared.read().unwrap().clone();
    let result = traced.invoke(state).await?;

    if let Some(explanation) = result
        .state
        .get_context::<ExplanationPayload>(CTX_EXPLANATION)
    {
        println!("Summary: {}", explanation.summary);
        println!("Rationale: {}", explanation.rationale);
        if let Some(diff) = &explanation.diff_hint {
            println!("Diff hint:\n{diff}");
        }
    }

    if let Some(summary) = result.state.get_context::<String>("change_summary") {
        println!("Final summary after edit: {summary}");
    }

    if let Some(events) = result
        .state
        .get_context::<Vec<ApprovalEvent>>(CTX_APPROVAL_EVENTS)
    {
        println!("Approval events: {}", events.len());
        for event in &events {
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
