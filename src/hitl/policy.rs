//! Human-in-the-loop approval policy matrix.

use serde::{Deserialize, Serialize};

use crate::guardrails::RiskLevel;

/// Action taken when a checkpoint is evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalAction {
    /// Continue without pausing.
    Allow,
    /// Pause execution until a human approves.
    Pause,
    /// Reject and route to a denial path.
    Deny,
}

/// Maps risk levels to required approval behavior.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ApprovalMatrix {
    low: ApprovalAction,
    medium: ApprovalAction,
    high: ApprovalAction,
}

impl Default for ApprovalMatrix {
    fn default() -> Self {
        Self {
            low: ApprovalAction::Allow,
            medium: ApprovalAction::Pause,
            high: ApprovalAction::Pause,
        }
    }
}

impl ApprovalMatrix {
    /// Permissive matrix: only high-risk pauses.
    pub fn permissive() -> Self {
        Self::default()
    }

    /// Strict matrix: medium and high both pause.
    pub fn strict() -> Self {
        Self::default()
    }

    /// Resolve the action for a risk level.
    pub fn action_for(&self, risk: RiskLevel) -> ApprovalAction {
        match risk {
            RiskLevel::Low => self.low,
            RiskLevel::Medium => self.medium,
            RiskLevel::High => self.high,
        }
    }

    /// Whether execution must pause for human input.
    pub fn requires_pause(&self, risk: RiskLevel) -> bool {
        matches!(self.action_for(risk), ApprovalAction::Pause)
    }
}

/// Named approver role for audit trails.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApproverRole(pub String);

impl ApproverRole {
    /// Default operator role.
    pub fn operator() -> Self {
        Self("operator".to_string())
    }

    /// Security reviewer role.
    pub fn security() -> Self {
        Self("security".to_string())
    }
}

/// Policy bundle used by checkpoint nodes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ApprovalPolicy {
    /// Risk-to-action matrix.
    pub matrix: ApprovalMatrix,
    /// Role recorded on approval events.
    pub approver_role: ApproverRole,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            matrix: ApprovalMatrix::default(),
            approver_role: ApproverRole::operator(),
        }
    }
}

impl ApprovalPolicy {
    /// Create policy with a custom matrix.
    pub fn with_matrix(matrix: ApprovalMatrix) -> Self {
        Self {
            matrix,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_risk_pauses_by_default() {
        let policy = ApprovalPolicy::default();
        assert!(policy.matrix.requires_pause(RiskLevel::High));
        assert!(!policy.matrix.requires_pause(RiskLevel::Low));
    }
}
