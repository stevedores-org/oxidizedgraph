# ADR 001: Consolidate on NodeExecutor + AgentState Implementation

## Status

Accepted

## Context

oxidizedgraph evolved with two parallel implementations:

1. **Generic traits** (`node.rs`, `edge.rs`): `Node<S: State>`, `Edge<S>`, `Router<S>` with parameterized state. An incomplete exploratory approach — the corresponding `Runtime<S>` in `runtime.rs` references a generic `CompiledGraph<S>` that no longer exists.

2. **Concrete types** (`graph.rs`, `runner.rs`): `NodeExecutor`, `EdgeType`, `GraphBuilder`, `GraphRunner` with `AgentState` and `SharedState` (`Arc<RwLock<AgentState>>`). This is the production implementation used by all examples, built-in nodes, and the execution runtime.

The generic files (`node.rs`, `edge.rs`, `runtime.rs`) were never wired into `lib.rs` and sat as orphaned dead code. All downstream consumers use the concrete implementation.

## Decision

1. **v0.2.0**: Re-add `node` and `edge` modules to `lib.rs` with `#[deprecated]` attributes. Leave `runtime.rs` out of the module tree (it won't compile against current `CompiledGraph`). Document the planned removal.

2. **v0.3.0**: Delete `node.rs`, `edge.rs`, `runtime.rs`. Remove module declarations from `lib.rs`.

### Rationale

- Concrete types are simpler and more ergonomic for the LangGraph use case.
- `AgentState` with `SharedState` (`Arc<RwLock>`) provides the right level of abstraction.
- Generic approach adds complexity without clear benefits — it was never completed.
- Enables cleaner integration with the stevedores-org ecosystem (llama.rs, oxidizedMLX, oxidizedRAG).

## Consequences

- **Positive**: Simpler codebase, clearer API surface, easier ecosystem integration.
- **Negative**: Breaking change for anyone using generic traits (extremely unlikely since they were never exported).
- **Mitigation**: Deprecation period with compiler warnings, migration guide, semver compliance.

## Integration Benefits

Concrete `AgentState` + `NodeExecutor` enables direct integration with:

- **llama.rs**: LLM inference backend for `LLMNode`
- **oxidizedMLX**: Apple Silicon acceleration for model execution
- **oxidizedRAG**: Knowledge retrieval for agent context
- **local-ci**: Test workflow graphs as part of CI
- **aivcs**: Version control for graph definitions and state
