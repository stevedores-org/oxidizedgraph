# Contributing to oxidizedgraph

This guide is a fast map for contributors who want to ship meaningful changes quickly.

## Local Setup

```bash
git clone aivcs://stevedores-org/oxidizedgraph.git
cd oxidizedgraph
cargo test
```

Optional checks:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features
```

## Start Contributing Map

### 1) Add or change graph semantics
- Primary files: `src/graph.rs`, `src/runner.rs`
- Typical tasks:
  - new edge behavior
  - transition resolution changes
  - execution loop safety/invariants
- Add/update tests in the corresponding `#[cfg(test)]` sections.

### 2) Add a built-in node type
- Primary files: `src/nodes/mod.rs`, `src/nodes/*.rs`
- Typical tasks:
  - new `NodeExecutor` implementation
  - routing/output behavior via `NodeOutput`
  - state mutation patterns
- Add tests in the node module and export from `src/nodes/mod.rs`.

### 3) Improve checkpointing and resume
- Primary files: `src/checkpoint/mod.rs`, `src/checkpoint/memory.rs`, `src/checkpoint/runner.rs`
- Typical tasks:
  - checkpoint model/schema changes
  - backend behavior
  - resume semantics and history management
- Validate with checkpoint runner tests and resume scenarios.

### 4) Improve streaming/events/observability
- Primary files: `src/events/types.rs`, `src/events/bus.rs`, `src/events/runner.rs`, `src/events/handler.rs`
- Typical tasks:
  - event schema additions
  - event delivery behavior
  - metrics/logging hooks
- Ensure event compatibility and update tests for handlers and bus behavior.

### 5) Extend orchestration (subgraphs/parallel)
- Primary files: `src/orchestration/subgraph_node.rs`, `src/orchestration/parallel.rs`, `src/orchestration/spawner.rs`
- Typical tasks:
  - join strategy behavior
  - result merging rules
  - dynamic spawning lifecycle
- Add deterministic tests for ordering and failure cases.

### 6) Improve API server behavior
- Primary file: `src/bin/server.rs`
- Typical tasks:
  - session lifecycle
  - execute/checkpoint/restore endpoints
  - API error handling and contracts
- Add endpoint tests as server behavior becomes productionized.

### 7) Work on docs/examples
- Primary files: `README.md`, `examples/*.rs`
- Keep examples runnable and aligned with current API.

## Contribution Workflow

1. Create a branch from `develop`.
2. Make focused changes (one topic per PR).
3. Add/update tests for behavior changes.
4. Run `cargo test` locally.
5. Open PR to `develop` with:
   - problem statement
   - design/approach
   - test evidence
   - follow-ups (if any)

## PR Review Expectations

- Behavior and correctness first.
- Backward-compatibility impact called out.
- Clear error semantics (no silent fallbacks for core paths).
- Tests cover new behavior and key failure modes.
