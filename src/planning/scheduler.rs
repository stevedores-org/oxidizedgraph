use crate::planning::plan::{EpicPlan, TaskStatus};
use std::collections::{HashMap, HashSet};

/// Dependency-aware Scheduler for EpicPlan tasks.
#[derive(Clone, Debug, Default)]
pub struct Scheduler;

impl Scheduler {
    /// Create a new Scheduler instance.
    pub fn new() -> Self {
        Self
    }

    /// Retrieve task IDs that are ready for execution.
    /// A task is ready if its status is Pending and all its dependencies are Completed.
    pub fn next_tasks(&self, plan: &EpicPlan) -> Vec<String> {
        let mut ready = Vec::new();
        for task in plan.tasks.values() {
            if task.status != TaskStatus::Pending {
                continue;
            }

            let mut deps_satisfied = true;
            for dep_id in &task.dependencies {
                if let Some(dep_task) = plan.get_task(dep_id) {
                    if dep_task.status != TaskStatus::Completed {
                        deps_satisfied = false;
                        break;
                    }
                } else {
                    // Dependency does not exist in the plan; treat as unsatisfied
                    deps_satisfied = false;
                    break;
                }
            }

            if deps_satisfied {
                ready.push(task.id.clone());
            }
        }
        ready
    }

    /// Check if the dependency graph of the plan contains cycles.
    pub fn has_cycles(&self, plan: &EpicPlan) -> bool {
        let mut visited = HashMap::new(); // 0 = unvisited, 1 = visiting, 2 = visited

        fn dfs(node: &str, plan: &EpicPlan, visited: &mut HashMap<String, u8>) -> bool {
            visited.insert(node.to_string(), 1);
            if let Some(task) = plan.get_task(node) {
                for dep in &task.dependencies {
                    let state = visited.get(dep).copied().unwrap_or(0);
                    if state == 1 {
                        return true; // Cycle detected
                    } else if state == 0 && dfs(dep, plan, visited) {
                        return true;
                    }
                }
            }
            visited.insert(node.to_string(), 2);
            false
        }

        for id in plan.tasks.keys() {
            if visited.get(id).copied().unwrap_or(0) == 0 && dfs(id, plan, &mut visited) {
                return true;
            }
        }
        false
    }

    /// Prioritize a set of task IDs based on the transitively downstream dependencies (critical path).
    /// Tasks that block more downstream tasks will be sorted first.
    pub fn prioritize_tasks(&self, plan: &EpicPlan, ready_task_ids: &[String]) -> Vec<String> {
        let mut downstream_counts = HashMap::new();

        // Calculate transitive downstream count for every task
        for id in plan.tasks.keys() {
            let mut visited = HashSet::new();
            let mut stack = vec![id.clone()];
            while let Some(curr) = stack.pop() {
                if let Some(task) = plan.get_task(&curr) {
                    for dep in &task.dependencies {
                        if visited.insert(dep.clone()) {
                            stack.push(dep.clone());
                        }
                    }
                }
            }
            for dep in visited {
                *downstream_counts.entry(dep).or_insert(0) += 1;
            }
        }

        let mut sorted = ready_task_ids.to_vec();
        sorted.sort_by(|a, b| {
            let count_a = downstream_counts.get(a).copied().unwrap_or(0);
            let count_b = downstream_counts.get(b).copied().unwrap_or(0);
            count_b.cmp(&count_a) // Descending order (higher block counts first)
        });

        sorted
    }
}
