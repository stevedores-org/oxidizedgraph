//! Example: Role orchestration workflow (Issue #18 Phase 2 / EPIC2)
//!
//! Demonstrates GovernanceNode, role handoffs, role-based routing, and
//! per-role tool policy enforcement.
//!
//! Run with: cargo run --example role_orchestration_workflow

use async_trait::async_trait;
use oxidizedgraph::prelude::*;

const MANIFEST: &str = "\
<@all>
Follow repo conventions and keep diffs focused.

<@architect>
Produce a minimal design note before implementation begins.

<@builder>
Implement the change and run local checks before handoff.

<@auditor>
Verify tests pass and no policy regressions were introduced.
";

/// Simulates architect planning output.
struct PlanNode;

#[async_trait]
impl NodeExecutor for PlanNode {
    fn id(&self) -> &str {
        "plan"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;
        guard.set_context("design_note", "Add GovernanceNode + role routing primitives");
        Ok(NodeOutput::cont())
    }
}

/// Simulates builder implementation.
struct BuildNode;

#[async_trait]
impl NodeExecutor for BuildNode {
    fn id(&self) -> &str {
        "build"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;
        guard.set_context("change_summary", "EPIC2 governance nodes landed");
        Ok(NodeOutput::cont())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let graph = GraphBuilder::new()
        .name("role_orchestration")
        .add_node(GovernanceNode::new(
            "gov_architect",
            MANIFEST,
            AgentRole::Architect,
        ))
        .add_node(PlanNode)
        .add_node(RoleHandoffNode::new("handoff_builder", MANIFEST, AgentRole::Builder))
        .add_node(BuildNode)
        .add_node(RoleHandoffNode::new("handoff_auditor", MANIFEST, AgentRole::Auditor))
        .add_node(RoleRouterNode::new("role_route", "default"))
        .add_node(EchoNode::new("audit_lane", "Auditor lane: policy checks complete"))
        .add_node(EchoNode::new("fallback", "No role set — unexpected"))
        .add_node(EchoNode::new("done", "Role orchestration workflow complete"))
        .set_entry_point("gov_architect")
        .add_edge("gov_architect", "plan")
        .add_edge("plan", "handoff_builder")
        .add_edge("handoff_builder", "build")
        .add_edge("build", "handoff_auditor")
        .add_edge("handoff_auditor", "role_route")
        .add_edge_with_key("role_route", "audit_lane", "auditor")
        .add_edge_with_key("role_route", "fallback", "default")
        .add_edge("audit_lane", "done")
        .add_edge("fallback", "done")
        .add_edge_to_end("done")
        .compile()?;

    let run_ctx = RunContext::with_ids("role-run", "role-thread").tag("issue-18-epic2");
    let traced = TracedRunner::with_context(graph, run_ctx, RunnerConfig::default().verbose(true));
    let result = traced.invoke(AgentState::new()).await?;

    println!("Run ID: {}", result.run_context.run_id);
    println!("Final role: {:?}", agent_role_from_state(&result.state));
    if let Some(note) = result.state.get_context::<String>("design_note") {
        println!("Design: {note}");
    }
    if let Some(summary) = result.state.get_context::<String>("change_summary") {
        println!("Change: {summary}");
    }
    if let Some(guidance) = result.state.get_context::<String>(CTX_GOVERNANCE_GUIDANCE) {
        println!("Final guidance blocks:\n{guidance}");
    }

    // Per-role tool policy demo: architect cannot shell, builder can.
    for role in [AgentRole::Architect, AgentRole::Builder, AgentRole::Auditor] {
        let policy = tool_policy_for_role(&role);
        let engine = ToolPolicyEngine::new(policy);
        let decision = engine.evaluate("shell", None);
        println!("{role} shell policy: {decision:?}");
    }

    Ok(())
}
