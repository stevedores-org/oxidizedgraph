//! Multi-repository execution coordinator.

use std::collections::HashMap;

use super::change_graph::{CrossRepoChangeGraph, RepoChangeStatus};

/// Coordinates cross-repo changes in dependency order.
#[derive(Clone, Debug, Default)]
pub struct MultiRepoCoordinator;

impl MultiRepoCoordinator {
    /// Create a coordinator.
    pub fn new() -> Self {
        Self
    }

    /// Return change IDs ready to execute (deps released or CI-passed).
    pub fn ready_changes(&self, graph: &CrossRepoChangeGraph) -> Vec<String> {
        let mut ready = Vec::new();
        for change in graph.changes.values() {
            if change.status != RepoChangeStatus::Pending {
                continue;
            }
            let deps_ok = change.dependencies.iter().all(|dep_id| {
                graph
                    .get_change(dep_id)
                    .map(|dep| {
                        matches!(
                            dep.status,
                            RepoChangeStatus::CiPassed | RepoChangeStatus::Released
                        )
                    })
                    .unwrap_or(false)
            });
            if deps_ok {
                ready.push(change.id.clone());
            }
        }
        ready.sort();
        ready
    }

    /// Detect dependency cycles.
    pub fn has_cycles(&self, graph: &CrossRepoChangeGraph) -> bool {
        let mut visited = HashMap::new();

        fn dfs(id: &str, graph: &CrossRepoChangeGraph, visited: &mut HashMap<String, u8>) -> bool {
            visited.insert(id.to_string(), 1);
            if let Some(change) = graph.get_change(id) {
                for dep in &change.dependencies {
                    let state = visited.get(dep.as_str()).copied().unwrap_or(0);
                    if state == 1 {
                        return true;
                    }
                    if state == 0 && dfs(dep, graph, visited) {
                        return true;
                    }
                }
            }
            visited.insert(id.to_string(), 2);
            false
        }

        for id in graph.changes.keys() {
            if visited.get(id).copied().unwrap_or(0) == 0 && dfs(id, graph, &mut visited) {
                return true;
            }
        }
        false
    }

    /// Mark a change status and propagate blocks to downstream repos on failure.
    pub fn mark_status(
        &self,
        graph: &mut CrossRepoChangeGraph,
        change_id: &str,
        status: RepoChangeStatus,
    ) -> bool {
        if graph.get_change_mut(change_id).is_none() {
            return false;
        }
        graph.get_change_mut(change_id).unwrap().status = status;
        if status == RepoChangeStatus::CiFailed {
            self.propagate_blocks(graph);
        }
        true
    }

    /// Block pending/in-progress downstream changes when upstream fails.
    pub fn propagate_blocks(&self, graph: &mut CrossRepoChangeGraph) {
        loop {
            let mut changed = false;
            let failed_or_blocked: Vec<String> = graph
                .changes
                .values()
                .filter(|c| {
                    matches!(
                        c.status,
                        RepoChangeStatus::CiFailed | RepoChangeStatus::Blocked
                    )
                })
                .map(|c| c.id.clone())
                .collect();

            for change in graph.changes.values_mut() {
                if matches!(
                    change.status,
                    RepoChangeStatus::Pending | RepoChangeStatus::InProgress
                ) {
                    for dep in &change.dependencies {
                        if failed_or_blocked.contains(dep) {
                            change.status = RepoChangeStatus::Blocked;
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

    /// True when every change is released.
    pub fn all_released(&self, graph: &CrossRepoChangeGraph) -> bool {
        !graph.changes.is_empty()
            && graph
                .changes
                .values()
                .all(|c| c.status == RepoChangeStatus::Released)
    }

    /// True when any change failed or is blocked.
    pub fn has_rollout_failure(&self, graph: &CrossRepoChangeGraph) -> bool {
        graph.changes.values().any(|c| {
            matches!(
                c.status,
                RepoChangeStatus::CiFailed | RepoChangeStatus::Blocked
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cicd::change_graph::RepoChange;

    fn sample_graph() -> CrossRepoChangeGraph {
        let mut graph = CrossRepoChangeGraph::new("issue-27", "run-1");
        graph.add_change(RepoChange::new("infra", "org/infra", "Publish module"));
        graph.add_change(RepoChange::new("lib", "org/lib", "Bump dependency").depends_on("infra"));
        graph.add_change(RepoChange::new("app", "org/app", "Deploy").depends_on("lib"));
        graph
    }

    #[test]
    fn executes_in_dependency_order() {
        let coordinator = MultiRepoCoordinator::new();
        let mut graph = sample_graph();

        assert_eq!(coordinator.ready_changes(&graph), vec!["infra"]);
        coordinator.mark_status(&mut graph, "infra", RepoChangeStatus::CiPassed);
        assert_eq!(coordinator.ready_changes(&graph), vec!["lib"]);
    }

    #[test]
    fn upstream_failure_blocks_downstream() {
        let coordinator = MultiRepoCoordinator::new();
        let mut graph = sample_graph();
        coordinator.mark_status(&mut graph, "infra", RepoChangeStatus::CiFailed);

        assert!(coordinator.has_rollout_failure(&graph));
        assert_eq!(
            graph.get_change("lib").unwrap().status,
            RepoChangeStatus::Blocked
        );
        assert_eq!(
            graph.get_change("app").unwrap().status,
            RepoChangeStatus::Blocked
        );
    }
}
