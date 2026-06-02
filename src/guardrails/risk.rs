//! Change-risk classification for approval routing.

use serde::{Deserialize, Serialize};

/// Risk level for an autonomous change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Low risk — auto-merge eligible when gates pass
    Low,
    /// Medium — may need review
    Medium,
    /// High — requires explicit human approval
    High,
}

/// Describes a proposed change for risk scoring.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChangeRisk {
    /// Number of files touched
    pub files_changed: u32,
    /// Whether destructive git operations are requested
    pub destructive_git: bool,
    /// Whether shell commands were used
    pub used_shell: bool,
    /// Whether network tools were invoked
    pub used_network: bool,
    /// Lines deleted (approximate)
    pub lines_deleted: u32,
}

/// Merge blocker rule outcome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MergeBlocker {
    /// Whether merge is blocked
    pub blocked: bool,
    /// Reason shown to humans/agents
    pub reason: String,
}

/// Classifies change risk for human-in-the-loop routing.
#[derive(Clone, Debug, Default)]
pub struct RiskClassifier {
    high_risk_delete_threshold: u32,
}

impl RiskClassifier {
    /// Create a classifier with default thresholds.
    pub fn new() -> Self {
        Self {
            high_risk_delete_threshold: 100,
        }
    }

    /// Classify change risk.
    pub fn classify(&self, change: &ChangeRisk) -> RiskLevel {
        if change.destructive_git {
            return RiskLevel::High;
        }
        if change.lines_deleted >= self.high_risk_delete_threshold {
            return RiskLevel::High;
        }
        if change.used_network || change.files_changed > 20 {
            return RiskLevel::Medium;
        }
        if change.used_shell || change.files_changed > 5 {
            return RiskLevel::Medium;
        }
        RiskLevel::Low
    }

    /// Evaluate merge blocker from gate pass status and risk.
    pub fn merge_blocker(&self, gates_passed: bool, risk: RiskLevel) -> MergeBlocker {
        if !gates_passed {
            return MergeBlocker {
                blocked: true,
                reason: "Required quality gates failed".to_string(),
            };
        }
        if risk == RiskLevel::High {
            return MergeBlocker {
                blocked: true,
                reason: "High-risk change requires explicit human approval".to_string(),
            };
        }
        MergeBlocker {
            blocked: false,
            reason: String::new(),
        }
    }

    /// Transition key for routing after risk evaluation.
    pub fn approval_route(risk: RiskLevel, gates_passed: bool) -> &'static str {
        if !gates_passed {
            return "gate_failed";
        }
        match risk {
            RiskLevel::High => "needs_approval",
            RiskLevel::Medium => "review",
            RiskLevel::Low => "auto_continue",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_risk_destructive_git() {
        let classifier = RiskClassifier::new();
        let level = classifier.classify(&ChangeRisk {
            destructive_git: true,
            ..Default::default()
        });
        assert_eq!(level, RiskLevel::High);
    }

    #[test]
    fn test_merge_blocker_on_failed_gates() {
        let classifier = RiskClassifier::new();
        let blocker = classifier.merge_blocker(false, RiskLevel::Low);
        assert!(blocker.blocked);
    }
}
