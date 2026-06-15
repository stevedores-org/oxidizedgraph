//! In-memory worker spawner for local dev and tests.

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use uuid::Uuid;

use super::spec::{
    WorkerError, WorkerJobHandle, WorkerJobSpec, WorkerJobStatus, WorkerSpawnerConfig,
};
use super::WorkerSpawner;

/// Simulates worker Job spawning without a Kubernetes cluster.
#[derive(Debug, Default)]
pub struct MemoryWorkerSpawner {
    config: WorkerSpawnerConfig,
    jobs: RwLock<HashMap<String, WorkerJobStatus>>,
}

impl MemoryWorkerSpawner {
    /// Create a spawner with the given config.
    pub fn new(config: WorkerSpawnerConfig) -> Self {
        Self {
            config,
            jobs: RwLock::new(HashMap::new()),
        }
    }

    /// Create a spawner with default config (always succeeds immediately).
    pub fn default_spawner() -> Self {
        Self::new(WorkerSpawnerConfig::default())
    }
}

#[async_trait]
impl WorkerSpawner for MemoryWorkerSpawner {
    async fn spawn(&self, spec: WorkerJobSpec) -> Result<WorkerJobHandle, WorkerError> {
        let job_name = format!("oxg-worker-{}", &spec.task_id[..8.min(spec.task_id.len())]);
        let handle = WorkerJobHandle {
            job_name: job_name.clone(),
            namespace: self.config.namespace.clone(),
        };

        if let Ok(mut guard) = self.jobs.write() {
            guard.insert(job_name, WorkerJobStatus::Running);
        }

        Ok(handle)
    }

    async fn status(&self, job_name: &str) -> Result<WorkerJobStatus, WorkerError> {
        Ok(self
            .jobs
            .read()
            .ok()
            .and_then(|guard| guard.get(job_name).copied())
            .unwrap_or(WorkerJobStatus::NotFound))
    }
}

/// Mark a simulated job complete (test helper).
impl MemoryWorkerSpawner {
    /// Transition a simulated job to succeeded (tests only).
    pub fn complete_job(&self, job_name: &str) {
        if let Ok(mut guard) = self.jobs.write() {
            guard.insert(job_name.to_string(), WorkerJobStatus::Succeeded);
        }
    }

    /// Generate a unique job name prefix for tests.
    pub fn next_job_name(&self) -> String {
        format!("oxg-worker-{}", Uuid::new_v4())
    }
}
