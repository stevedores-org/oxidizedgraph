//! Kubernetes `batch/v1.Job` worker spawner (in-cluster).

use async_trait::async_trait;
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{
    Container, EnvVar, EnvVarSource, PodSpec, PodTemplateSpec, SecretKeySelector,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, PostParams};
use kube::Client;

use super::spec::{
    WorkerError, WorkerJobHandle, WorkerJobSpec, WorkerJobStatus, WorkerSpawnerConfig,
};
use super::WorkerSpawner;

/// Spawns ephemeral worker Jobs via the Kubernetes API (Workload Identity / in-cluster SA).
pub struct K8sWorkerSpawner {
    client: Client,
    config: WorkerSpawnerConfig,
}

impl K8sWorkerSpawner {
    /// Connect using in-cluster config or `KUBECONFIG`.
    pub async fn connect(config: WorkerSpawnerConfig) -> Result<Self, WorkerError> {
        let client = Client::try_default()
            .map_err(|e| WorkerError::Config(format!("kubernetes client: {e}")))?;
        Ok(Self { client, config })
    }

    /// Build a Job manifest from a worker spec.
    fn build_job(&self, spec: &WorkerJobSpec) -> Job {
        let image = spec
            .image
            .clone()
            .unwrap_or_else(|| self.config.worker_image.clone());

        let mut env = vec![
            EnvVar {
                name: "A2A_TASK_ID".into(),
                value: Some(spec.task_id.clone()),
                ..Default::default()
            },
            EnvVar {
                name: "A2A_CONTEXT_ID".into(),
                value: Some(spec.context_id.clone()),
                ..Default::default()
            },
            EnvVar {
                name: "A2A_INPUT".into(),
                value: Some(spec.input_text.clone()),
                ..Default::default()
            },
            EnvVar {
                name: "ORCHESTRATOR_URL".into(),
                value: Some(self.config.orchestrator_url.clone()),
                ..Default::default()
            },
            EnvVar {
                name: "RUST_LOG".into(),
                value: Some("info".into()),
                ..Default::default()
            },
        ];

        if let Some(secret) = &self.config.github_token_secret {
            env.push(EnvVar {
                name: "GITHUB_TOKEN".into(),
                value: None,
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: secret.clone(),
                        key: "token".into(),
                        optional: Some(false),
                    }),
                    ..Default::default()
                }),
            });
        }

        let labels = btreemap! {
            "app.kubernetes.io/name".to_string() => "oxidizedgraph-worker".to_string(),
            "app.kubernetes.io/part-of".to_string() => "stevedores".to_string(),
            "agent-build/task-id".to_string() => spec.task_id.clone(),
            "agent-build/managed-by".to_string() => "oxidizedgraph".to_string(),
        };

        Job {
            metadata: ObjectMeta {
                generate_name: Some("oxg-worker-".into()),
                namespace: Some(self.config.namespace.clone()),
                labels: Some(labels),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::batch::v1::JobSpec {
                ttl_seconds_after_finished: Some(self.config.ttl_seconds_after_finished),
                backoff_limit: Some(0),
                template: PodTemplateSpec {
                    spec: Some(PodSpec {
                        restart_policy: Some("Never".into()),
                        service_account_name: Some(self.config.service_account.clone()),
                        containers: vec![Container {
                            name: "worker".into(),
                            image: Some(image),
                            env: Some(env),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

#[async_trait]
impl WorkerSpawner for K8sWorkerSpawner {
    async fn spawn(&self, spec: WorkerJobSpec) -> Result<WorkerJobHandle, WorkerError> {
        let jobs: Api<Job> = Api::namespaced(self.client.clone(), &self.config.namespace);
        let job = self.build_job(&spec);
        let created = jobs
            .create(&PostParams::default(), &job)
            .await
            .map_err(|e| WorkerError::SpawnFailed(e.to_string()))?;

        let job_name = created
            .metadata
            .name
            .ok_or_else(|| WorkerError::SpawnFailed("Job created without name".into()))?;

        Ok(WorkerJobHandle {
            job_name,
            namespace: self.config.namespace.clone(),
        })
    }

    async fn status(&self, job_name: &str) -> Result<WorkerJobStatus, WorkerError> {
        let jobs: Api<Job> = Api::namespaced(self.client.clone(), &self.config.namespace);
        let job = match jobs.get(job_name).await {
            Ok(j) => j,
            Err(kube::Error::Api(e)) if e.code == 404 => {
                return Ok(WorkerJobStatus::NotFound);
            }
            Err(e) => return Err(WorkerError::StatusFailed(e.to_string())),
        };

        let status = job.status.unwrap_or_default();
        if status.succeeded.unwrap_or(0) > 0 {
            return Ok(WorkerJobStatus::Succeeded);
        }
        if status.failed.unwrap_or(0) > 0 {
            return Ok(WorkerJobStatus::Failed);
        }
        if status.active.unwrap_or(0) > 0 {
            return Ok(WorkerJobStatus::Running);
        }
        Ok(WorkerJobStatus::Pending)
    }
}

/// Helper macro for label maps (avoids extra dependency).
macro_rules! btreemap {
    ($($key:expr => $value:expr),+ $(,)?) => {{
        let mut map = std::collections::BTreeMap::new();
        $(map.insert($key, $value);)+
        map
    }};
}
use btreemap;
