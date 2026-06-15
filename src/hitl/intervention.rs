//! Operator intervention API: pause, edit, approve, deny, and resume.

use serde::{Deserialize, Serialize};

use thiserror::Error;

use crate::guardrails::RiskLevel;
use crate::state::AgentState;

use super::explain::ReviewSummaryBuilder;
use super::policy::ApprovalPolicy;
use super::types::{
    append_approval_event, ApprovalDecision, ApprovalEvent, ApprovalRequest,
    ExplanationPayload, InterventionEdit, CTX_APPROVAL_DECISION, CTX_APPROVAL_EVENTS,
    CTX_APPROVAL_REQUEST, CTX_EXPLANATION, CTX_HITL_EDITS, CTX_HITL_PAUSED,
};

/// Current HITL status for a paused run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitlStatus {
    /// Whether execution is paused for human input.
    pub paused: bool,
    /// Pending approval request when paused.
    pub request: Option<ApprovalRequest>,
    /// Explainability payload for operators.
    pub explanation: Option<ExplanationPayload>,
    /// Latest decision if recorded.
    pub decision: Option<ApprovalDecision>,
    /// Immutable approval audit events.
    pub events: Vec<ApprovalEvent>,
}

/// Intervention API errors.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum HitlError {
    /// No pending approval request to act on.
    #[error("no pending approval request")]
    NoPendingRequest,
    /// Run is not paused for intervention.
    #[error("run is not paused for intervention")]
    NotPaused,
}

/// Programmatic hooks for operator pause, edit, and resume flows.
#[derive(Clone, Debug, Default)]
pub struct HitlController {
    policy: ApprovalPolicy,
    summarizer: ReviewSummaryBuilder,
}

impl HitlController {
    /// Create with default approval policy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a custom policy matrix.
    pub fn with_policy(policy: ApprovalPolicy) -> Self {
        Self {
            policy,
            summarizer: ReviewSummaryBuilder::new(),
        }
    }

    /// Read current HITL status from agent state.
    pub fn status(&self, state: &AgentState) -> HitlStatus {
        HitlStatus {
            paused: state.get_context(CTX_HITL_PAUSED).unwrap_or(false),
            request: state.get_context(CTX_APPROVAL_REQUEST),
            explanation: state.get_context(CTX_EXPLANATION),
            decision: state.get_context(CTX_APPROVAL_DECISION),
            events: state.get_context(CTX_APPROVAL_EVENTS).unwrap_or_default(),
        }
    }

    /// Manually pause a run and create an approval checkpoint.
    pub fn pause(
        &self,
        state: &mut AgentState,
        reason: impl Into<String>,
        risk_level: RiskLevel,
    ) -> ApprovalRequest {
        let summary = state
            .get_context::<String>("change_summary")
            .unwrap_or_default();
        let request = ApprovalRequest::new(reason, risk_level).with_summary(summary);
        let review = self.summarizer.from_state(
            state,
            format!("Manual pause: {}", request.reason),
        );
        let explanation = self.summarizer.to_explanation(&review);

        append_approval_event(state, ApprovalEvent::checkpoint_created(&request));
        state.set_context(CTX_APPROVAL_REQUEST, request.clone());
        state.set_context(CTX_EXPLANATION, explanation);
        state.set_context(CTX_HITL_PAUSED, true);
        state.remove_context(CTX_APPROVAL_DECISION);

        request
    }

    /// Queue operator edits to apply before resume (without losing state).
    pub fn queue_edits(&self, state: &mut AgentState, edits: InterventionEdit) -> Result<(), HitlError> {
        if !state.get_context::<bool>(CTX_HITL_PAUSED).unwrap_or(false) {
            return Err(HitlError::NotPaused);
        }
        append_approval_event(
            state,
            ApprovalEvent::intervention_edited(&edits),
        );
        state.set_context(CTX_HITL_EDITS, edits);
        Ok(())
    }

    /// Apply queued edits into live context (called by `EditInterventionNode` or resume).
    pub fn apply_edits(state: &mut AgentState) {
        if let Some(edits) = state.get_context::<InterventionEdit>(CTX_HITL_EDITS) {
            for (key, value) in edits.context_patches {
                state.set_context(key, value);
            }
        }
    }

    /// Record operator approval and clear pause.
    pub fn approve(
        &self,
        state: &mut AgentState,
        approver: impl Into<String>,
        rationale: Option<String>,
    ) -> Result<ApprovalDecision, HitlError> {
        let request: ApprovalRequest = state
            .get_context(CTX_APPROVAL_REQUEST)
            .ok_or(HitlError::NoPendingRequest)?;

        Self::apply_edits(state);

        let mut decision = ApprovalDecision::approve(&request.id, approver);
        decision.rationale = rationale;

        append_approval_event(state, ApprovalEvent::decision_recorded(&decision));
        state.set_context(CTX_APPROVAL_DECISION, decision.clone());
        state.set_context(CTX_HITL_PAUSED, false);
        state.remove_context(CTX_HITL_EDITS);

        Ok(decision)
    }

    /// Record operator denial and keep audit trail.
    pub fn deny(
        &self,
        state: &mut AgentState,
        approver: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Result<ApprovalDecision, HitlError> {
        let request: ApprovalRequest = state
            .get_context(CTX_APPROVAL_REQUEST)
            .ok_or(HitlError::NoPendingRequest)?;

        let decision = ApprovalDecision::deny(&request.id, approver, rationale);

        append_approval_event(state, ApprovalEvent::decision_recorded(&decision));
        state.set_context(CTX_APPROVAL_DECISION, decision.clone());
        state.set_context(CTX_HITL_PAUSED, false);
        state.remove_context(CTX_HITL_EDITS);

        Ok(decision)
    }

    /// Clear pause without recording a decision (e.g. external resume hook).
    pub fn resume(&self, state: &mut AgentState) {
        Self::apply_edits(state);
        state.set_context(CTX_HITL_PAUSED, false);
    }

    /// Whether policy requires pause for the given risk level.
    pub fn requires_pause(&self, risk: RiskLevel) -> bool {
        self.policy.matrix.requires_pause(risk)
    }
}
