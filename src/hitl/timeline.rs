//! Operator-facing run timeline from traces and approval events.

use serde::{Deserialize, Serialize};

use crate::execution::TransitionRecord;

use super::types::ApprovalEvent;

/// One entry in an operator timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimelineEntry {
    /// Monotonic sequence number.
    pub seq: usize,
    /// Entry kind: `transition` or `approval`.
    pub kind: String,
    /// Short label for display.
    pub label: String,
    /// ISO timestamp when available.
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// Optional detail payload.
    pub detail: Option<serde_json::Value>,
}

/// Aggregated operator view of a run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunTimeline {
    /// Ordered entries.
    pub entries: Vec<TimelineEntry>,
}

impl RunTimeline {
    /// Build a timeline from transition records and approval events.
    pub fn from_artifacts(
        transitions: &[TransitionRecord],
        approvals: &[ApprovalEvent],
    ) -> Self {
        let mut entries = Vec::new();
        let mut seq = 0usize;

        for record in transitions {
            entries.push(TimelineEntry {
                seq,
                kind: "transition".to_string(),
                label: format!("{} -> {:?}", record.node_id, record.next_node),
                timestamp: Some(record.recorded_at),
                detail: Some(serde_json::json!({
                    "iteration": record.iteration,
                    "output_kind": record.output_kind,
                    "run_id": record.run_id,
                })),
            });
            seq += 1;
        }

        for event in approvals {
            entries.push(TimelineEntry {
                seq,
                kind: "approval".to_string(),
                label: event.kind.clone(),
                timestamp: Some(event.timestamp),
                detail: Some(event.detail.clone()),
            });
            seq += 1;
        }

        Self { entries }
    }

    /// Number of approval events in the timeline.
    pub fn approval_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == "approval")
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn builds_mixed_timeline() {
        let transitions = vec![TransitionRecord {
            run_id: "run-1".to_string(),
            iteration: 0,
            node_id: "gate".to_string(),
            output_kind: "transition".to_string(),
            next_node: Some("checkpoint".to_string()),
            output_target: Some("awaiting_approval".to_string()),
            state_iteration: 0,
            recorded_at: Utc::now(),
        }];
        let approvals = vec![ApprovalEvent {
            id: "evt-1".to_string(),
            timestamp: Utc::now(),
            kind: "checkpoint_created".to_string(),
            detail: serde_json::json!({}),
        }];

        let timeline = RunTimeline::from_artifacts(&transitions, &approvals);
        assert_eq!(timeline.entries.len(), 2);
        assert_eq!(timeline.approval_count(), 1);
    }
}
