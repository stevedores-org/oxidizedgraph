//! Replay validation against recorded transition logs.

use crate::graph::NodeOutput;

use super::transition::{TransitionLog, TransitionRecord};

/// Report from comparing an expected trace to an actual trace.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayReport {
    /// Whether traces match
    pub matches: bool,
    /// Human-readable mismatches
    pub mismatches: Vec<String>,
}

impl ReplayReport {
    /// Whether replay validation passed.
    pub fn is_ok(&self) -> bool {
        self.matches
    }
}

/// Validates execution traces for deterministic replay auditing.
#[derive(Clone, Debug, Default)]
pub struct ReplayRunner;

impl ReplayRunner {
    /// Create a replay validator.
    pub fn new() -> Self {
        Self
    }

    /// Compare two transition logs node-by-node.
    pub fn compare(&self, expected: &TransitionLog, actual: &TransitionLog) -> ReplayReport {
        let mut mismatches = Vec::new();

        if expected.len() != actual.len() {
            mismatches.push(format!(
                "length mismatch: expected {}, got {}",
                expected.len(),
                actual.len()
            ));
        }

        for (i, (exp, act)) in expected
            .records()
            .iter()
            .zip(actual.records().iter())
            .enumerate()
        {
            if exp.node_id != act.node_id {
                mismatches.push(format!(
                    "step {i}: node_id expected '{}', got '{}'",
                    exp.node_id, act.node_id
                ));
            }
            if exp.output_kind != act.output_kind {
                mismatches.push(format!(
                    "step {i}: output_kind expected '{}', got '{}'",
                    exp.output_kind, act.output_kind
                ));
            }
            if exp.next_node != act.next_node {
                mismatches.push(format!(
                    "step {i}: next_node expected {:?}, got {:?}",
                    exp.next_node, act.next_node
                ));
            }
        }

        ReplayReport {
            matches: mismatches.is_empty(),
            mismatches,
        }
    }

    /// Check whether a log would route the same way for a given output kind sequence.
    pub fn output_sequence(log: &TransitionLog) -> Vec<&str> {
        log.records()
            .iter()
            .map(|r| r.output_kind.as_str())
            .collect()
    }
}

/// Reconstruct node output from a recorded transition.
pub fn output_from_record(record: &TransitionRecord) -> NodeOutput {
    match record.output_kind.as_str() {
        "finish" => NodeOutput::finish(),
        "continue" => NodeOutput::cont(),
        "continue_to" => record
            .next_node
            .as_ref()
            .map(|node| NodeOutput::continue_to(node.clone()))
            .unwrap_or_else(NodeOutput::cont),
        "route" => record
            .output_target
            .as_ref()
            .or(record.next_node.as_ref())
            .map(|target| NodeOutput::route(target.clone()))
            .unwrap_or_else(NodeOutput::cont),
        "transition" => record
            .output_target
            .as_ref()
            .map(|key| NodeOutput::transition(key.clone()))
            .unwrap_or_else(|| NodeOutput::transition("unknown")),
        _ => NodeOutput::cont(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::transition::TransitionRecord;
    use super::*;

    #[test]
    fn test_output_from_record_preserves_transition_key() {
        let record = TransitionRecord {
            run_id: "r".to_string(),
            iteration: 0,
            node_id: "gate".to_string(),
            output_kind: "transition".to_string(),
            next_node: Some("ship".to_string()),
            output_target: Some("passed".to_string()),
            state_iteration: 1,
            recorded_at: chrono::Utc::now(),
        };

        let output = output_from_record(&record);
        assert_eq!(output.target(), Some("passed"));
    }

    #[test]
    fn test_replay_compare_matching_logs() {
        let mut a = TransitionLog::new();
        let mut b = TransitionLog::new();
        let record = TransitionRecord::from_step(
            "r1",
            0,
            "gate",
            &NodeOutput::transition("passed"),
            Some("ship"),
            1,
        );
        a.push(record.clone());
        b.push(record);

        let report = ReplayRunner::new().compare(&a, &b);
        assert!(report.is_ok());
    }

    #[test]
    fn test_replay_detects_mismatch() {
        let mut expected = TransitionLog::new();
        let mut actual = TransitionLog::new();
        expected.push(TransitionRecord::from_step(
            "r",
            0,
            "a",
            &NodeOutput::cont(),
            None,
            0,
        ));
        actual.push(TransitionRecord::from_step(
            "r",
            0,
            "b",
            &NodeOutput::cont(),
            None,
            0,
        ));

        let report = ReplayRunner::new().compare(&expected, &actual);
        assert!(!report.is_ok());
        assert!(!report.mismatches.is_empty());
    }
}
