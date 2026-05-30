# Sovereign Intelligence Standards — oxidizedgraph

## 1. Architectural Integrity
- Maintain the graph-based execution model (Pillar 5).
- Ensure all node transitions are tracked and logged.
- The `SharedState` must remain the single source of truth for workflow data.

## 2. Orchestration Safety
- Recursion limits must be enforced by the `Runtime`.
- Node failures must be categorized (Retryable vs. Fatal).
- Subgraph integration must preserve parent state context.

## 3. Operational Excellence
- `cargo test` is the primary health check.
- Documentation must follow the 7-File Rule.
- AIVCS integration is required for production audit trails.
