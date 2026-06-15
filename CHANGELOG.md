# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - Unreleased

### Deprecated

- Generic `Node<S: State>` trait in `node` module — use `NodeExecutor` from `graph` module instead
- Generic `Edge<S>` enum in `edge` module — use `EdgeType` and `GraphEdge` from `graph` module instead
- `Router<S>` trait in `edge` module — use conditional edges in `GraphBuilder` instead
- `FnNode<S>`, `WrappedNode<S>`, `BoxedNode<S>` — use `FunctionNode` or implement `NodeExecutor` directly

### Added

- Decision memory (`memory::decision`) — queryable log of *why* changes were
  made, keyed by run and artifact. `DecisionStore` trait, in-memory backend,
  and SurrealDB backend behind the `persistence` feature. First slice of
  EPIC5 (issue #23).
- Integration tests for complex graph patterns (`tests/graph_patterns.rs`)
  - Multi-branch conditional routing
  - Cycle with exit condition
  - Recursion limit enforcement
  - Error propagation through runner
  - Deep linear graph (50 nodes)
  - Concurrent state mutation
  - Conditional edge routing
  - Node finish semantics
  - Message manipulation
- ADR-001: Architectural decision to consolidate on single implementation
- Migration guide for deprecated APIs (`docs/MIGRATION-0.2.md`)
- This changelog

### Fixed

- `FnNode<S>` Sync bound on `Fut` generic parameter (in deprecated `node` module)

### Notes

Deprecated APIs will be removed in v0.3.0 (planned 2-3 months after v0.2.0 release).
The `runtime.rs` file is dead code (not in module tree) and will also be deleted in v0.3.0.
See `docs/MIGRATION-0.2.md` for migration guide and `docs/ADR-001-single-implementation.md` for rationale.

## [0.1.1] - 2024

### Added

- Initial release with `NodeExecutor`, `GraphBuilder`, `GraphRunner`
- Built-in nodes: Echo, Delay, Conditional, Function, LLM, Tool
- `AgentState` with `SharedState` (`Arc<RwLock>`) thread-safe state management
- petgraph-based graph structure
- Mermaid diagram generation
- Git integration module
