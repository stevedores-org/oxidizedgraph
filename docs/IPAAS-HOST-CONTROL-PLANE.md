# ADR: iPaaS Host Control Plane (out of repo)

**Status:** Accepted (locked)

## Context

Gap / OSC needs a production iPaaS control path: XYFlow persistence, webhooks, OAuth refresh, AES-256-GCM vault, Postgres `SKIP LOCKED` jobs, and a warm Bun connector pool. That surface must not be folded into the graph runtime crate.

## Decision

**oxidizedgraph remains the Execution Plane only** — graph run, Tokio branch parallelism, `conditional_edge`, checkpoints, A2A JSON-RPC worker dispatch.

**axum Host Control Plane + Bun UDS pool live in Orb Stack Cloud**, not in this repository:

- Architecture: [stevedores-org/orb-stack-cloud `docs/OXIDIZEDGRAPH-IPAAS-HOST.md`](https://github.com/stevedores-org/orb-stack-cloud/blob/feat/gap-on-orbstack/docs/OXIDIZEDGRAPH-IPAAS-HOST.md)
- Scaffold: [`blueprint/ipaas-host/`](https://github.com/stevedores-org/orb-stack-cloud/tree/feat/gap-on-orbstack/blueprint/ipaas-host)
- Linear: [LOR-597](https://linear.app/lornu-ai/issue/LOR-597)

## Consequences

- Do **not** add axum Host, vault, or Bun supervisor code to oxidizedgraph.
- Do **not** greenfield a petgraph state machine; consumers compile `WorkflowBlueprint` → oxidizedgraph `NodeExecutor` bindings from the OSC Host.
- Bun uses **UDS Option 1** (warm pool); reject in-process `bun:ffi`.
- A2A Jobs ([WORKER_ORCHESTRATION.md](WORKER_ORCHESTRATION.md)) stay complementary to the Bun UDS pool — not a substitute.
