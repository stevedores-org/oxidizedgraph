# oxidizedgraph — Commercial Enterprise Edition

Private distribution of oxidizedgraph for Lornu AI customers and internal
production workloads.

## Enterprise modules (`src/enterprise/`)

| Module | Purpose |
|--------|---------|
| `tenant` | Multi-tenant boundaries, RBAC subjects, roles, permissions |
| `secrets` | Scoped credentials, secret handles, log redaction |
| `audit` | Immutable audit log chain, compliance export |
| `slo` | SLO tracking, cost budgets, spend guardrails |
| `node` | Graph nodes: `TenantGuardNode`, `BudgetGuardNode`, `AuditExportNode`, … |

## Example

```bash
cargo run --example enterprise_workflow
```

See [docs/ROADMAP-18.md](ROADMAP-18.md) for the full enterprise readiness epic.

## Documentation (SSO)

| Edition | URL | Access |
|---------|-----|--------|
| **Commercial** | https://docs.oxidizedgraph.lornu.ai | Cloudflare Access — paid customers + `@lornu.ai` staff |
| **Community OSS** | https://docs.stevedores.org/oxidizedgraph/ | Public |

Deploy and SSO runbook: [RUNBOOK_COMMERCIAL_DOCS_SSO.md](RUNBOOK_COMMERCIAL_DOCS_SSO.md)

## Licensing

This repository is **proprietary** — see [LICENSE](../LICENSE). The community
Apache-2.0 edition remains at [stevedores-org/oxidizedgraph](https://github.com/stevedores-org/oxidizedgraph).
