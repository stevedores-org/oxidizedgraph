# Upstream — stevedores-org/oxidizedgraph

The Lornu AI **Commercial Edition** (`lornu-ai/oxidizedgraph`, private) is
derived from the open-source project:

| | |
|---|---|
| **Upstream** | https://github.com/stevedores-org/oxidizedgraph |
| **OSS license** | Apache-2.0 — see [LICENSE-APACHE-UPSTREAM.md](LICENSE-APACHE-UPSTREAM.md) |
| **Public crate** | https://crates.io/crates/oxidizedgraph (community edition) |

## Edition split

| Edition | Repository | Visibility | Enterprise modules |
|---------|------------|------------|--------------------|
| Community (OSS) | `stevedores-org/oxidizedgraph` | Public | Roadmap / partial |
| Commercial | `lornu-ai/oxidizedgraph` | Private | `src/enterprise/*` — RBAC, tenancy, audit, SLO |

## Sync policy

1. **Bugfixes and core graph/runtime** — prefer landing in upstream first, then cherry-pick or merge into this repo.
2. **Enterprise-only features** — land here; contribute generic abstractions upstream when appropriate.
3. **Do not** push commercial-only code or customer-specific config to the public upstream repo.

## Provenance

Initial commercial import: `main` @ stevedores-org/oxidizedgraph (2026-06).
