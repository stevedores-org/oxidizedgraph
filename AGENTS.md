# oxidizedgraph — Agent Capabilities

## OrchestratorAgent

The **OrchestratorAgent** is the core execution engine. It compiles and runs complex agent workflows defined as Directed Graphs.

### Capabilities

1. **Stateful Graph Execution**
   - Manages shared state across asynchronous node execution.
   - Enforces execution limits (recursion/iteration).
   - Supports conditional routing and dynamic edge selection.

2. **Node Specialization**
   - Supports LLM-based reasoning nodes.
   - Provides tool-execution nodes with type-safe schemas.
   - Integrates subgraph execution for modularity.

3. **Observability & Ledger**
   - Emits structured events for every node transition.
   - Supports AIVCS integration for content-addressed run recording.

## A2A Protocol

| Direction | Event | Purpose |
|-----------|-------|---------|
| Inbound | `WORKFLOW_START` | Initialize a new graph execution |
| Outbound | `NODE_TRANSITION` | Notify when a node finishes and the next starts |
| Outbound | `WORKFLOW_COMPLETE` | Notify when the END node is reached |
