# oxidizedgraph — Gemini CLI Instructions

## Project Context

**oxidizedgraph** is a high-performance agent orchestration framework in Rust, providing a humble attempt at LangGraph. It uses a graph-based model for defining complex agent workflows with type-safe state and asynchronous execution.

## Core Mandates

1. **Safety First**: Use `RwLock` for shared state management to prevent race conditions during node execution.
2. **Determinism**: Ensure node execution order is governed by the graph edges and transitions.
3. **Traceability**: All node transitions and state updates must be logged and, where configured, recorded to the **AIVCS** ledger.
4. **Resilience**: Implement recursion limits and error handling to prevent runaway loops or unhandled failures in long-running workflows.

## Development Patterns

- **Trait boundaries**: Prefer implementing `NodeExecutor` for custom node logic.
- **TDD-First**: Add unit tests for every new node type or edge-case in graph routing.
- **Clippy hygiene**: Maintain zero warnings in local development and CI.

## Required Files (7-File Rule)

Maintain consistency across the following files:
1. `README.md`: High-level overview and status.
2. `CLAUDE.md`: Build/test commands.
3. `AGENTS.md`: Capability registry.
4. `GEMINI.md`: This file.
5. `.cursorrules`: IDE rules.
6. `.github/copilot-instructions.md`: Copilot context.
7. `.github/system-instruction.md`: Intelligence standards.

## Useful Commands

```bash
# Run all tests
cargo test

# Run examples
cargo run --example react_agent

# Check clippy
cargo clippy --all-targets -- -D warnings
```
