# GitHub Copilot Instructions — oxidizedgraph

- **Project Type**: Rust Library (Agent Orchestration).
- **Core Stack**: Rust, Tokio, Petgraph, Serde.
- **Conventions**:
  - Use `async-trait` for node executors.
  - State management is via `RwLock<AgentState>`.
  - Prefer `GraphBuilder` for assembling workflows.
- **Style**:
  - Idiomatic Rust 2021.
  - Zero unsafe code without `// SAFETY:` comments.
  - Mandatory unit tests for routing logic.
