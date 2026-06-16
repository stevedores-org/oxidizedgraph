use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use oxidizedgraph::checkpoint::{Checkpoint, Checkpointer, MemoryCheckpointer};
use oxidizedgraph::events::EventBus;
use oxidizedgraph::graph::{GraphBuilder, NodeExecutor, NodeOutput};
use oxidizedgraph::prelude::{AgentState, SharedState};
use oxidizedgraph::runner::{GraphRunner, RunnerConfig};
use oxidizedgraph::state::ToolCall;
use oxidizedgraph::{error::NodeError, events::Event};
use std::time::Duration;
use tokio::runtime::Runtime;

struct LoopNode {
    id: String,
    limit: usize,
}

#[async_trait::async_trait]
impl NodeExecutor for LoopNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;
        let count = guard.get_context::<usize>("count").unwrap_or(0) + 1;
        guard.set_context("count", count);

        if count >= self.limit {
            Ok(NodeOutput::finish())
        } else {
            Ok(NodeOutput::cont())
        }
    }
}

fn make_noisy_graph(noise_nodes: usize, loop_limit: usize) -> GraphRunner {
    let mut builder = GraphBuilder::new()
        .name("bench")
        .add_node(LoopNode {
            id: "loop".to_string(),
            limit: loop_limit,
        })
        .set_entry_point("loop")
        .add_edge("loop", "loop");

    for idx in 0..noise_nodes {
        let node_id = format!("noise-{idx}");
        builder = builder
            .add_node(LoopNode {
                id: node_id.clone(),
                limit: 1,
            })
            .add_edge_to_end(node_id);
    }

    let graph = builder.compile().expect("compile benchmark graph");
    GraphRunner::new(graph, RunnerConfig::default().max_iterations(512))
}

fn make_state() -> AgentState {
    let mut state = AgentState::new();
    state.tool_calls = (0..8)
        .map(|idx| ToolCall::new(format!("tool-{idx}"), "noop", serde_json::json!({"i": idx})))
        .collect();

    for idx in 0..32 {
        state.set_context(format!("key-{idx}"), format!("value-{idx}"));
    }

    state
}

fn naive_transition_lookup(
    graph: &oxidizedgraph::graph::CompiledGraph,
    from: &str,
    state: &AgentState,
    transition_key: &str,
) -> Option<String> {
    for edge in graph.edges() {
        if edge.from != from {
            continue;
        }
        if edge.transition_key.as_deref() != Some(transition_key) {
            continue;
        }
        return match &edge.edge_type {
            oxidizedgraph::graph::EdgeType::Direct => Some(edge.to.clone()),
            oxidizedgraph::graph::EdgeType::Conditional(router) => Some(router(state)),
        };
    }

    if transition_key == oxidizedgraph::graph::transitions::CONTINUE {
        for edge in graph.edges() {
            if edge.from != from || edge.transition_key.is_some() {
                continue;
            }
            return match &edge.edge_type {
                oxidizedgraph::graph::EdgeType::Direct => Some(edge.to.clone()),
                oxidizedgraph::graph::EdgeType::Conditional(router) => Some(router(state)),
            };
        }
    }

    None
}

fn bench_graph_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_build");
    for &noise_nodes in &[32usize, 256usize] {
        group.bench_with_input(
            BenchmarkId::from_parameter(noise_nodes),
            &noise_nodes,
            |b, &n| {
                b.iter(|| {
                    let mut builder = GraphBuilder::new()
                        .name("build")
                        .add_node(LoopNode {
                            id: "loop".to_string(),
                            limit: 1,
                        })
                        .set_entry_point("loop")
                        .add_edge("loop", "loop");

                    for idx in 0..n {
                        let node_id = format!("noise-{idx}");
                        builder = builder
                            .add_node(LoopNode {
                                id: node_id.clone(),
                                limit: 1,
                            })
                            .add_edge_to_end(node_id);
                    }

                    black_box(builder.compile().expect("compile graph"));
                })
            },
        );
    }
    group.finish();
}

fn bench_graph_invoke(c: &mut Criterion) {
    let runtime = Runtime::new().expect("runtime");
    let mut group = c.benchmark_group("graph_invoke");
    for &noise_nodes in &[32usize, 256usize] {
        let runner = make_noisy_graph(noise_nodes, 256usize);
        group.bench_with_input(
            BenchmarkId::from_parameter(noise_nodes),
            &noise_nodes,
            |b, _| {
                b.iter(|| {
                    let state = make_state();
                    let result = runtime
                        .block_on(runner.invoke(state))
                        .expect("invoke graph");
                    black_box(result);
                })
            },
        );
    }
    group.finish();
}

fn bench_transition_lookup(c: &mut Criterion) {
    let graph = make_noisy_graph(256usize, 256usize).graph().clone();
    let state = make_state();
    let mut group = c.benchmark_group("transition_lookup");

    group.bench_function("naive", |b| {
        b.iter(|| {
            black_box(naive_transition_lookup(
                &graph,
                "loop",
                &state,
                oxidizedgraph::graph::transitions::CONTINUE,
            ))
        })
    });

    group.bench_function("indexed", |b| {
        b.iter(|| {
            black_box(graph.get_next_node_for_transition(
                "loop",
                &state,
                oxidizedgraph::graph::transitions::CONTINUE,
            ))
        })
    });

    group.finish();
}

fn bench_checkpointing(c: &mut Criterion) {
    let runtime = Runtime::new().expect("runtime");
    let state = make_state();
    let checkpoint = Checkpoint::new("thread-1", "loop", state);
    let checkpointer = MemoryCheckpointer::new();

    let mut group = c.benchmark_group("checkpointing");
    group.bench_function("save", |b| {
        b.iter(|| {
            runtime
                .block_on(checkpointer.save(black_box(checkpoint.clone())))
                .expect("save checkpoint");
        })
    });

    group.bench_function("load", |b| {
        runtime
            .block_on(checkpointer.save(checkpoint.clone()))
            .expect("seed checkpoint");
        b.iter(|| {
            black_box(
                runtime
                    .block_on(checkpointer.load("thread-1"))
                    .expect("load checkpoint"),
            )
        })
    });

    group.finish();
}

fn bench_event_dispatch(c: &mut Criterion) {
    let bus = EventBus::new();
    let _subscribers: Vec<_> = (0..16).map(|_| bus.subscribe()).collect();
    let event = Event::graph_started("thread-1", Some("bench".to_string()), "loop".to_string());

    c.bench_function("event_dispatch/publish", |b| {
        b.iter(|| black_box(bus.publish(event.clone())))
    });
}

fn bench_state_packing(c: &mut Criterion) {
    let state = make_state();
    let mut group = c.benchmark_group("state_packing");

    group.bench_function("clone", |b| b.iter(|| black_box(state.clone())));

    group.bench_function("serialize_context", |b| {
        b.iter(|| {
            black_box(serde_json::to_vec(black_box(&state.context)).expect("serialize context"))
        })
    });

    group.finish();
}

fn configure_criterion() -> Criterion {
    Criterion::default()
        .sample_size(20)
        .measurement_time(Duration::from_secs(1))
}

criterion_group! {
    name = benches;
    config = configure_criterion();
    targets = bench_graph_build, bench_graph_invoke, bench_transition_lookup, bench_checkpointing, bench_event_dispatch, bench_state_packing
}
criterion_main!(benches);
