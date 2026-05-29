# CLAUDE.md: oxidizedgraph Project Guide

## Project Overview
oxidizedgraph is a Rust framework for building and orchestrating AI agent workflows using a graph-based model.

## Build & Test Commands
- **Build**: `cargo build --workspace`
- **Test**: `cargo test --workspace`
- **Run Example**: `cargo run --example simple_workflow`
- **Check Clippy**: `cargo clippy --all-targets -- -D warnings`

## Development Conventions
- **Language**: Idiomatic Rust 2021.
- **State**: Use `AgentState` for workflow state, accessed via `RwLock` in `SharedState`.
- **Nodes**: Implement `NodeExecutor` trait for custom logic.
- **Edges**: Use `GraphBuilder` to define static and conditional transitions.
- **Error Handling**: Use `NodeError` for node-level failures and `RuntimeError` for framework-level issues.
- **AIVCS**: All production runs should integrate with `AivcsEventSink` (forthcoming).
