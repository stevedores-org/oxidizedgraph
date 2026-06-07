//! Deterministic execution core for autonomous agent workflows.
//!
//! Provides run correlation (`RunContext`), append-only transition logs,
//! traced execution (`TracedRunner`), and replay validation.

mod replay;
mod traced_runner;
mod transition;
mod validation;

pub use replay::{output_from_record, ReplayReport, ReplayRunner};
pub use traced_runner::{TracedRunResult, TracedRunner};
pub use transition::{RunContext, TransitionLog, TransitionRecord};
pub use validation::{StateValidator, ValidationError};
