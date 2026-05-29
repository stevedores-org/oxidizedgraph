# oxidizedgraph on OCI + GKE Autopilot

Package and run **oxidizedgraph-server** on Kubernetes using **Nix OCI images** and **Kustomize** (no Helm). The legacy `Dockerfile` remains for reference; prefer `flake.nix` + `dockworker.toml`.

## Build & push

```bash
nix develop   # optional: skopeo, just

nix build .#server-image -L
docker load -i result
docker image ls oxidizedgraph/server

# Or
just image
just push
# dockworker build   # all images in dockworker.toml
```

Image: `ghcr.io/stevedores-org/oxidizedgraph/server:latest`

## Deploy (GKE Autopilot)

```bash
# Edit deploy/base/serviceaccount.yaml — Workload Identity GCP SA email

kubectl apply -k deploy/overlays/gke-autopilot
kubectl -n oxidizedgraph get pods,svc
```

## API

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/health` | GET | Liveness (k8s probe) |
| `/readiness` | GET | Readiness (k8s probe) |
| `/api/v1/sessions` | POST | Create session |
| `/api/v1/sessions/{id}/execute` | POST | Run graph |

```bash
kubectl -n oxidizedgraph port-forward svc/oxidizedgraph 8080:8080

curl -s http://localhost:8080/health
curl -s -X POST http://localhost:8080/api/v1/sessions \
  -H 'Content-Type: application/json' \
  -d '{}'
```

## Environment

| Variable | Default | Notes |
|----------|---------|-------|
| `PORT` | `8080` | HTTP listen port |
| `RUST_LOG` | `info` | Tracing filter |

## data-fabric integration

When emitting graph events to [data-fabric](https://github.com/stevedores-org/data-fabric), point the fabric persister at this service’s session/execute API. See data-fabric `docs/INTEGRATION_OXIDIZEDGRAPH.md`.

## Autopilot constraints

- Resource requests/limits are required (set in `deployment.yaml`).
- No privileged pods or hostPath volumes.
- Use Workload Identity for pulling from private registries on GCR/GAR if needed.
