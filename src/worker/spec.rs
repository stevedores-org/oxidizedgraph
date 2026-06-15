//! Worker Job specification and status types.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from worker Job lifecycle operations.
#[derive(Debug, Error)]
pub enum WorkerError {
    /// Kubernetes or runtime failure.
    #[error("worker spawn failed: {0}")]
    SpawnFailed(String),
    /// Status lookup failed.
    #[error("worker status failed: {0}")]
    StatusFailed(String),
    /// Invalid configuration.
    #[error("worker config invalid: {0}")]
    Config(String),
}

/// Runtime configuration for worker spawning.
#[derive(Debug, Clone)]
pub struct WorkerSpawnerConfig {
    /// Spawner mode: `memory` or `k8s`.
    pub mode: String,
    /// Kubernetes namespace for worker Jobs.
    pub namespace: String,
    /// Container image for worker Jobs.
    pub worker_image: String,
    /// ServiceAccount name for worker pods.
    pub service_account: String,
    /// TTL after Job completion (seconds).
    pub ttl_seconds_after_finished: i32,
    /// Orchestrator callback base URL injected into worker env.
    pub orchestrator_url: String,
    /// Optional GitHub token secret name (projected by crossplane-heaven #6).
    pub github_token_secret: Option<String>,
}

impl Default for WorkerSpawnerConfig {
    fn default() -> Self {
        Self {
            mode: "memory".into(),
            namespace: "oxidizedgraph".into(),
            worker_image: "ghcr.io/stevedores-org/oxidizedgraph/server:0.2.0".into(),
            service_account: "oxidizedgraph-worker".into(),
            ttl_seconds_after_finished: 3600,
            orchestrator_url: "http://oxidizedgraph:8080".into(),
            github_token_secret: Some("github-app-token".into()),
        }
    }
}

impl WorkerSpawnerConfig {
    /// Load config from environment variables with sensible defaults.
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("WORKER_SPAWNER") {
            cfg.mode = v;
        }
        if let Ok(v) = std::env::var("WORKER_NAMESPACE") {
            cfg.namespace = v;
        }
        if let Ok(v) = std::env::var("WORKER_IMAGE") {
            cfg.worker_image = v;
        }
        if let Ok(v) = std::env::var("WORKER_SERVICE_ACCOUNT") {
            cfg.service_account = v;
        }
        if let Ok(v) = std::env::var("WORKER_TTL_SECONDS") {
            if let Ok(n) = v.parse() {
                cfg.ttl_seconds_after_finished = n;
            }
        }
        if let Ok(v) = std::env::var("ORCHESTRATOR_URL") {
            cfg.orchestrator_url = v;
        }
        if let Ok(v) = std::env::var("GITHUB_TOKEN_SECRET") {
            cfg.github_token_secret = if v.is_empty() { None } else { Some(v) };
        }
        cfg
    }
}

/// Request to spawn an ephemeral worker Job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkerJobSpec {
    /// A2A task id.
    pub task_id: String,
    /// A2A context id.
    pub context_id: String,
    /// Plain-text task input for the worker.
    pub input_text: String,
    /// Optional image override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Handle returned after a successful spawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerJobHandle {
    /// Kubernetes Job name.
    pub job_name: String,
    /// Namespace the Job was created in.
    pub namespace: String,
}

/// Observed worker Job status mapped to A2A task states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerJobStatus {
    /// Job created but no pod running yet.
    Pending,
    /// At least one pod is active.
    Running,
    /// Job completed successfully.
    Succeeded,
    /// Job failed.
    Failed,
    /// Job no longer exists.
    NotFound,
}
