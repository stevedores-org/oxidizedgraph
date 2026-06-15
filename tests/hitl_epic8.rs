//! Integration tests for EPIC8 human-in-the-loop (#26).

use oxidizedgraph::prelude::*;
use oxidizedgraph::hitl::{CTX_APPROVAL_EVENTS, CTX_HITL_PAUSED};
use std::sync::{Arc, RwLock};

#[tokio::test]
async fn epic8_checkpoint_pauses_high_risk_change() {
    let checkpoint = ApprovalCheckpointNode::new("checkpoint");
    let mut state = AgentState::new();
    state.set_context("change_risk_level", RiskLevel::High);
    state.set_context("change_summary", "Delete legacy auth module");
    let shared = Arc::new(RwLock::new(state));

    let output = checkpoint.execute(shared.clone()).await.unwrap();
    assert_eq!(output.target(), Some("awaiting_approval"));

    let guard = shared.read().unwrap();
    assert!(guard.get_context::<bool>(CTX_HITL_PAUSED).unwrap());
    let events = guard.get_context::<Vec<ApprovalEvent>>(CTX_APPROVAL_EVENTS).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "checkpoint_created");
}

#[tokio::test]
async fn epic8_grant_approval_resumes_workflow() {
    let checkpoint = ApprovalCheckpointNode::new("checkpoint");
    let grant = GrantApprovalNode::new("grant", "security-reviewer");
    let mut state = AgentState::new();
    state.set_context("change_risk_level", RiskLevel::High);
    let shared = Arc::new(RwLock::new(state));

    checkpoint.execute(shared.clone()).await.unwrap();
    grant.execute(shared.clone()).await.unwrap();

    // Re-run checkpoint — should route approved now that decision is recorded.
    let output = checkpoint.execute(shared.clone()).await.unwrap();
    assert_eq!(output.target(), Some("approved"));
}

#[tokio::test]
async fn epic8_low_risk_skips_pause() {
    let checkpoint = ApprovalCheckpointNode::new("checkpoint");
    let mut state = AgentState::new();
    state.set_context("change_risk_level", RiskLevel::Low);
    let shared = Arc::new(RwLock::new(state));

    let output = checkpoint.execute(shared).await.unwrap();
    assert_eq!(output.target(), Some("approved"));
}

#[test]
fn epic8_timeline_merges_transitions_and_approvals() {
    use oxidizedgraph::execution::TransitionRecord;
    use chrono::Utc;

    let transitions = vec![TransitionRecord {
        run_id: "r1".into(),
        iteration: 1,
        node_id: "gate".into(),
        output_kind: "transition".into(),
        next_node: Some("checkpoint".into()),
        output_target: Some("needs_approval".into()),
        state_iteration: 1,
        recorded_at: Utc::now(),
    }];
    let approvals = vec![ApprovalEvent {
        id: "e1".into(),
        timestamp: Utc::now(),
        kind: "checkpoint_created".into(),
        detail: serde_json::json!({}),
    }];

    let timeline = RunTimeline::from_artifacts(&transitions, &approvals);
    assert_eq!(timeline.entries.len(), 2);
    assert_eq!(timeline.approval_count(), 1);
}

#[tokio::test]
async fn epic8_graph_hitl_loop() {
    let _graph = GraphBuilder::new()
        .add_node(ApprovalCheckpointNode::new("checkpoint"))
        .add_node(GrantApprovalNode::new("grant", "operator"))
        .add_node(EchoNode::new("ship", "Shipped after approval"))
        .add_node(EchoNode::new("wait", "Awaiting operator"))
        .set_entry_point("checkpoint")
        .add_edge_with_key("checkpoint", "wait", "awaiting_approval")
        .add_edge_with_key("checkpoint", "ship", "approved")
        .add_edge("wait", "grant")
        .add_edge("grant", "checkpoint")
        .add_edge_to_end("ship")
        .compile()
        .unwrap();

    let mut state = AgentState::new();
    state.set_context("change_risk_level", RiskLevel::High);

    // First pass pauses at wait — simulate partial run by invoking checkpoint only.
    let checkpoint = ApprovalCheckpointNode::new("checkpoint");
    let shared = Arc::new(RwLock::new(state));
    let first = checkpoint.execute(shared.clone()).await.unwrap();
    assert_eq!(first.target(), Some("awaiting_approval"));

    // Operator grants approval and checkpoint routes through.
    GrantApprovalNode::new("grant", "operator")
        .execute(shared.clone())
        .await
        .unwrap();
    let second = checkpoint.execute(shared.clone()).await.unwrap();
    assert_eq!(second.target(), Some("approved"));
}
