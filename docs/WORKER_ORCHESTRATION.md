# Worker Job orchestration (issue #41)

oxidizedgraph runs as a long-lived Deployment on **stevedores-org GKE** (`hub`
namespace). Build tasks arrive over **A2A JSON-RPC** at `POST /rpc`; the
orchestrator spawns one ephemeral `batch/v1.Job` per task in the `workers`
namespace.

Production GitOps for hub + workers lives in
[stevedores-org/crossplane-heaven](https://github.com/stevedores-org/crossplane-heaven)
(`infrastructure/gke/hub/`). This repo ships a matching kustomize overlay for
local smoke tests.

## Prerequisites

The `deploy/overlays/gke-hub` overlay assumes the following infrastructure is already in place:

### Required Resources

- **Namespaces**: `Namespace/hub` (orchestrator) and `Namespace/workers` (Job spawning)
- **ServiceAccount**: `ServiceAccount/adk-agent-worker` in the `workers` namespace
- **Workload Identity binding**: GSA (`adk-agent-worker@PROJECT_ID.iam.gserviceaccount.com`) to KSA (`adk-agent-worker`/workers)
  - The overlay **requires** you to manually add the `google.iam.gke.io/gcp-service-account` annotation to the orchestrator Deployment's Pod spec
  - Example: `.spec.template.metadata.annotations["google.iam.gke.io/gcp-service-account"] = "adk-agent-worker@PROJECT_ID.iam.gserviceaccount.com"`

### Optional: Secret Projection

If using External Secrets for automatic secret projection:
- **ClusterSecretStore**: `ClusterSecretStore/gcp-secret-manager` in `external-secrets-system`
- Create an `ExternalSecret` resource to project the `github-app-token` Secret into the orchestrator Deployment
- If not using External Secrets, manually project the Secret via `envFrom` or volume mounts

### For crossplane-heaven users

These resources can be generated via [stevedores-org/crossplane-heaven](https://github.com/stevedores-org/crossplane-heaven) (crossplane compositions in `infrastructure/gke/`). Otherwise, create these resources manually before applying the overlay.

## Runtime flow

1. Client calls `SendMessage` with a user message describing the build task.
2. Orchestrator creates an A2A task in `TASK_STATE_SUBMITTED`.
3. Orchestrator creates a Kubernetes Job (or in-memory simulation locally).
4. Task transitions to `TASK_STATE_WORKING` with the Job name attached.
5. Client polls `GetTask` until the Job completes (`TASK_STATE_COMPLETED`).

## Agent discovery

- Agent Card: `GET /.well-known/agent-card.json`
- JSON-RPC: `POST /rpc`

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `WORKER_SPAWNER` | `memory` | `memory` for dev/tests, `k8s` in-cluster |
| `WORKER_NAMESPACE` | `oxidizedgraph` | Namespace for worker Jobs (overridden to `workers` in gke-hub overlay) |
| `WORKER_IMAGE` | crate server image | Worker container image |
| `WORKER_SERVICE_ACCOUNT` | `oxidizedgraph-worker` | SA for worker pods (overridden to `adk-agent-worker` in gke-hub overlay) |
| `WORKER_TTL_SECONDS` | `3600` | Job TTL after finish |
| `ORCHESTRATOR_URL` | `http://oxidizedgraph:8080` | Callback URL for workers |
| `ORCHESTRATOR_PUBLIC_URL` | same as bind URL | Used in Agent Card |
| `GITHUB_TOKEN_SECRET` | `github-app-token` | Secret projected into workers |

## Deploy

```bash
# Dev — memory spawner (no cluster Job API needed)
kustomize build deploy/base | kubectl apply -f -

# stevedores-org GKE hub — k8s spawner + GKE Workload Identity placeholder
kustomize build deploy/overlays/gke-hub | kubectl apply -f -

# GKE Autopilot (oxidizedgraph namespace, memory spawner)
kustomize build deploy/overlays/gke-autopilot | kubectl apply -f -
```

Worker Job shape is documented in `deploy/base/worker-job-template.yaml`.

## Example JSON-RPC

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "SendMessage",
  "params": {
    "message": {
      "role": "ROLE_USER",
      "parts": [{"text": "Build feature branch foo"}],
      "messageId": "msg-1"
    },
    "configuration": { "returnImmediately": true }
  }
}
```
