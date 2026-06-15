//! HITL request, decision, and explainability types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::guardrails::RiskLevel;

/// Context key: execution is paused awaiting human input.
pub const CTX_HITL_PAUSED: &str = "hitl_paused";
/// Context key: pending approval request payload.
pub const CTX_APPROVAL_REQUEST: &str = "approval_request";
/// Context key: latest approval decision.
pub const CTX_APPROVAL_DECISION: &str = "approval_decision";
/// Context key: append-only approval audit log.
pub const CTX_APPROVAL_EVENTS: &str = "approval_events";
/// Context key: standardized explanation for operators.
pub const CTX_EXPLANATION: &str = "hitl_explanation";

/// Lifecycle status of an approval request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Awaiting human decision.
    Pending,
    /// Human approved continuation.
    Approved,
    /// Human rejected the action.
    Denied,
}

/// A checkpoint request for human review.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique request id.
    pub id: String,
    /// Human-readable reason for the pause.
    pub reason: String,
    /// Classified risk driving the checkpoint.
    pub risk_level: RiskLevel,
    /// Optional tool name when pausing on tool policy.
    pub tool_name: Option<String>,
    /// Short summary of the proposed change.
    pub change_summary: Option<String>,
    /// When the request was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ApprovalRequest {
    /// Create a new pending approval request.
    pub fn new(reason: impl Into<String>, risk_level: RiskLevel) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            reason: reason.into(),
            risk_level,
            tool_name: None,
            change_summary: None,
            created_at: chrono::Utc::now(),
        }
    }

    /// Attach tool context.
    pub fn with_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }

    /// Attach change summary.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.change_summary = Some(summary.into());
        self
    }
}

/// Operator decision on an approval request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApprovalDecision {
    /// Request this decision applies to.
    pub request_id: String,
    /// Outcome.
    pub status: ApprovalStatus,
    /// Approver identity or role label.
    pub approver: String,
    /// Optional rationale recorded by the operator.
    pub rationale: Option<String>,
    /// When the decision was recorded.
    pub decided_at: chrono::DateTime<chrono::Utc>,
}

impl ApprovalDecision {
    /// Record an approval.
    pub fn approve(request_id: impl Into<String>, approver: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            status: ApprovalStatus::Approved,
            approver: approver.into(),
            rationale: None,
            decided_at: chrono::Utc::now(),
        }
    }

    /// Record a denial.
    pub fn deny(
        request_id: impl Into<String>,
        approver: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            status: ApprovalStatus::Denied,
            approver: approver.into(),
            rationale: Some(rationale.into()),
            decided_at: chrono::Utc::now(),
        }
    }
}

/// Explainability payload shown to operators at a checkpoint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExplanationPayload {
    /// One-line summary of what the agent intends to do.
    pub summary: String,
    /// Why the checkpoint fired.
    pub rationale: String,
    /// Optional diff or file hint.
    pub diff_hint: Option<String>,
    /// Recent tool actions leading to the checkpoint.
    pub tool_actions: Vec<String>,
}

impl ExplanationPayload {
    /// Build from checkpoint context.
    pub fn new(summary: impl Into<String>, rationale: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            rationale: rationale.into(),
            diff_hint: None,
            tool_actions: Vec::new(),
        }
    }
}

/// Immutable audit event for approval workflows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApprovalEvent {
    /// Event id.
    pub id: String,
    /// Event timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Event kind label.
    pub kind: String,
    /// Serialized payload (request, decision, or explanation).
    pub detail: serde_json::Value,
}

impl ApprovalEvent {
    /// Record a checkpoint created event.
    pub fn checkpoint_created(request: &ApprovalRequest) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            kind: "checkpoint_created".to_string(),
            detail: serde_json::to_value(request).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Record a decision event.
    pub fn decision_recorded(decision: &ApprovalDecision) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            kind: "decision_recorded".to_string(),
            detail: serde_json::to_value(decision).unwrap_or(serde_json::Value::Null),
        }
    }
}

/// Append an approval event to agent context.
pub fn append_approval_event(state: &mut crate::state::AgentState, event: ApprovalEvent) {
    let mut events: Vec<ApprovalEvent> = state.get_context(CTX_APPROVAL_EVENTS).unwrap_or_default();
    events.push(event);
    state.set_context(CTX_APPROVAL_EVENTS, events);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_request_round_trips() {
        let req = ApprovalRequest::new("shell invocation", RiskLevel::High).with_tool("shell");
        assert_eq!(req.tool_name.as_deref(), Some("shell"));
        assert_eq!(req.risk_level, RiskLevel::High);
    }
}
