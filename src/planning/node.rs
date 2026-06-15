use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::planning::plan::{EpicPlan, Task, TaskStatus};
use crate::planning::progress::PlanProgress;
use crate::planning::scheduler::Scheduler;
use crate::state::SharedState;

/// Type alias for the goal decomposer closure.
pub type DecomposerFn = Arc<dyn Fn(&str) -> Vec<Task> + Send + Sync>;

/// Node that decomposes the agent's goal into an EpicPlan.
pub struct PlanningNode {
    id: String,
    decomposer: Option<DecomposerFn>,
}

impl PlanningNode {
    /// Create a new planning node.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            decomposer: None,
        }
    }

    /// Create a new planning node with a custom decomposer function.
    pub fn with_decomposer<F>(id: impl Into<String>, decomposer: F) -> Self
    where
        F: Fn(&str) -> Vec<Task> + Send + Sync + 'static,
    {
        Self {
            id: id.into(),
            decomposer: Some(Arc::new(decomposer)),
        }
    }
}

#[async_trait]
impl NodeExecutor for PlanningNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        // Retrieve the goal from context
        let goal: String = guard
            .get_context("goal")
            .unwrap_or_else(|| "default goal".to_string());

        let mut plan = EpicPlan::new(&goal);

        // Decompose the goal
        let tasks = if let Some(decomposer) = &self.decomposer {
            decomposer(&goal)
        } else {
            // Default mock decomposition
            vec![
                Task::new("task1", "Task 1", "First task in sequence").with_estimate(10),
                Task::new("task2", "Task 2", "Second task dependent on Task 1")
                    .depends_on("task1")
                    .with_estimate(20),
            ]
        };

        for task in tasks {
            plan.add_task(task);
        }

        // Store the plan and initialize progress
        guard.set_context("epic_plan", plan.clone());
        let progress = PlanProgress::calculate(&plan);
        guard.set_context("plan_progress", progress);

        Ok(NodeOutput::cont())
    }
}

/// Node that schedules and tracks execution of the EpicPlan.
pub struct SchedulerNode {
    id: String,
    scheduler: Scheduler,
    auto_replan_recoveries: HashMap<String, Task>,
}

impl SchedulerNode {
    /// Create a new scheduler node.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            scheduler: Scheduler::new(),
            auto_replan_recoveries: HashMap::new(),
        }
    }

    /// Register an automatic recovery task to inject when a specific task fails.
    pub fn register_recovery(
        mut self,
        failed_task_id: impl Into<String>,
        recovery_task: Task,
    ) -> Self {
        self.auto_replan_recoveries
            .insert(failed_task_id.into(), recovery_task);
        self
    }
}

#[async_trait]
impl NodeExecutor for SchedulerNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let mut plan: EpicPlan = guard.get_context("epic_plan").ok_or_else(|| {
            NodeError::execution_failed("No 'epic_plan' found in context".to_string())
        })?;

        // Cycle check
        if self.scheduler.has_cycles(&plan) {
            return Err(NodeError::execution_failed(
                "EpicPlan contains dependency cycles".to_string(),
            ));
        }

        // Check for any currently failed tasks to trigger auto-replan
        let mut has_failures = false;
        let mut failed_ids = Vec::new();
        for task in plan.tasks.values() {
            if task.status == TaskStatus::Failed {
                has_failures = true;
                failed_ids.push(task.id.clone());
            }
        }

        if has_failures {
            let mut replanned = false;
            for failed_id in failed_ids {
                if let Some(recovery) = self.auto_replan_recoveries.get(&failed_id).cloned() {
                    // Inject the recovery task
                    plan.inject_recovery_task(&failed_id, recovery);
                    replanned = true;
                }
            }

            if replanned {
                // Save plan and progress, then transition back to schedule the recovery task
                guard.set_context("epic_plan", plan.clone());
                let progress = PlanProgress::calculate(&plan);
                guard.set_context("plan_progress", progress);
                return Ok(NodeOutput::Transition("replan_injected".to_string()));
            } else {
                // Cannot recover, route to failure or manual replan
                return Ok(NodeOutput::Transition("replan_needed".to_string()));
            }
        }

        // Get ready tasks
        let ready = self.scheduler.next_tasks(&plan);
        if ready.is_empty() {
            // No ready tasks. Are they all completed?
            let all_done = plan
                .tasks
                .values()
                .all(|t| t.status == TaskStatus::Completed);
            if all_done {
                guard.set_context("is_complete", true);
                return Ok(NodeOutput::Transition("complete".to_string()));
            } else {
                // Remaining tasks are Blocked or Running.
                // If they are Blocked, and we have no failures to recover from, we are stuck.
                let any_blocked = plan.tasks.values().any(|t| t.status == TaskStatus::Blocked);
                if any_blocked {
                    return Ok(NodeOutput::Transition("blocked".to_string()));
                }
            }
            return Ok(NodeOutput::cont());
        }

        // Prioritize ready tasks
        let prioritized = self.scheduler.prioritize_tasks(&plan, &ready);
        let next_task_id = &prioritized[0];

        // Mark the selected task as Running
        plan.update_task_status(next_task_id, TaskStatus::Running);

        // Update plan and progress in state context
        guard.set_context("epic_plan", plan.clone());
        guard.set_context("current_task_id", next_task_id.clone());
        let progress = PlanProgress::calculate(&plan);
        guard.set_context("plan_progress", progress);

        // Transition to execute the task
        Ok(NodeOutput::Transition("execute_task".to_string()))
    }
}
