use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::SharedState;

/// Severity level for a review finding
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A machine-readable finding from a quality review or check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub severity: Severity,
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
    pub rule_id: String,
}

/// A comprehensive report of the quality of a change
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityReport {
    pub findings: Vec<ReviewFinding>,
    pub passed_checks: Vec<String>,
    pub failed_checks: Vec<String>,
}

impl QualityReport {
    /// Evaluates merge blocker rules
    pub fn is_mergeable(&self) -> bool {
        self.failed_checks.is_empty()
            && !self.findings.iter().any(|f| f.severity == Severity::Error)
    }
}

/// Gate orchestration node for CI-quality checks
pub struct QualityGateNode {
    id: String,
}

impl QualityGateNode {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl NodeExecutor for QualityGateNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let guard = state
            .read()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let report = guard
            .get_context::<QualityReport>("quality_report")
            .unwrap_or_default();

        if !report.is_mergeable() {
            // Block merge if required checks fail
            return Ok(NodeOutput::transition("block"));
        }

        Ok(NodeOutput::transition("pass"))
    }

    fn description(&self) -> Option<&str> {
        Some("Evaluates quality report and blocks merge if checks fail")
    }
}

/// Risk level assigned to a change operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High, // e.g. destructive operations
}

/// Change-risk classifier node for approval routing
pub struct ChangeRiskClassifierNode {
    id: String,
}

impl ChangeRiskClassifierNode {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl NodeExecutor for ChangeRiskClassifierNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let guard = state
            .read()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let risk = guard
            .get_context::<RiskLevel>("risk_level")
            .unwrap_or(RiskLevel::Low);

        if risk == RiskLevel::High {
            // High-risk actions require explicit human approval
            return Ok(NodeOutput::transition("require_approval"));
        }

        Ok(NodeOutput::transition("auto_approve"))
    }

    fn description(&self) -> Option<&str> {
        Some("Routes change approval based on classified risk level")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;
    use std::sync::{Arc, RwLock};

    #[test]
    fn test_quality_report_mergeable() {
        let mut report = QualityReport::default();
        assert!(report.is_mergeable());

        report.findings.push(ReviewFinding {
            severity: Severity::Warning,
            file: "src/main.rs".to_string(),
            line: Some(10),
            message: "Unused variable".to_string(),
            rule_id: "W001".to_string(),
        });
        assert!(report.is_mergeable()); // Warnings don't block

        report.findings.push(ReviewFinding {
            severity: Severity::Error,
            file: "src/main.rs".to_string(),
            line: None,
            message: "Syntax error".to_string(),
            rule_id: "E001".to_string(),
        });
        assert!(!report.is_mergeable()); // Errors block

        report.findings.clear();
        report.failed_checks.push("test-suite".to_string());
        assert!(!report.is_mergeable()); // Failed checks block
    }

    #[tokio::test]
    async fn test_quality_gate_node() {
        let node = QualityGateNode::new("quality_gate");
        let state = Arc::new(RwLock::new(AgentState::new()));

        {
            let mut guard = state.write().unwrap();
            let mut report = QualityReport::default();
            report.failed_checks.push("lint".to_string());
            guard.set_context("quality_report", report);
        }

        let result = node.execute(state.clone()).await.unwrap();
        assert_eq!(result.target(), Some("block"));

        {
            let mut guard = state.write().unwrap();
            let report = QualityReport::default();
            guard.set_context("quality_report", report);
        }

        let result = node.execute(state).await.unwrap();
        assert_eq!(result.target(), Some("pass"));
    }

    #[tokio::test]
    async fn test_change_risk_classifier_node() {
        let node = ChangeRiskClassifierNode::new("risk_classifier");
        let state = Arc::new(RwLock::new(AgentState::new()));

        {
            let mut guard = state.write().unwrap();
            guard.set_context("risk_level", RiskLevel::High);
        }

        let result = node.execute(state.clone()).await.unwrap();
        assert_eq!(result.target(), Some("require_approval"));

        {
            let mut guard = state.write().unwrap();
            guard.set_context("risk_level", RiskLevel::Low);
        }

        let result = node.execute(state).await.unwrap();
        assert_eq!(result.target(), Some("auto_approve"));
    }
}
