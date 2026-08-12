//! Human-in-the-loop controls for high-trust autonomous workflows.
//!
//! EPIC8 baseline for [issue #26](aivcs://stevedores-org/oxidizedgraph/issues/26):
//! approval checkpoints, operator decisions, explainability payloads, and
//! immutable approval audit events.

mod node;
mod policy;
mod timeline;
mod types;

pub use node::{ApprovalCheckpointNode, GrantApprovalNode, ResumeNode};
pub use policy::{ApprovalAction, ApprovalMatrix, ApprovalPolicy, ApproverRole};
pub use timeline::{RunTimeline, TimelineEntry};
pub use types::{
    append_approval_event, ApprovalDecision, ApprovalEvent, ApprovalRequest, ApprovalStatus,
    ExplanationPayload, CTX_APPROVAL_DECISION, CTX_APPROVAL_EVENTS, CTX_APPROVAL_REQUEST,
    CTX_EXPLANATION, CTX_HITL_PAUSED,
};
