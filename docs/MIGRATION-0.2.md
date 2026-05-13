# Migration Guide: 0.1.x → 0.2.0

## Deprecated APIs

The generic `Node<S>`, `Edge<S>`, and `Router<S>` traits in the `node` and `edge` modules are deprecated. These were never part of the default prelude and were previously not even compiled into the crate. If you were somehow using them directly, migrate to the primary API.

### Before (deprecated)

```rust
use oxidizedgraph::node::Node;
use oxidizedgraph::edge::Edge;

struct MyNode;

#[async_trait]
impl<S: State> Node<S> for MyNode {
    fn id(&self) -> &str { "my_node" }
    async fn run(&self, state: S) -> Result<NodeResult<S>, NodeError> {
        Ok(NodeResult::Continue(state))
    }
}
```

### After (recommended)

```rust
use oxidizedgraph::prelude::*;

struct MyNode;

#[async_trait]
impl NodeExecutor for MyNode {
    fn id(&self) -> &str { "my_node" }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state.write().map_err(|e| NodeError::execution_failed(e.to_string()))?;
        // Modify state via Arc<RwLock<AgentState>>
        Ok(NodeOutput::cont())
    }
}
```

### Key Differences

| Aspect | Generic (`Node<S>`) | Concrete (`NodeExecutor`) |
|--------|---------------------|---------------------------|
| State type | Generic `S: State` | Fixed `SharedState` (`Arc<RwLock<AgentState>>`) |
| Method | `run(&self, state: S)` | `execute(&self, state: SharedState)` |
| Return | `NodeResult<S>` | `NodeOutput` |
| Thread safety | Caller manages | Built-in via `Arc<RwLock>` |
| Builder | None | `GraphBuilder` fluent API |
| Runner | `Runtime<S>` (incomplete) | `GraphRunner` (production-ready) |

### Dead Code: `runtime.rs`

The `runtime.rs` file contains an incomplete `Runtime<S>` that references a generic `CompiledGraph<S>` which no longer exists. This file is not part of the module tree and is not compiled. It will be deleted in v0.3.0.

Note: `runner.rs` exports `pub type Runtime = GraphRunner;` as a convenience alias — this is the correct runtime to use.

## Timeline

- **v0.2.0**: Deprecation warnings added to `node` and `edge` modules
- **v0.3.0** (2-3 months later): Deprecated modules and `runtime.rs` removed
