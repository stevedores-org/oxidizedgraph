use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::planning::plan::{EpicPlan, Task, TaskStatus};
use crate::planning::progress::PlanProgress;
use crate::planning::scheduler::Scheduler;
use crate::planning::self_healing::{FailureClass, RetryPolicy, RecoveryRecord, classify_failure};
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
    self_healing_enabled: bool,
    retry_policies: HashMap<FailureClass, RetryPolicy>,
}

impl SchedulerNode {
    /// Create a new scheduler node.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            scheduler: Scheduler::new(),
            auto_replan_recoveries: HashMap::new(),
            self_healing_enabled: true,
            retry_policies: HashMap::new(),
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

    /// Enable or disable self-healing.
    pub fn with_self_healing(mut self, enabled: bool) -> Self {
        self.self_healing_enabled = enabled;
        self
    }

    /// Set a custom retry policy for a specific failure class.
    pub fn with_retry_policy(mut self, class: FailureClass, policy: RetryPolicy) -> Self {
        self.retry_policies.insert(class, policy);
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
            let mut attempts: HashMap<String, usize> = guard.get_context("task_attempts").unwrap_or_default();
            let mut history: Vec<RecoveryRecord> = guard.get_context("recovery_history").unwrap_or_default();
            let mut replanned_all = true;
            let mut replanned_any = false;

            for failed_id in failed_ids {
                let task = plan.get_task(&failed_id).unwrap().clone();
                let error_msg = task.error.clone().unwrap_or_else(|| "Unknown error".to_string());

                if self.self_healing_enabled {
                    let class = classify_failure(&error_msg);
                    let policy = self.retry_policies.get(&class).cloned().unwrap_or_else(|| {
                        match class {
                            FailureClass::Compile => RetryPolicy::compile_default(),
                            FailureClass::Test => RetryPolicy::test_default(),
                            FailureClass::Runtime => RetryPolicy::runtime_default(),
                            FailureClass::Integration => RetryPolicy::integration_default(),
                            FailureClass::Unknown => RetryPolicy::default(),
                        }
                    });

                    let current_attempts = *attempts.get(&failed_id).unwrap_or(&0);

                    if current_attempts < policy.max_attempts {
                        let attempt = current_attempts + 1;
                        attempts.insert(failed_id.clone(), attempt);

                        // If a manual recovery is registered, use it, else generate dynamically
                        let recovery = if let Some(manual_rec) = self.auto_replan_recoveries.get(&failed_id).cloned() {
                            manual_rec
                        } else {
                            let rec_id = format!("recovery_{}_{}", failed_id, attempt);
                            let rec_name = format!("Remediate {} failure in {}", class.as_str(), task.name);
                            let rec_desc = format!(
                                "Self-healing task injected for {} (Attempt {}/{}) due to: {}",
                                task.id, attempt, policy.max_attempts, error_msg
                            );
                            Task::new(rec_id, rec_name, rec_desc)
                        };

                        let record = RecoveryRecord::new(
                            failed_id.clone(),
                            attempt,
                            class,
                            error_msg.clone(),
                            "Inject recovery task",
                            format!(
                                "Classified as {:?}. Injected recovery task {}/{} attempts.",
                                class, attempt, policy.max_attempts
                            )
                        );
                        history.push(record);

                        tracing::info!(
                            task_id = %failed_id,
                            attempt = attempt,
                            class = ?class,
                            "Recovery decision: Inject recovery task. Rationale: classified as {:?}, attempt {}/{}",
                            class,
                            attempt,
                            policy.max_attempts
                        );

                        plan.inject_recovery_task(&failed_id, recovery);
                        replanned_any = true;
                    } else {
                        let record = RecoveryRecord::new(
                            failed_id.clone(),
                            current_attempts,
                            class,
                            error_msg.clone(),
                            "Halted (max attempts)",
                            format!(
                                "Exceeded max attempts ({}) for failure class {:?}",
                                policy.max_attempts, class
                            )
                        );
                        history.push(record);

                        tracing::warn!(
                            task_id = %failed_id,
                            attempts = current_attempts,
                            class = ?class,
                            "Recovery decision: Halted (max attempts). Rationale: Exceeded max attempts ({}) for failure class {:?}",
                            policy.max_attempts,
                            class
                        );

                        replanned_all = false;
                    }
                } else {
                    // Backwards-compatible behavior
                    if let Some(recovery) = self.auto_replan_recoveries.get(&failed_id).cloned() {
                        plan.inject_recovery_task(&failed_id, recovery);
                        replanned_any = true;
                    } else {
                        replanned_all = false;
                    }
                }
            }

            guard.set_context("task_attempts", attempts);
            guard.set_context("recovery_history", history);

            if replanned_any && replanned_all {
                // Save plan and progress, then transition back to schedule the recovery task
                guard.set_context("epic_plan", plan.clone());
                let progress = PlanProgress::calculate(&plan);
                guard.set_context("plan_progress", progress);
                return Ok(NodeOutput::Transition("replan_injected".to_string()));
            } else {
                // Cannot recover or limit exceeded, route to failure or manual replan
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
