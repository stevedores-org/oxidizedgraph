//! Ephemeral worker Job orchestration for AKS (issue #41).
//!
//! The orchestrator spawns one `batch/v1.Job` per build task. Workers receive
//! task metadata via environment variables and report back over A2A JSON-RPC.

mod memory;
mod spec;

#[cfg(feature = "k8s")]
mod k8s;

pub use memory::MemoryWorkerSpawner;
pub use spec::{WorkerJobHandle, WorkerJobSpec, WorkerJobStatus, WorkerSpawnerConfig};

use async_trait::async_trait;

use crate::a2a::Task;

pub use spec::WorkerError;

/// Spawns and tracks ephemeral worker Jobs.
#[async_trait]
pub trait WorkerSpawner: Send + Sync {
    /// Create a worker Job for the given task spec.
    async fn spawn(&self, spec: WorkerJobSpec) -> Result<WorkerJobHandle, WorkerError>;

    /// Poll the Job status by name.
    async fn status(&self, job_name: &str) -> Result<WorkerJobStatus, WorkerError>;
}

/// Build the configured worker spawner from environment variables.
///
/// `WORKER_SPAWNER=memory` (default) or `k8s` (requires `k8s` feature + in-cluster config).
pub async fn spawner_from_env() -> Result<Box<dyn WorkerSpawner>, WorkerError> {
    let config = WorkerSpawnerConfig::from_env();
    match config.mode.as_str() {
        #[cfg(feature = "k8s")]
        "k8s" => Ok(Box::new(k8s::K8sWorkerSpawner::connect(config).await?)),
        _ => Ok(Box::new(MemoryWorkerSpawner::new(config))),
    }
}

/// Convenience wrapper used by the A2A layer when constructing specs.
impl WorkerJobSpec {
    /// Build a worker spec from an A2A task.
    pub fn from_task(task: &Task) -> Self {
        Self {
            task_id: task.id.clone(),
            context_id: task.context_id.clone(),
            input_text: task
                .input
                .as_ref()
                .and_then(|m| m.parts.first())
                .map(|p| p.text.clone())
                .unwrap_or_default(),
            ..WorkerJobSpec::default()
        }
    }
}
