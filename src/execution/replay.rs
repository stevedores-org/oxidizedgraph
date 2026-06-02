//! Replay validation against recorded transition logs.

use crate::graph::NodeOutput;

use super::transition::TransitionLog;

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

/// Stub output for replay tooling from recorded kind labels.
#[allow(dead_code)]
pub fn output_from_kind(kind: &str) -> NodeOutput {
    match kind {
        "finish" => NodeOutput::finish(),
        "continue_to" => NodeOutput::cont(),
        "route" => NodeOutput::cont(),
        "transition" => NodeOutput::transition("replay"),
        _ => NodeOutput::cont(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::transition::TransitionRecord;

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
