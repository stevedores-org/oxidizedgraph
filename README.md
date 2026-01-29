# oxidizedgraph

**A humble attempt at LangGraph in Rust** - High-performance agent orchestration framework.

[![Crates.io](https://img.shields.io/crates/v/oxidizedgraph.svg)](https://crates.io/crates/oxidizedgraph)
[![Documentation](https://docs.rs/oxidizedgraph/badge.svg)](https://docs.rs/oxidizedgraph)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## Why oxidizedgraph?

| Feature | LangGraph (Python) | oxidizedgraph |
|---------|-------------------|---------------|
| Parallelism | Limited by GIL | True multi-core |
| Memory per session | ~50MB | ~5MB |
| Startup time | ~200ms | ~10ms |
| Type safety | Runtime | Compile-time |
| Binary size | Needs Python | ~15MB standalone |

## Quick Start

```toml
[dependencies]
oxidizedgraph = "0.1"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
```

```rust
use oxidizedgraph::prelude::*;

// Define a simple node
struct ProcessNode;

#[async_trait]
impl NodeExecutor for ProcessNode {
    fn id(&self) -> &str { "process" }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state.write().unwrap();
        guard.set_context("processed", true);
        Ok(NodeOutput::cont())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build the graph
    let graph = GraphBuilder::new()
        .add_node(ProcessNode)
        .set_entry_point("process")
        .add_edge_to_end("process")
        .compile()?;

    // Execute
    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await?;

    println!("Processed: {:?}", result.get_context::<bool>("processed"));
    Ok(())
}
```

## Core Concepts

### State

State flows through the graph between nodes. The built-in `AgentState` provides common fields:

```rust
pub struct AgentState {
    pub messages: Vec<Message>,      // Conversation history
    pub tool_calls: Vec<ToolCall>,   // Pending tool calls
    pub context: HashMap<String, Value>, // Arbitrary key-value storage
    pub iteration: usize,            // Current iteration count
    pub is_complete: bool,           // Completion flag
}
```

### Nodes

Nodes implement `NodeExecutor` and transform state:

```rust
#[async_trait]
impl NodeExecutor for MyNode {
    fn id(&self) -> &str { "my_node" }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        // Access state
        let mut guard = state.write().unwrap();

        // Do work...

        // Return next action
        Ok(NodeOutput::cont())  // Continue to next node via edges
        // or Ok(NodeOutput::finish())  // End execution
        // or Ok(NodeOutput::continue_to("specific_node"))  // Route to specific node
    }
}
```

### Edges

Edges connect nodes. They can be direct or conditional:

```rust
GraphBuilder::new()
    // Direct edge
    .add_edge("node_a", "node_b")

    // Edge to END
    .add_edge_to_end("node_b")

    // Conditional edge
    .add_conditional_edge("agent", |state| {
        if state.is_complete {
            transitions::END.to_string()
        } else {
            "continue".to_string()
        }
    })
```

### Built-in Nodes

- `EchoNode` - Stores a message in context
- `DelayNode` - Adds a configurable delay
- `StaticTransitionNode` - Always routes to a fixed target
- `ContextRouterNode` - Routes based on context values
- `ConditionalNode` - Routes based on a predicate
- `FunctionNode` - Create nodes from closures
- `LLMNode` - Call LLM providers
- `ToolNode` - Execute pending tool calls

### Runner

Execute your graph with configurable options:

```rust
let runner = GraphRunner::new(
    graph,
    RunnerConfig::default()
        .max_iterations(100)
        .verbose(true)
        .tag("my-workflow"),
);

let result = runner.invoke(initial_state).await?;
```

## Examples

Run the included examples:

```bash
# Simple linear workflow
cargo run --example simple_workflow

# ReAct agent pattern
cargo run --example react_agent
```

## Feature Roadmap

- [x] Core graph primitives
- [x] State management (AgentState)
- [x] NodeExecutor trait
- [x] Conditional edges
- [x] GraphRunner execution
- [x] Built-in nodes (LLM, Tool, Conditional, Function)
- [ ] Checkpointing (SQLite, Postgres)
- [ ] LLM integrations (Anthropic, OpenAI)
- [ ] Streaming execution
- [ ] WASM compilation
- [ ] Python bindings (PyO3)

## License

Apache-2.0 License - see LICENSE file.

## Contributing

Contributions welcome! Please open an issue or PR.
