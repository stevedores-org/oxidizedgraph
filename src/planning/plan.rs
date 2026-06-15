use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Status representing the execution state of a Task.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task is waiting for its dependencies to be completed.
    Pending,
    /// Task is currently executing.
    Running,
    /// Task has completed successfully.
    Completed,
    /// Task has failed during execution.
    Failed,
    /// Task cannot execute because one of its dependencies has failed or is blocked.
    Blocked,
}

/// A discrete unit of work within an EpicPlan.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Task {
    /// Unique identifier for the task.
    pub id: String,
    /// Human-readable name of the task.
    pub name: String,
    /// Detailed description of the task's goal.
    pub description: String,
    /// Current status of the task.
    pub status: TaskStatus,
    /// List of task IDs that must complete before this task can start.
    pub dependencies: Vec<String>,
    /// Inputs required for task execution.
    pub input: serde_json::Value,
    /// Optional output resulting from successful task execution.
    pub output: Option<serde_json::Value>,
    /// Error message if the task failed.
    pub error: Option<String>,
    /// Optional agent role assigned to execute this task.
    pub assigned_role: Option<String>,
    /// Estimated duration in seconds.
    pub estimated_duration: Option<u64>,
    /// Timestamp when the task execution started.
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Timestamp when the task execution completed.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Task {
    /// Create a new pending task.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            status: TaskStatus::Pending,
            dependencies: Vec::new(),
            input: serde_json::Value::Null,
            output: None,
            error: None,
            assigned_role: None,
            estimated_duration: None,
            started_at: None,
            completed_at: None,
        }
    }

    /// Add dependency on another task.
    pub fn depends_on(mut self, dep_id: impl Into<String>) -> Self {
        self.dependencies.push(dep_id.into());
        self
    }

    /// Assign an agent role to the task.
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.assigned_role = Some(role.into());
        self
    }

    /// Set an estimated duration.
    pub fn with_estimate(mut self, duration_secs: u64) -> Self {
        self.estimated_duration = Some(duration_secs);
        self
    }
}

/// A hierarchical plan representing a complex goal decomposed into tasks.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EpicPlan {
    /// The parent goal being pursued.
    pub goal: String,
    /// Map of task ID to task definition.
    pub tasks: HashMap<String, Task>,
    /// Timestamp when the plan was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp when the plan was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl EpicPlan {
    /// Create a new empty EpicPlan for the given goal.
    pub fn new(goal: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            goal: goal.into(),
            tasks: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a task to the plan.
    pub fn add_task(&mut self, task: Task) {
        self.tasks.insert(task.id.clone(), task);
        self.updated_at = chrono::Utc::now();
    }

    /// Retrieve a reference to a task by ID.
    pub fn get_task(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Retrieve a mutable reference to a task by ID.
    pub fn get_task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.get_mut(id)
    }

    /// Update the status of a specific task.
    pub fn update_task_status(&mut self, id: &str, status: TaskStatus) -> bool {
        let now = chrono::Utc::now();
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = status;
            match status {
                TaskStatus::Running => {
                    task.started_at = Some(now);
                }
                TaskStatus::Completed => {
                    task.completed_at = Some(now);
                }
                TaskStatus::Failed | TaskStatus::Blocked => {
                    task.completed_at = Some(now);
                }
                _ => {}
            }
            self.propagate_blocked_status();
            self.updated_at = now;
            true
        } else {
            false
        }
    }

    /// Propagate failed/blocked statuses down the dependency tree.
    pub fn propagate_blocked_status(&mut self) {
        loop {
            let mut changed = false;
            let mut failed_or_blocked_ids = std::collections::HashSet::new();
            for task in self.tasks.values() {
                if task.status == TaskStatus::Failed || task.status == TaskStatus::Blocked {
                    failed_or_blocked_ids.insert(task.id.clone());
                }
            }

            for task in self.tasks.values_mut() {
                if task.status == TaskStatus::Pending || task.status == TaskStatus::Running {
                    for dep in &task.dependencies {
                        if failed_or_blocked_ids.contains(dep) {
                            task.status = TaskStatus::Blocked;
                            changed = true;
                            break;
                        }
                    }
                }
            }

            if !changed {
                break;
            }
        }
    }

    /// Inject a recovery task to address a failure in a specific task.
    /// The failed task and its downstream tasks will be made dependent on this recovery task.
    pub fn inject_recovery_task(&mut self, failed_task_id: &str, mut recovery_task: Task) -> bool {
        if !self.tasks.contains_key(failed_task_id) {
            return false;
        }

        let rec_id = recovery_task.id.clone();

        // Inherit the original dependencies of the failed task
        if let Some(failed_task) = self.tasks.get(failed_task_id) {
            recovery_task.dependencies = failed_task.dependencies.clone();
        }

        // Add the recovery task to the plan
        self.add_task(recovery_task);

        // Update the failed task to depend solely on the recovery task, resetting its status to Pending
        if let Some(failed_task) = self.tasks.get_mut(failed_task_id) {
            failed_task.dependencies = vec![rec_id];
            failed_task.status = TaskStatus::Pending;
            failed_task.error = None;
        }

        // Re-run block propagation to re-enable correct dependencies
        self.propagate_blocked_status();
        self.updated_at = chrono::Utc::now();
        true
    }
}
