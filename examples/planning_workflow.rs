//! Example: Planning and long-horizon autonomy (Issue #24 / EPIC6)
//!
//! Demonstrates goal decomposition, dependency scheduling, and progress tracking.
//!
//! Run with: cargo run --example planning_workflow

use async_trait::async_trait;
use oxidizedgraph::prelude::*;

/// Simulates executing the task selected by SchedulerNode.
struct ExecuteTaskNode;

#[async_trait]
impl NodeExecutor for ExecuteTaskNode {
    fn id(&self) -> &str {
        "execute_task"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let task_id: String = guard
            .get_context("current_task_id")
            .ok_or_else(|| NodeError::execution_failed("missing current_task_id".to_string()))?;

        let mut plan: EpicPlan = guard
            .get_context("epic_plan")
            .ok_or_else(|| NodeError::execution_failed("missing epic_plan".to_string()))?;

        plan.update_task_status(&task_id, TaskStatus::Completed);
        let progress = PlanProgress::calculate(&plan);
        guard.set_context("epic_plan", plan);
        guard.set_context("plan_progress", progress);

        Ok(NodeOutput::cont())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let planner = PlanningNode::with_decomposer("planner", |goal| {
        vec![
            Task::new("design_api", "Design API", format!("Design for {goal}")).with_estimate(30),
            Task::new("implement_core", "Implement", "Core implementation")
                .depends_on("design_api")
                .with_estimate(120),
            Task::new("verify", "Verify", "Run verification suite")
                .depends_on("implement_core")
                .with_estimate(45),
        ]
    });

    let scheduler = SchedulerNode::new("scheduler");

    let graph = GraphBuilder::new()
        .name("planning_workflow")
        .add_node(planner)
        .add_node(scheduler)
        .add_node(ExecuteTaskNode)
        .add_node(EchoNode::new("done", "Plan complete"))
        .set_entry_point("planner")
        .add_edge("planner", "scheduler")
        .add_edge_with_key("scheduler", "execute_task", "execute_task")
        .add_edge_with_key("scheduler", "done", "complete")
        .add_edge("execute_task", "scheduler")
        .add_edge_to_end("done")
        .compile()?;

    let run_ctx = RunContext::with_ids("plan-run", "plan-thread").tag("issue-24-epic6");
    let traced = TracedRunner::with_context(graph, run_ctx, RunnerConfig::default().verbose(true));

    let mut state = AgentState::new();
    state.set_context("goal", "ship_planning_module");

    let result = traced.invoke(state).await?;

    if let Some(progress) = result.state.get_context::<PlanProgress>("plan_progress") {
        println!(
            "Progress: {:.0}% ({}/{}) confidence={:.2}",
            progress.percent_complete,
            progress.completed_tasks,
            progress.total_tasks,
            progress.confidence_score
        );
    }

    if let Some(plan) = result.state.get_context::<EpicPlan>("epic_plan") {
        for task in plan.tasks.values() {
            println!("  {} — {:?}", task.id, task.status);
        }
    }

    Ok(())
}
