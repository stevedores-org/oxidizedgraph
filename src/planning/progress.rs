use crate::planning::plan::{EpicPlan, TaskStatus};
use serde::{Deserialize, Serialize};

/// Progress report for an EpicPlan.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanProgress {
    /// Percentage of tasks that have been Completed.
    pub percent_complete: f32,
    /// Total number of tasks in the plan.
    pub total_tasks: usize,
    /// Number of completed tasks.
    pub completed_tasks: usize,
    /// Number of currently running tasks.
    pub running_tasks: usize,
    /// Number of failed tasks.
    pub failed_tasks: usize,
    /// Number of blocked tasks.
    pub blocked_tasks: usize,
    /// Confidence score between 0.0 (low) and 1.0 (high) reflecting plan health.
    pub confidence_score: f32,
    /// Estimated completion timestamp.
    pub estimated_completion: Option<chrono::DateTime<chrono::Utc>>,
}

impl PlanProgress {
    /// Calculate the progress and estimates for the given EpicPlan.
    pub fn calculate(plan: &EpicPlan) -> Self {
        let total_tasks = plan.tasks.len();
        if total_tasks == 0 {
            return Self {
                percent_complete: 100.0,
                total_tasks: 0,
                completed_tasks: 0,
                running_tasks: 0,
                failed_tasks: 0,
                blocked_tasks: 0,
                confidence_score: 1.0,
                estimated_completion: None,
            };
        }

        let mut completed_tasks = 0;
        let mut running_tasks = 0;
        let mut failed_tasks = 0;
        let mut blocked_tasks = 0;
        let mut pending_tasks = 0;

        for task in plan.tasks.values() {
            match task.status {
                TaskStatus::Completed => completed_tasks += 1,
                TaskStatus::Running => running_tasks += 1,
                TaskStatus::Failed => failed_tasks += 1,
                TaskStatus::Blocked => blocked_tasks += 1,
                TaskStatus::Pending => pending_tasks += 1,
            }
        }

        let percent_complete = (completed_tasks as f32 / total_tasks as f32) * 100.0;

        // Calculate confidence score (1.0 default, penalty for failures/blocks)
        let mut confidence_score = 1.0;
        confidence_score -= (failed_tasks as f32 * 0.25).min(0.5);
        confidence_score -= (blocked_tasks as f32 * 0.15).min(0.3);
        confidence_score = confidence_score.max(0.0);

        // Estimate completion time
        let now = chrono::Utc::now();
        let mut total_actual_secs = 0;
        let mut completed_with_duration = 0;

        for task in plan.tasks.values() {
            if task.status == TaskStatus::Completed {
                if let (Some(start), Some(end)) = (task.started_at, task.completed_at) {
                    let diff = end.signed_duration_since(start).num_seconds();
                    if diff > 0 {
                        total_actual_secs += diff;
                        completed_with_duration += 1;
                    }
                }
            }
        }

        // Average duration of tasks in seconds
        let avg_duration_secs = if completed_with_duration > 0 {
            total_actual_secs as f64 / completed_with_duration as f64
        } else {
            // Fallback to task-defined estimates
            let mut total_est = 0;
            let mut est_count = 0;
            for task in plan.tasks.values() {
                if let Some(est) = task.estimated_duration {
                    total_est += est;
                    est_count += 1;
                }
            }
            if est_count > 0 {
                total_est as f64 / est_count as f64
            } else {
                60.0 // Default to 60 seconds per task fallback
            }
        };

        let remaining_tasks = running_tasks + pending_tasks + blocked_tasks;
        let remaining_secs = remaining_tasks as f64 * avg_duration_secs;

        let estimated_completion = if remaining_secs > 0.0 {
            Some(now + chrono::Duration::seconds(remaining_secs as i64))
        } else {
            None
        };

        Self {
            percent_complete,
            total_tasks,
            completed_tasks,
            running_tasks,
            failed_tasks,
            blocked_tasks,
            confidence_score,
            estimated_completion,
        }
    }
}
