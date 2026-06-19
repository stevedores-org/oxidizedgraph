# oxidizedgraph — Lornu AI Commercial Enterprise Edition

**LangGraph in Rust** — high-performance agent orchestration with enterprise
tenancy, RBAC, audit, and SLO guardrails.

> **Private / commercial.** This repository is `lornu-ai/oxidizedgraph`. The
> Apache-2.0 community edition is
> [stevedores-org/oxidizedgraph](https://github.com/stevedores-org/oxidizedgraph).
> See [UPSTREAM.md](UPSTREAM.md) and [docs/ENTERPRISE.md](docs/ENTERPRISE.md).

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

# Autonomous dev workflow (Issue #18 Phase 1)
cargo run --example autonomous_dev_workflow
```

See [docs/ROADMAP-18.md](docs/ROADMAP-18.md) for the autonomous agent orchestration roadmap ([#18](https://github.com/stevedores-org/oxidizedgraph/issues/18)).

## Feature Roadmap

- [x] Core graph primitives
- [x] State management (AgentState)
- [x] NodeExecutor trait
- [x] Conditional edges
- [x] GraphRunner execution
- [x] Built-in nodes (LLM, Tool, Conditional, Function)
- [x] Checkpointing (memory; optional SurrealDB persistence)
- [x] Streaming execution and events
- [x] Autonomous orchestration Phase 1 — traced execution, tool policy, quality gates ([#18](https://github.com/stevedores-org/oxidizedgraph/issues/18))
- [ ] LLM integrations (Anthropic, OpenAI) — production providers
- [ ] WASM compilation
- [ ] Python bindings (PyO3)

## License

Apache-2.0 License - see LICENSE file.

## Deploy (OCI + GKE)

Container images are built with **Nix** (`flake.nix`) and published via `dockworker.toml` to `ghcr.io/stevedores-org/oxidizedgraph/server`. Kubernetes manifests live under `deploy/` (Kustomize base + GKE Autopilot overlay).

```bash
just image          # nix build .#server-image
just deploy-gke     # kubectl apply -k deploy/overlays/gke-autopilot
```

See [docs/DEPLOY_GKE.md](docs/DEPLOY_GKE.md) and [docs/PACKAGING.md](docs/PACKAGING.md). No `Dockerfile` — images are OCI-standard, built by Nix (`pkgs.dockerTools.buildLayeredImage`) and packaged with `dockworker.ai`.

## Contributing

See `CONTRIBUTING.md` for a practical start-contributing map and workflow.
