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

## EKS / ECR (linux/amd64)

Clusters are **amd64**; build images on CI (`ubuntu-latest` Nix), not on Apple Silicon.

### CI publish (GitHub OIDC)

Long-lived `AWS_ACCESS_KEY_ID` in GitHub is **not** used. `publish-ecr` assumes an IAM role via **GitHub OIDC** (aligned with your ESO/Flux zero-secret posture).

| Repo variable | Purpose |
|---------------|---------|
| `AWS_OIDC_ROLE_ARN` | IAM role trusted by `token.actions.githubusercontent.com` for this repo |

**Trust-policy contract.** The IAM role specified by `AWS_OIDC_ROLE_ARN` must trust the GitHub OIDC provider for **audience** `sts.amazonaws.com` and **subject** matching `repo:stevedores-org/oxidizedgraph:*` (or scoped to a specific ref, e.g. `repo:stevedores-org/oxidizedgraph:ref:refs/heads/main`). The workflow declares the audience explicitly so a trust-policy reshuffle has the matching value at hand.

A missing `AWS_OIDC_ROLE_ARN` is a loud CI failure (the `publish-ecr-preflight` job exits non-zero on every push to `main`), not a silent skip.

Workflow env: `ECR_REGISTRY=148080843892.dkr.ecr.us-east-2.amazonaws.com`, `ECR_REPOSITORY=stevedores-org/oxidizedgraph/server`.

Tags on merge to `main`: `{Cargo version}`, `{version}-{git_sha}`, `latest`.

### Cluster (Flux + ESO)

Runtime credentials (ECR pull, env, GKE WI bindings) are delivered by **External Secrets Operator** and **Flux** in your platform layer — not static secrets in this app repo.

- **GKE**: WI annotation via Flux/ESO overlay (placeholder in `serviceaccount-wi.yaml` is smoke-only).
- **EKS**: IRSA or `imagePullSecrets` from ESO after CI pushes to ECR.

```bash
kubectl apply -k deploy/overlays/gke-autopilot
just deploy-eks
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
