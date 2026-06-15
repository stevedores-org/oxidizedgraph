# Worker Job orchestration (issue #41)

oxidizedgraph runs as a long-lived Deployment on the AKS hub. Build tasks arrive
over **A2A JSON-RPC** at `POST /rpc`; the orchestrator spawns one ephemeral
`batch/v1.Job` per task.

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
| `WORKER_NAMESPACE` | `oxidizedgraph` | Namespace for worker Jobs |
| `WORKER_IMAGE` | crate server image | Worker container image |
| `WORKER_SERVICE_ACCOUNT` | `oxidizedgraph-worker` | SA for worker pods |
| `WORKER_TTL_SECONDS` | `3600` | Job TTL after finish |
| `ORCHESTRATOR_URL` | `http://oxidizedgraph:8080` | Callback URL for workers |
| `ORCHESTRATOR_PUBLIC_URL` | same as bind URL | Used in Agent Card |
| `GITHUB_TOKEN_SECRET` | `github-app-token` | Secret projected into workers |

## Deploy

```bash
# Dev / GKE / EKS — memory spawner (no cluster Job API needed)
kustomize build deploy/base | kubectl apply -f -

# AKS hub — enables k8s spawner + Azure WI annotation placeholder
kustomize build deploy/overlays/aks-hub | kubectl apply -f -
```

Worker Job shape is documented in `deploy/base/worker-job-template.yaml`.
Crossplane templates and Workload Identity wiring live in
[stevedores-org/crossplane-heaven](https://github.com/stevedores-org/crossplane-heaven)
(issues #4, #5).

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
