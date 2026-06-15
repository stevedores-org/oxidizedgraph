use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use oxidizedgraph::prelude::*;
use std::sync::Arc;
use tokio::runtime::Runtime;

struct RouteNode {
    id: String,
    transition: String,
}

#[async_trait]
impl NodeExecutor for RouteNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
        Ok(NodeOutput::transition(self.transition.clone()))
    }
}

struct FinishNode {
    id: String,
}

#[async_trait]
impl NodeExecutor for FinishNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
        Ok(NodeOutput::finish())
    }
}

fn build_compile_graph(width: usize) -> GraphBuilder {
    let mut builder = GraphBuilder::new()
        .name("perf-graph")
        .set_entry_point("router");

    builder = builder.add_node(RouteNode {
        id: "router".to_string(),
        transition: "route-0".to_string(),
    });

    for idx in 0..width {
        let node_id = format!("node-{idx}");
        builder = builder.add_node(FinishNode {
            id: node_id.clone(),
        });
        builder = builder.add_edge_with_key("router", node_id, format!("route-{idx}"));
    }

    builder
}

fn build_invoke_graph(width: usize) -> CompiledGraph {
    build_compile_graph(width).compile().unwrap()
}

fn benchmark_graph_compile(c: &mut Criterion) {
    c.bench_function("graph_compile_128_edges", |b| {
        b.iter(|| {
            let graph = black_box(build_compile_graph(128));
            black_box(graph.compile().unwrap())
        })
    });
}

fn benchmark_graph_invoke(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let graph = build_invoke_graph(128);
    let runner = GraphRunner::with_defaults(graph);
    let state = {
        let mut state = AgentState::new();
        state.set_context("route", "route-127");
        state
    };

    c.bench_function("graph_invoke_128_routes", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let result = runner.invoke(black_box(state.clone())).await.unwrap();
                black_box(result)
            })
        })
    });
}

fn benchmark_checkpoint_save(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let state = AgentState::with_user_message("benchmark checkpointing");
    let checkpoint = Checkpoint::new("thread-bench", "node-a", state);

    c.bench_function("checkpoint_save", |b| {
        b.iter_batched(
            MemoryCheckpointer::new,
            |checkpointer| {
                runtime.block_on(async {
                    checkpointer
                        .save(black_box(checkpoint.clone()))
                        .await
                        .unwrap();
                });
            },
            BatchSize::SmallInput,
        )
    });

    let populated = runtime.block_on(async {
        let checkpointer = MemoryCheckpointer::new();
        for idx in 0..32 {
            let checkpoint =
                Checkpoint::new("thread-bench", format!("node-{idx}"), AgentState::new());
            checkpointer.save(checkpoint).await.unwrap();
        }
        checkpointer
    });

    c.bench_function("checkpoint_list", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let list = populated.list(black_box("thread-bench")).await.unwrap();
                black_box(list)
            })
        })
    });
}

fn benchmark_event_dispatch(c: &mut Criterion) {
    let bus = Arc::new(EventBus::new());
    let _subscriber = bus.subscribe();
    let event = Event::graph_started(
        "thread-bench",
        Some("perf".to_string()),
        "router".to_string(),
    );

    c.bench_function("event_publish", |b| {
        b.iter(|| {
            black_box(bus.publish(black_box(event.clone())));
        })
    });
}

fn benchmark_context_pack(c: &mut Criterion) {
    let retrieval = (0..16)
        .map(|idx| {
            let doc = RepositoryDocument::source(
                "stevedores-org/oxidizedgraph",
                format!("src/module-{idx}.rs"),
                format!("context pack benchmark content {idx}"),
            )
            .with_symbol(format!("Symbol{idx}"));
            RetrievalResult {
                document: doc,
                score: (idx + 1) as f32,
                matched_terms: vec!["context".to_string(), "pack".to_string()],
            }
        })
        .collect::<Vec<_>>();
    let episodes_storage = (0..8)
        .map(|idx| {
            EpisodicMemory::new(
                format!("task-{idx}"),
                format!("run-{idx}"),
                "stevedores-org/oxidizedgraph",
                "Recorded a previous run for context packing.",
                if idx % 2 == 0 {
                    RunOutcome::Success
                } else {
                    RunOutcome::Failure
                },
            )
        })
        .collect::<Vec<_>>();
    let decisions_storage = (0..8)
        .map(|idx| {
            DecisionMemory::new(
                format!("decision-{idx}"),
                format!("task-{idx}"),
                "stevedores-org/oxidizedgraph",
                "use graph execution",
                "Reduce allocations in the execution hot path.",
            )
        })
        .collect::<Vec<_>>();
    let episodes = episodes_storage.iter().collect::<Vec<_>>();
    let decisions = decisions_storage.iter().collect::<Vec<_>>();

    let packer = ContextPacker::new(4_096).reserved_tokens(256);
    let policy = ContextPolicy::default();

    c.bench_function("context_pack", |b| {
        b.iter(|| {
            black_box(packer.pack(
                black_box(&retrieval),
                black_box(&episodes),
                black_box(&decisions),
                black_box(&policy),
            ))
        })
    });
}

criterion_group!(
    benches,
    benchmark_graph_compile,
    benchmark_graph_invoke,
    benchmark_checkpoint_save,
    benchmark_event_dispatch,
    benchmark_context_pack
);
criterion_main!(benches);
