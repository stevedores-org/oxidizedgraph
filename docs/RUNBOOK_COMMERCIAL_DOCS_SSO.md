# Runbook: Commercial docs SSO (`docs.oxidizedgraph.lornu.ai`)

Paid **Lornu AI Commercial Edition** documentation. Browser access requires
**Cloudflare Zero Trust SSO** — no anonymous public read.

| | |
|---|---|
| **Public URL** | https://docs.oxidizedgraph.lornu.ai |
| **Edition** | `lornu-ai/oxidizedgraph` (private) |
| **OSS docs (free)** | https://docs.stevedores.org/oxidizedgraph/ |
| **Access config** | `deploy/cloudflare-access/configmap.yaml` |
| **K8s manifests** | `deploy/docs/` |

## Architecture

```text
Browser → Cloudflare (DNS proxy) → Access SSO gate → GKE Ingress → oxidizedgraph-docs Service
                ↑
         Google / GitHub IdP
         + staff @lornu.ai
         + paid customer Access Group
```

- **Origin** (GKE nginx serving static Vite build) is not intended for direct public access.
- **Probes** use cluster-internal Service URLs — no Access bypass path on the public hostname.

## Prerequisites

- [ ] `lornu.ai` zone in Cloudflare (orange-cloud DNS)
- [ ] Zero Trust IdPs: **Google** (Workspace) and/or **GitHub** org SSO configured
- [ ] Account API token: `Access: Apps and Policies → Edit`
- [ ] GAR image `gcp-lornu-ai/lornu-ai/oxidizedgraph-docs` (CI: `.github/workflows/docs-publish.yml`)
- [ ] Flux path in `lornu-ai/infra-code` pointing at `deploy/docs/overlays/gke-prod` (follow-up PR)

## Step 1 — Build and publish docs image

On merge to `main` (when WIF vars are set):

```bash
# CI publishes to:
# us-central1-docker.pkg.dev/gcp-lornu-ai/lornu-ai/oxidizedgraph-docs:latest
```

Local smoke build:

```bash
cd docs-site
bun install --frozen-lockfile
bun run build
# OCI: see docs-site/dockworker.toml — dockworker build && dockworker push
```

## Step 2 — Deploy Kubernetes (Flux)

Add to `infra-code` (example):

```yaml
# flux/kustomizations/ks-oxidizedgraph-docs-gke-prod.yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: oxidizedgraph-docs-gke-prod
  namespace: flux-system
spec:
  interval: 10m
  sourceRef:
    kind: GitRepository
    name: oxidizedgraph-commercial
  path: ./deploy/docs/overlays/gke-prod
  prune: true
  wait: true
```

Reconcile:

```bash
flux reconcile kustomization oxidizedgraph-docs-gke-prod -n flux-system --with-source
kubectl -n oxidizedgraph-docs get ingress,deploy,pods
```

## Step 3 — DNS

Ensure `docs.oxidizedgraph.lornu.ai` is proxied through Cloudflare to the GKE ingress LB.
Use Crossplane/external-dns (preferred) or a manual CNAME to the nginx LB.

## Step 4 — Cloudflare Access (SSO)

Apply desired-state ConfigMap (via Flux or manually):

```bash
kubectl apply -k deploy/cloudflare-access/
```

Provision policies:

```bash
export CF_API_TOKEN=<account-api-token>
./scripts/provision-cloudflare-docs-access.sh
```

Verify unauthenticated users are blocked:

```bash
./scripts/provision-cloudflare-docs-access.sh --verify-only
# Expect HTTP 403 or 302 to IdP — NOT 200 with doc HTML
```

### Customer onboarding (paid seats)

1. Create **Zero Trust → Access → Groups** e.g. `oxidizedgraph-customers-2026`
2. Add customer emails or IdP group rules
3. Set `customer_access_group_id` in `deploy/cloudflare-access/configmap.yaml`
4. Re-run provision script (or manage policies in dashboard)

Until the group exists, add comma-separated emails to `customer_emails` for pilot customers.

## Step 5 — GitHub repo About

Repo homepage should point at the SSO docs URL (already set via `gh repo edit --homepage`).

## Troubleshooting

| Symptom | Check |
|---------|--------|
| 200 without login | Access app missing or DNS bypasses Cloudflare |
| 403 forever after login | User not in staff domain or customer group |
| 502/503 | `kubectl -n oxidizedgraph-docs get pods,ingress` — image pull / ingress |
| Assets 404 | Rebuild docs with `VITE_DOCS_BASE=/` (commercial root host) |

## Related

- [ENTERPRISE.md](ENTERPRISE.md) — enterprise modules in the crate
- [UPSTREAM.md](../UPSTREAM.md) — OSS vs commercial split
- [infra-code RUNBOOK_CLOUDFLARE_MCP_ACCESS_AIVCS_IO.md](https://github.com/lornu-ai/infra-code) — similar Access pattern (service token; docs use interactive SSO only)
