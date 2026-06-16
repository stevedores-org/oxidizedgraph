//! Boundary tests around the END / iteration-limit check ordering fix
//! (#98). The reordered branches in CheckpointingRunner and StreamingRunner
//! moved END detection ahead of the iteration cap. These tests pin the
//! happy path AND the symmetric edge cases — so a future regression that
//! re-introduces an off-by-one (e.g. `iterations > max_iterations`,
//! re-ordering the checks, or a tighter ASSERT) flips a red test instead
//! of slipping through.
//!
//! Coverage matrix:
//!
//! | scenario                                        | expected     |
//! |-------------------------------------------------|--------------|
//! | 2-node graph, `max_iterations = 2` (exact)      | Completed    |
//! | 2-node graph, `max_iterations = 1`              | RecursionLimit |
//! | 1-node graph that immediately finishes,         |              |
//! |   `max_iterations = 1` (exact)                  | Completed    |
//! | 1-node graph that immediately finishes,         |              |
//! |   `max_iterations = 0`                          | RecursionLimit |
//!
//! The "entry-point is literally `transitions::END`" case isn't
//! constructible — `GraphBuilder::compile()` rejects an entry that
//! isn't a registered node, so the runtime check that would handle it
//! is dead-but-defensible. The boundary cases above cover every path
//! that user code can actually reach.
//! All three runners (`GraphRunner`, `StreamingRunner`, `CheckpointingRunner`)
//! are exercised via the same shared graph builders so a future divergence
//! in iteration accounting shows up in three tests, not one.

use oxidizedgraph::prelude::*;
use std::sync::Arc;

/// Build a two-step graph where `step2` writes a marker to the state.
/// Used by the exact-iteration happy-path tests so we can assert the
/// graph actually ran end-to-end, not just that the call returned `Ok`.
fn two_step_graph_with_marker() -> oxidizedgraph::graph::CompiledGraph {
    GraphBuilder::new()
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "step1",
            |_state| async { Ok(NodeOutput::continue_to("step2")) },
        ))
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "step2",
            |state| async move {
                let mut guard = state.write().map_err(|e| {
                    oxidizedgraph::error::NodeError::execution_failed(e.to_string())
                })?;
                guard.set_context("reached_step2", true);
                Ok(NodeOutput::finish())
            },
        ))
        .set_entry_point("step1")
        .compile()
        .expect("two-step graph compiles")
}

/// Single-step graph that immediately finishes on the entry node.
/// Used by the off-by-one boundary tests.
fn single_step_graph() -> oxidizedgraph::graph::CompiledGraph {
    GraphBuilder::new()
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "only",
            |state| async move {
                let mut guard = state.write().map_err(|e| {
                    oxidizedgraph::error::NodeError::execution_failed(e.to_string())
                })?;
                guard.set_context("reached_only", true);
                Ok(NodeOutput::finish())
            },
        ))
        .set_entry_point("only")
        .compile()
        .expect("single-step graph compiles")
}

// ───────────────────────────────────────────────────────────────────────
// Happy path: exact-iteration limit completes — and the graph actually ran
// ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn graph_runner_completes_on_exact_iteration_limit() {
    let runner = oxidizedgraph::runner::GraphRunner::new(
        two_step_graph_with_marker(),
        RunnerConfig::new().max_iterations(2),
    );
    let final_state = runner
        .invoke(AgentState::new())
        .await
        .expect("exact-iter run must complete");
    assert_eq!(
        final_state.get_context::<bool>("reached_step2"),
        Some(true),
        "step2 must have actually run, not just been counted"
    );
}

#[tokio::test]
async fn streaming_runner_completes_on_exact_iteration_limit() {
    let runner = oxidizedgraph::events::StreamingRunner::new(
        two_step_graph_with_marker(),
        Arc::new(oxidizedgraph::events::EventBus::new()),
    )
    .with_config(RunnerConfig::new().max_iterations(2));

    let result = runner
        .invoke("test_thread_stream", AgentState::new())
        .await
        .expect("exact-iter run must complete");
    assert!(
        matches!(
            result,
            oxidizedgraph::events::StreamingRunResult::Completed(_)
        ),
        "must be the Completed variant, not some other Ok branch: {result:?}"
    );
    let oxidizedgraph::events::StreamingRunResult::Completed(state) = result else {
        unreachable!()
    };
    assert_eq!(state.get_context::<bool>("reached_step2"), Some(true));
}

#[tokio::test]
async fn checkpointing_runner_completes_on_exact_iteration_limit() {
    let checkpointer = Arc::new(oxidizedgraph::checkpoint::MemoryCheckpointer::new());
    let runner = oxidizedgraph::checkpoint::CheckpointingRunner::new(
        two_step_graph_with_marker(),
        checkpointer,
    )
    .with_config(RunnerConfig::new().max_iterations(2));

    let result = runner
        .invoke("test_thread_checkpoint", AgentState::new())
        .await
        .expect("exact-iter run must complete");
    assert!(
        matches!(result, oxidizedgraph::checkpoint::RunResult::Completed(_)),
        "must be Completed, not Interrupted/etc: {result:?}"
    );
    assert_eq!(
        result.state().get_context::<bool>("reached_step2"),
        Some(true)
    );
}

// ───────────────────────────────────────────────────────────────────────
// Boundary: iteration cap below required still rejects (verifies the
// reorder didn't disable the iter check)
// ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn graph_runner_rejects_when_max_is_below_required() {
    // 2-node graph needs 2 transitions worth of work; max=1 must
    // RecursionLimit. Without this, a future tweak that always-completes
    // on END before checking iter would silently neuter the cap.
    let runner = oxidizedgraph::runner::GraphRunner::new(
        two_step_graph_with_marker(),
        RunnerConfig::new().max_iterations(1),
    );
    let err = runner.invoke(AgentState::new()).await.unwrap_err();
    assert!(
        matches!(err, oxidizedgraph::error::RuntimeError::RecursionLimit(1)),
        "expected RecursionLimit(1), got {err:?}"
    );
}

#[tokio::test]
async fn streaming_runner_rejects_when_max_is_below_required() {
    let runner = oxidizedgraph::events::StreamingRunner::new(
        two_step_graph_with_marker(),
        Arc::new(oxidizedgraph::events::EventBus::new()),
    )
    .with_config(RunnerConfig::new().max_iterations(1));
    let err = runner
        .invoke("rejects", AgentState::new())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        oxidizedgraph::error::RuntimeError::RecursionLimit(1)
    ));
}

#[tokio::test]
async fn checkpointing_runner_rejects_when_max_is_below_required() {
    let checkpointer = Arc::new(oxidizedgraph::checkpoint::MemoryCheckpointer::new());
    let runner = oxidizedgraph::checkpoint::CheckpointingRunner::new(
        two_step_graph_with_marker(),
        checkpointer,
    )
    .with_config(RunnerConfig::new().max_iterations(1));
    let err = runner
        .invoke("rejects", AgentState::new())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        oxidizedgraph::error::RuntimeError::RecursionLimit(1)
    ));
}

// ───────────────────────────────────────────────────────────────────────
// Boundary: single-step graph with max=1 (exact) and max=0 (below)
// ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn graph_runner_single_step_with_exact_max_completes() {
    let runner = oxidizedgraph::runner::GraphRunner::new(
        single_step_graph(),
        RunnerConfig::new().max_iterations(1),
    );
    let final_state = runner.invoke(AgentState::new()).await.unwrap();
    assert_eq!(final_state.get_context::<bool>("reached_only"), Some(true));
}

#[tokio::test]
async fn graph_runner_single_step_with_zero_max_rejects() {
    let runner = oxidizedgraph::runner::GraphRunner::new(
        single_step_graph(),
        RunnerConfig::new().max_iterations(0),
    );
    let err = runner.invoke(AgentState::new()).await.unwrap_err();
    assert!(matches!(
        err,
        oxidizedgraph::error::RuntimeError::RecursionLimit(0)
    ));
}

// Note: the "entry point is literally END" degenerate case named in the
// PR-#98 review is not constructible via the public API — `GraphBuilder::compile`
// validates that the entry node exists, and `transitions::END = "__end__"`
// is not a registerable node id. The runtime semantics of "END check before
// iter check" therefore only matter when execution REACHES END after at
// least one step, which the cases above already cover at the boundary.
