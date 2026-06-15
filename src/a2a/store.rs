//! In-memory task registry for A2A orchestration.

use std::collections::HashMap;
use std::sync::RwLock;

use super::types::{Task, TaskState};

/// Thread-safe in-memory store of A2A tasks.
#[derive(Debug, Default)]
pub struct TaskStore {
    tasks: RwLock<HashMap<String, Task>>,
}

impl TaskStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a task.
    pub fn upsert(&self, task: Task) {
        if let Ok(mut guard) = self.tasks.write() {
            guard.insert(task.id.clone(), task);
        }
    }

    /// Fetch a task by id.
    pub fn get(&self, task_id: &str) -> Option<Task> {
        self.tasks
            .read()
            .ok()
            .and_then(|guard| guard.get(task_id).cloned())
    }

    /// Update an existing task in place.
    pub fn update<F>(&self, task_id: &str, f: F) -> Option<Task>
    where
        F: FnOnce(&mut Task),
    {
        let mut guard = self.tasks.write().ok()?;
        let task = guard.get_mut(task_id)?;
        f(task);
        Some(task.clone())
    }

    /// Mark a task canceled when not terminal.
    pub fn cancel(&self, task_id: &str) -> Option<Task> {
        self.update(task_id, |task| {
            if !task.status.state.is_terminal() {
                task.status.state = TaskState::TaskStateCanceled;
                task.status.timestamp = Some(chrono::Utc::now().to_rfc3339());
            }
        })
    }

    /// List all tasks (newest last).
    pub fn list(&self) -> Vec<Task> {
        self.tasks
            .read()
            .ok()
            .map(|guard| guard.values().cloned().collect())
            .unwrap_or_default()
    }
}
