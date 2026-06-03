//! Code quality guardrails for autonomous agent workflows.

mod findings;
mod gate;
mod risk;

pub use findings::{FindingSeverity, GateCheck, GateResult, ReviewFinding};
pub use gate::{CommandRunner, CommandSpec, MockCommandRunner, QualityGateConfig, QualityGateNode};
pub use risk::{ChangeRisk, MergeBlocker, RiskClassifier, RiskLevel};
