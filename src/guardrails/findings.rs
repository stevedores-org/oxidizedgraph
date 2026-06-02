//! Machine-readable review findings and gate results.

use serde::{Deserialize, Serialize};

/// Severity of a review or gate finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum FindingSeverity {
    /// Informational only
    Info,
    /// Warning — should fix but not blocking by default
    Warning,
    /// Error — blocks merge when gate is required
    Error,
}

/// A single review finding with file/line reference when available.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReviewFinding {
    /// Stable rule or check identifier
    pub rule_id: String,
    /// Human-readable message
    pub message: String,
    /// Severity
    pub severity: FindingSeverity,
    /// File path relative to repo root
    pub file: Option<String>,
    /// 1-based line number
    pub line: Option<u32>,
    /// Column when known
    pub column: Option<u32>,
}

impl ReviewFinding {
    /// Create an error-level finding.
    pub fn error(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            message: message.into(),
            severity: FindingSeverity::Error,
            file: None,
            line: None,
            column: None,
        }
    }

    /// Attach file/line location.
    pub fn at(mut self, file: impl Into<String>, line: u32) -> Self {
        self.file = Some(file.into());
        self.line = Some(line);
        self
    }
}

/// Result of a single quality check (e.g. clippy, test).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GateCheck {
    /// Check name (e.g. "clippy", "test")
    pub name: String,
    /// Whether the check passed
    pub passed: bool,
    /// Exit code from the underlying command
    pub exit_code: Option<i32>,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Stdout/stderr excerpt for diagnostics
    pub log_excerpt: String,
    /// Findings produced by this check
    pub findings: Vec<ReviewFinding>,
}

/// Aggregate gate result stored on agent state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GateResult {
    /// Whether all required checks passed
    pub passed: bool,
    /// Individual check results
    pub checks: Vec<GateCheck>,
    /// All findings across checks
    pub findings: Vec<ReviewFinding>,
    /// Whether merge should be blocked
    pub merge_blocked: bool,
}

impl GateResult {
    /// Create a passing gate result.
    pub fn pass(checks: Vec<GateCheck>) -> Self {
        Self {
            passed: true,
            checks,
            findings: Vec::new(),
            merge_blocked: false,
        }
    }

    /// Create a failing gate result with findings.
    pub fn fail(checks: Vec<GateCheck>, findings: Vec<ReviewFinding>) -> Self {
        let has_errors = findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Error);
        Self {
            passed: false,
            checks,
            findings,
            merge_blocked: has_errors,
        }
    }

    /// Count findings by severity.
    pub fn count_errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::Error)
            .count()
    }
}
