//! Example: Autonomous AI agent development workflow (Issue #18 Phase 1)
//!
//! Demonstrates traced execution, tool policy, and quality gate routing.
//!
//! Run with: cargo run --example autonomous_dev_workflow

use async_trait::async_trait;
use oxidizedgraph::prelude::*;
use std::time::Duration;

/// Simulates an agent implementing a change.
struct ImplementNode;

#[async_trait]
impl NodeExecutor for ImplementNode {
    fn id(&self) -> &str {
        "implement"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;
        guard.set_context("change_summary", "Added autonomous orchestration modules");
        guard.set_context(
            "change_risk",
            ChangeRisk {
                files_changed: 8,
                used_shell: false,
                ..Default::default()
            },
        );
        Ok(NodeOutput::cont())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let gate_config = QualityGateConfig {
        checks: vec![CommandSpec {
            name: "echo_check".to_string(),
            program: "echo".to_string(),
            args: vec!["gates_ok".to_string()],
        }],
        block_on_failure: true,
    };

    let graph = GraphBuilder::new()
        .name("autonomous_dev")
        .add_node(ImplementNode)
        .add_node(QualityGateNode::new("quality_gate", gate_config))
        .add_node(StaticTransitionNode::new("ship", "done"))
        .add_node(StaticTransitionNode::new("human_review", "done"))
        .add_node(EchoNode::new("review", "Queued for optional human review"))
        .add_node(StaticTransitionNode::new("fix_loop", "implement"))
        .add_node(EchoNode::new(
            "done",
            "Workflow complete — ready to open PR",
        ))
        .set_entry_point("implement")
        .add_edge("implement", "quality_gate")
        .add_edge_with_key("quality_gate", "ship", "passed")
        .add_edge_with_key("quality_gate", "human_review", "needs_approval")
        .add_edge_with_key("quality_gate", "review", "review")
        .add_edge_with_key("quality_gate", "fix_loop", "gate_failed")
        .add_edge("review", "done")
        .add_edge_to_end("done")
        .add_edge("fix_loop", "implement")
        .compile()?;

    let run_ctx = RunContext::with_ids("demo-run", "demo-thread").tag("issue-18");
    let traced = TracedRunner::with_context(graph, run_ctx, RunnerConfig::default().verbose(true));

    let result = traced.invoke(AgentState::new()).await?;

    println!("Run ID: {}", result.run_context.run_id);
    println!("Transitions: {}", result.transition_log.len());
    for record in result.transition_log.records() {
        println!(
            "  [{}] {} -> {:?} ({})",
            record.iteration, record.node_id, record.next_node, record.output_kind
        );
    }

    if let Some(summary) = result.state.get_context::<String>("change_summary") {
        println!("Change: {summary}");
    }
    if let Some(gate) = result.state.get_context::<GateResult>("gate_result") {
        println!(
            "Gate passed: {} (checks: {})",
            gate.passed,
            gate.checks.len()
        );
    }
    if let Some(blocker) = result.state.get_context::<MergeBlocker>("merge_blocker") {
        if blocker.blocked {
            println!("Merge blocked: {}", blocker.reason);
        }
    }

    // Tool policy demo
    let policy = ToolPolicyEngine::new(
        ToolExecutionPolicy::permissive().allow_tool("echo", vec![Capability::Shell]),
    );
    let tool = FunctionTool::new("echo", "Echo", serde_json::json!({}), |args| {
        Ok(args["message"].as_str().unwrap_or("").to_string())
    });
    let registry = ToolRegistry::new().register(tool);
    let tool_node = ToolNode::with_config(
        "tools",
        registry,
        ToolNodeConfig::with_timeout(Duration::from_secs(5)).with_policy(policy),
    );

    let mut tool_state = AgentState::new();
    tool_state.tool_calls.push(ToolCall::new(
        "tc1",
        "echo",
        serde_json::json!({"message": "policy ok"}),
    ));
    let shared = std::sync::Arc::new(std::sync::RwLock::new(tool_state));
    tool_node.execute(shared).await?;

    Ok(())
}
