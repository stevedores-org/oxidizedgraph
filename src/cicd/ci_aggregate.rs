//! CI signal aggregation across repositories.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Conclusion for a single CI check signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiConclusion {
    /// Check succeeded.
    Success,
    /// Check failed.
    Failure,
    /// Check still running.
    Pending,
    /// Check skipped.
    Skipped,
}

/// A CI workflow check signal from one repository.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CiCheckSignal {
    /// Repository slug.
    pub repo: String,
    /// Workflow name (e.g. `validate`).
    pub workflow: String,
    /// Job or check name.
    pub check: String,
    /// Check conclusion.
    pub conclusion: CiConclusion,
    /// Link to workflow run details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl CiCheckSignal {
    /// Create a signal.
    pub fn new(
        repo: impl Into<String>,
        workflow: impl Into<String>,
        check: impl Into<String>,
        conclusion: CiConclusion,
    ) -> Self {
        Self {
            repo: repo.into(),
            workflow: workflow.into(),
            check: check.into(),
            conclusion,
            url: None,
        }
    }
}

/// Consolidated CI report for a multi-repo objective.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CiAggregateReport {
    /// Objective this report covers.
    pub objective_id: String,
    /// All ingested signals.
    pub signals: Vec<CiCheckSignal>,
    /// True when no failures or pending checks remain.
    pub passed: bool,
    /// Repos with at least one failing check.
    pub failing_repos: Vec<String>,
    /// Repos with pending checks.
    pub pending_repos: Vec<String>,
}

/// Aggregates per-repo CI signals into an objective-level report.
#[derive(Clone, Debug, Default)]
pub struct CiAggregator;

impl CiAggregator {
    /// Create an aggregator.
    pub fn new() -> Self {
        Self
    }

    /// Build a consolidated report from raw CI signals.
    pub fn aggregate(objective_id: impl Into<String>, signals: &[CiCheckSignal]) -> CiAggregateReport {
        let objective_id = objective_id.into();
        let mut repo_status: HashMap<String, (bool, bool, bool)> = HashMap::new();

        for signal in signals {
            let entry = repo_status.entry(signal.repo.clone()).or_insert((true, false, false));
            match signal.conclusion {
                CiConclusion::Failure => {
                    entry.0 = false;
                    entry.1 = true;
                }
                CiConclusion::Pending => {
                    entry.0 = false;
                    entry.2 = true;
                }
                CiConclusion::Success | CiConclusion::Skipped => {}
            }
        }

        let mut failing_repos = Vec::new();
        let mut pending_repos = Vec::new();
        for (repo, (all_ok, any_fail, any_pending)) in &repo_status {
            if *any_fail {
                failing_repos.push(repo.clone());
            } else if *any_pending || !all_ok {
                pending_repos.push(repo.clone());
            }
        }
        failing_repos.sort();
        pending_repos.sort();

        let passed = failing_repos.is_empty() && pending_repos.is_empty() && !signals.is_empty();

        CiAggregateReport {
            objective_id,
            signals: signals.to_vec(),
            passed,
            failing_repos,
            pending_repos,
        }
    }

    /// Whether release should be blocked based on the aggregate report.
    pub fn blocks_release(&self, report: &CiAggregateReport) -> bool {
        !report.passed
    }

    /// Repos represented in the signal set.
    pub fn repos_in_scope(signals: &[CiCheckSignal]) -> Vec<String> {
        let mut repos: HashSet<_> = signals.iter().map(|s| s.repo.clone()).collect();
        let mut sorted: Vec<_> = repos.drain().collect();
        sorted.sort();
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_failures_across_repos() {
        let signals = vec![
            CiCheckSignal::new("org/infra", "validate", "cargo test", CiConclusion::Success),
            CiCheckSignal::new("org/app", "validate", "cargo test", CiConclusion::Failure),
        ];
        let report = CiAggregator::aggregate("issue-27", &signals);
        assert!(!report.passed);
        assert_eq!(report.failing_repos, vec!["org/app"]);
    }

    #[test]
    fn pending_checks_block_release() {
        let signals = vec![CiCheckSignal::new(
            "org/lib",
            "validate",
            "nix build",
            CiConclusion::Pending,
        )];
        let report = CiAggregator::aggregate("issue-27", &signals);
        assert!(!report.passed);
        assert_eq!(report.pending_repos, vec!["org/lib"]);
    }
}
