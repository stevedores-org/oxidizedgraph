//! Planning and Long-Horizon Autonomy module.
//!
//! Provides goal decomposition (`EpicPlan`), dependency scheduling (`Scheduler`),
//! confidence/ETA tracking (`PlanProgress`), and integration nodes (`PlanningNode`, `SchedulerNode`).

mod node;
mod plan;
mod progress;
mod scheduler;

pub use node::{PlanningNode, SchedulerNode};
pub use plan::{EpicPlan, Task, TaskStatus};
pub use progress::PlanProgress;
pub use scheduler::Scheduler;
