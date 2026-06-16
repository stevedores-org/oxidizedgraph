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

The `deploy/overlays/gke-hub` overlay assumes the following infrastructure is already in place via [stevedores-org/crossplane-heaven](https://github.com/stevedores-org/crossplane-heaven) (`infrastructure/gke/hub/`):

- **Namespaces**: `Namespace/hub` (orchestrator) and `Namespace/workers` (Job spawning)
- **ServiceAccount**: `ServiceAccount/adk-agent-worker` in the `workers` namespace
- **Workload Identity binding**: GSA (`oxidizedgraph@PROJECT_ID.iam.gserviceaccount.com`) to KSA (`adk-agent-worker`/workers) via `google.iam.gke.io/gcp-service-account` annotation
- **External Secrets**: `ClusterSecretStore/gcp-secret-manager` in `external-secrets-system` (for secret projection to the deployment)

If deploying outside the `hub` cluster, substitute your own crossplane composition or create these resources manually before applying the overlay.

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
