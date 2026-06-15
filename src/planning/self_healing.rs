use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Taxonomy of task execution failures.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// Code syntax or compiler type-checking errors.
    Compile,
    /// Test assertions or test suite run failures.
    Test,
    /// Uncaught exceptions, panics, memory faults at runtime.
    Runtime,
    /// Downstream API, network, database connection errors.
    Integration,
    /// Catch-all for uncategorized errors.
    Unknown,
}

impl FailureClass {
    /// String representation of the failure class.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Compile => "compile",
            Self::Test => "test",
            Self::Runtime => "runtime",
            Self::Integration => "integration",
            Self::Unknown => "unknown",
        }
    }
}

/// Configuration for retry safety limits per failure class.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum number of recovery attempts before halting.
    pub max_attempts: usize,
    /// Backoff factor (unused in simple executor but good for metadata).
    pub backoff_factor: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_factor: 1,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy.
    pub fn new(max_attempts: usize) -> Self {
        Self {
            max_attempts,
            backoff_factor: 1,
        }
    }

    /// Default policy tuned for compilation errors.
    pub fn compile_default() -> Self {
        Self {
            max_attempts: 3,
            backoff_factor: 1,
        }
    }

    /// Default policy tuned for test failures.
    pub fn test_default() -> Self {
        Self {
            max_attempts: 2,
            backoff_factor: 2,
        }
    }

    /// Default policy tuned for runtime exceptions.
    pub fn runtime_default() -> Self {
        Self {
            max_attempts: 2,
            backoff_factor: 2,
        }
    }

    /// Default policy tuned for integration/network/API issues.
    pub fn integration_default() -> Self {
        Self {
            max_attempts: 4,
            backoff_factor: 3,
        }
    }
}

/// An audit log record for a recovery action taken during execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryRecord {
    /// The ID of the failed task.
    pub task_id: String,
    /// The attempt number (1-indexed).
    pub attempt: usize,
    /// Classified category of the failure.
    pub failure_class: FailureClass,
    /// The error message that triggered the recovery.
    pub error: String,
    /// The action taken (e.g. "Inject recovery task", "Halted (max attempts)").
    pub decision: String,
    /// Rationale behind the decision.
    pub rationale: String,
    /// Timestamp when this decision was recorded.
    pub timestamp: DateTime<Utc>,
}

impl RecoveryRecord {
    /// Create a new recovery record.
    pub fn new(
        task_id: impl Into<String>,
        attempt: usize,
        failure_class: FailureClass,
        error: impl Into<String>,
        decision: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            attempt,
            failure_class,
            error: error.into(),
            decision: decision.into(),
            rationale: rationale.into(),
            timestamp: Utc::now(),
        }
    }
}

/// Helper function to classify an error string into a FailureClass.
pub fn classify_failure(error_msg: &str) -> FailureClass {
    let err_lower = error_msg.to_lowercase();
    if err_lower.contains("compile")
        || err_lower.contains("compilation")
        || err_lower.contains("cargo build")
        || err_lower.contains("rustc")
        || err_lower.contains("syntax error")
    {
        FailureClass::Compile
    } else if err_lower.contains("test failed")
        || err_lower.contains("cargo test")
        || err_lower.contains("assertion failed")
        || err_lower.contains("panic at")
        || err_lower.contains("test failure")
    {
        FailureClass::Test
    } else if err_lower.contains("connection refused")
        || err_lower.contains("api gateway")
        || err_lower.contains("integration")
        || err_lower.contains("webhook")
        || err_lower.contains("network error")
        || err_lower.contains("timeout")
    {
        FailureClass::Integration
    } else if err_lower.contains("runtime error")
        || err_lower.contains("panicked")
        || err_lower.contains("null pointer")
        || err_lower.contains("segmentation fault")
        || err_lower.contains("index out of bounds")
    {
        FailureClass::Runtime
    } else {
        FailureClass::Unknown
    }
}
