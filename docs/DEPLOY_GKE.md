# oxidizedgraph on OCI + GKE Autopilot

Package and run **oxidizedgraph-server** on Kubernetes using **Nix OCI images** and **Kustomize** (no Helm). **No Dockerfile** — build with `flake.nix` and publish via `dockworker.toml` (see [PACKAGING.md](PACKAGING.md)).

## Build & push

```bash
nix develop   # skopeo, kubectl, kustomize, just

just image
just push
# Prod: IMAGE_TAG="$(git rev-parse --short HEAD)" just push
```

Default image: `ghcr.io/stevedores-org/oxidizedgraph/server:0.2.0` (semver, not `:latest`).

See [PACKAGING.md](PACKAGING.md) for the Stevedores Nix cache (opt-in) and tag strategy.

## Deploy (GKE Autopilot)

The **gke-autopilot** overlay is opinionated:

- **2 replicas** + **PodDisruptionBudget** (`minAvailable: 1`)
- **Guaranteed QoS** (`limits` = `requests`: 250m CPU, 512Mi memory)
- **`imagePullPolicy: Always`**
- **NetworkPolicy** default-deny ingress/egress with allowances for oxidizedgraph HTTP/DNS/HTTPS
- **Workload Identity** patch in overlay only (`deploy/overlays/gke-autopilot/serviceaccount-wi.yaml`)

```bash
# Edit overlay WI placeholder (do not patch base/):
#   deploy/overlays/gke-autopilot/serviceaccount-wi.yaml

# Optional: pin digest at deploy time
cd deploy/overlays/gke-autopilot
kustomize edit set image ghcr.io/stevedores-org/oxidizedgraph/server=ghcr.io/stevedores-org/oxidizedgraph/server@sha256:...

kubectl apply -k deploy/overlays/gke-autopilot
kubectl -n oxidizedgraph get pods,svc,pdb,networkpolicy
```

Local/minimal deploy without Autopilot hardening: `kubectl apply -k deploy/base`.

## EKS smoke (ECR)

For **linux/amd64** clusters (e.g. EKS), CI publishes Nix-built images on merge to `main` when repo secrets are set:

| Secret / var | Value |
|--------------|-------|
| `AWS_ACCESS_KEY_ID` | IAM user/role with `ecr:*` push |
| `AWS_SECRET_ACCESS_KEY` | matching secret |
| Repository variable `ECR_PUBLISH` | `true` (enables `publish-ecr` job on merge to main) |
| Workflow `ECR_REGISTRY` | `148080843892.dkr.ecr.us-east-2.amazonaws.com` |
| Workflow `ECR_REPOSITORY` | `stevedores-org/oxidizedgraph/server` |

Tags pushed: `{Cargo version}`, `{version}-{git_sha}`, `latest`.

```bash
# After merge + publish-ecr job
kubectl -n oxidizedgraph set image deployment/oxidizedgraph \
  oxidizedgraph=148080843892.dkr.ecr.us-east-2.amazonaws.com/stevedores-org/oxidizedgraph/server:0.2.0
```

If NetworkPolicy/PDB were previously applied into `default`, delete them and re-apply the overlay.

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

- Resource requests/limits are required; the overlay sets Guaranteed QoS.
- No privileged pods or hostPath volumes.
- NetworkPolicy may need extra ingress rules if callers live in other namespaces (edit `networkpolicy.yaml`).
