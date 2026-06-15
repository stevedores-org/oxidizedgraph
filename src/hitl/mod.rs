//! Human-in-the-loop controls for high-trust autonomous workflows.
//!
//! EPIC8 baseline for [issue #26](https://github.com/stevedores-org/oxidizedgraph/issues/26):
//! approval checkpoints, operator decisions, explainability payloads, and
//! immutable approval audit events.

mod explain;
mod intervention;
mod node;
mod policy;
mod timeline;
mod types;

pub use explain::{
    ReviewSummary, ReviewSummaryBuilder, CTX_PROPOSED_DIFF, CTX_RECENT_TOOL_ACTIONS,
    CTX_REVIEW_FINDINGS,
};
pub use intervention::{HitlController, HitlError, HitlStatus};
pub use node::{ApprovalCheckpointNode, EditInterventionNode, GrantApprovalNode, ResumeNode};
pub use policy::{ApprovalAction, ApprovalMatrix, ApprovalPolicy, ApproverRole};
pub use timeline::{RunTimeline, TimelineEntry};
pub use types::{
    append_approval_event, ApprovalDecision, ApprovalEvent, ApprovalRequest, ApprovalStatus,
    ExplanationPayload, InterventionEdit, CTX_APPROVAL_DECISION, CTX_APPROVAL_EVENTS,
    CTX_APPROVAL_REQUEST, CTX_EXPLANATION, CTX_HITL_EDITS, CTX_HITL_PAUSED,
};
