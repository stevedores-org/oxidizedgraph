use oxidizedgraph::prelude::*;

#[tokio::test]
async fn test_runner_exact_iterations() {
    let graph = GraphBuilder::new()
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "step1",
            |_state| async { Ok(NodeOutput::continue_to("step2")) },
        ))
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "step2",
            |_state| async { Ok(NodeOutput::finish()) },
        ))
        .set_entry_point("step1")
        .compile()
        .unwrap();

    let runner =
        oxidizedgraph::runner::GraphRunner::new(graph, RunnerConfig::new().max_iterations(2));

    let state = AgentState::new();
    let result = runner.invoke(state).await;
    assert!(
        result.is_ok(),
        "Failed with exact iterations limit for runner {:?}",
        result
    );
}

#[tokio::test]
async fn test_streaming_runner_exact_iterations() {
    let graph = GraphBuilder::new()
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "step1",
            |_state| async { Ok(NodeOutput::continue_to("step2")) },
        ))
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "step2",
            |_state| async { Ok(NodeOutput::finish()) },
        ))
        .set_entry_point("step1")
        .compile()
        .unwrap();

    let runner = oxidizedgraph::events::StreamingRunner::new(
        graph,
        std::sync::Arc::new(oxidizedgraph::events::EventBus::new()),
    )
    .with_config(RunnerConfig::new().max_iterations(2));

    let state = AgentState::new();
    let result = runner.invoke("test_thread_stream", state).await;
    assert!(
        result.is_ok(),
        "Failed with exact iterations limit for streaming runner {:?}",
        result
    );
}

#[tokio::test]
async fn test_checkpointing_runner_exact_iterations() {
    let graph = GraphBuilder::new()
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "step1",
            |_state| async { Ok(NodeOutput::continue_to("step2")) },
        ))
        .add_node(oxidizedgraph::nodes::FunctionNode::new(
            "step2",
            |_state| async { Ok(NodeOutput::finish()) },
        ))
        .set_entry_point("step1")
        .compile()
        .unwrap();

    let checkpointer = std::sync::Arc::new(oxidizedgraph::checkpoint::MemoryCheckpointer::new());
    let runner = oxidizedgraph::checkpoint::CheckpointingRunner::new(graph, checkpointer)
        .with_config(RunnerConfig::new().max_iterations(2));

    let state = AgentState::new();
    let result = runner.invoke("test_thread_checkpoint", state).await;
    assert!(
        result.is_ok(),
        "Failed with exact iterations limit for checkpoint runner {:?}",
        result
    );
}
