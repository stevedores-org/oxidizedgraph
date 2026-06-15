//! Explainability and diff summaries for operator review.

use serde::{Deserialize, Serialize};

use crate::guardrails::ReviewFinding;
use crate::state::AgentState;

use super::types::ExplanationPayload;

/// Context key for a proposed change diff (unified diff or summary text).
pub const CTX_PROPOSED_DIFF: &str = "proposed_diff";
/// Context key for recent tool actions leading to a checkpoint.
pub const CTX_RECENT_TOOL_ACTIONS: &str = "recent_tool_actions";
/// Context key for machine-readable review findings at checkpoint.
pub const CTX_REVIEW_FINDINGS: &str = "review_findings";

/// Operator-facing review bundle combining summary, diff, and findings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReviewSummary {
    /// One-line change summary.
    pub summary: String,
    /// Why the checkpoint fired.
    pub rationale: String,
    /// Diff or file-change hint for reviewers.
    pub diff_hint: Option<String>,
    /// Recent tool invocations.
    pub tool_actions: Vec<String>,
    /// Structured findings from quality gates or reviewers.
    pub findings: Vec<ReviewFinding>,
}

impl ReviewSummary {
    /// Render a compact markdown block for PR bodies or operator UI.
    pub fn to_markdown(&self) -> String {
        let mut lines = vec![
            format!("**Summary:** {}", self.summary),
            format!("**Rationale:** {}", self.rationale),
        ];
        if let Some(diff) = &self.diff_hint {
            lines.push(format!("**Diff:**\n```\n{diff}\n```"));
        }
        if !self.tool_actions.is_empty() {
            lines.push("**Recent tools:**".to_string());
            for action in &self.tool_actions {
                lines.push(format!("- {action}"));
            }
        }
        if !self.findings.is_empty() {
            lines.push("**Findings:**".to_string());
            for finding in &self.findings {
                lines.push(format!(
                    "- [{:?}] {}: {}",
                    finding.severity, finding.rule_id, finding.message
                ));
            }
        }
        lines.join("\n")
    }
}

/// Builds standardized explanation payloads from agent state.
#[derive(Clone, Debug, Default)]
pub struct ReviewSummaryBuilder;

impl ReviewSummaryBuilder {
    /// Create a builder.
    pub fn new() -> Self {
        Self
    }

    /// Build a review summary from checkpoint context.
    pub fn from_state(&self, state: &AgentState, rationale: impl Into<String>) -> ReviewSummary {
        let summary = state
            .get_context::<String>("change_summary")
            .unwrap_or_else(|| "Autonomous change pending review".to_string());
        let diff_hint = state.get_context::<String>(CTX_PROPOSED_DIFF);
        let tool_actions: Vec<String> = state
            .get_context(CTX_RECENT_TOOL_ACTIONS)
            .unwrap_or_default();
        let findings: Vec<ReviewFinding> =
            state.get_context(CTX_REVIEW_FINDINGS).unwrap_or_default();

        ReviewSummary {
            summary,
            rationale: rationale.into(),
            diff_hint,
            tool_actions,
            findings,
        }
    }

    /// Convert to the graph context explanation payload.
    pub fn to_explanation(&self, review: &ReviewSummary) -> ExplanationPayload {
        ExplanationPayload {
            summary: review.summary.clone(),
            rationale: review.rationale.clone(),
            diff_hint: review.diff_hint.clone(),
            tool_actions: review.tool_actions.clone(),
        }
    }
}
