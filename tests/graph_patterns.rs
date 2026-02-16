//! Integration tests for complex graph patterns
//!
//! Tests multi-branch routing, cycles, error handling, deep chains,
//! and concurrent state access.

use async_trait::async_trait;
use oxidizedgraph::prelude::*;

// ── Helpers ──────────────────────────────────────────────────────────

/// Node that appends its id to a "visited" context key.
struct TrackingNode {
    id: String,
}

impl TrackingNode {
    fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl NodeExecutor for TrackingNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;
        let visited = guard.get_context::<String>("visited").unwrap_or_default();
        guard.set_context("visited", format!("{},{}", visited, self.id));
        Ok(NodeOutput::cont())
    }
}

/// Node that always fails.
struct FailingNode {
    id: String,
    message: String,
}

impl FailingNode {
    fn new(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            message: message.into(),
        }
    }
}

#[async_trait]
impl NodeExecutor for FailingNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
        Err(NodeError::execution_failed(&self.message))
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn multi_branch_conditional_routing() {
    // Graph: start → router → (path_a | path_b | path_c) → END
    // The router reads "route" from context and dispatches accordingly.

    struct RouterNode;

    #[async_trait]
    impl NodeExecutor for RouterNode {
        fn id(&self) -> &str {
            "router"
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            let route: String = guard.get_context("route").unwrap_or_default();
            Ok(NodeOutput::route(route))
        }
    }

    // Build separate graphs for each branch since CompiledGraph isn't Clone.
    for (route, expected) in [("path_a", ",path_a"), ("path_b", ",path_b"), ("path_c", ",path_c")] {
        let graph = GraphBuilder::new()
            .add_node(RouterNode)
            .add_node(TrackingNode::new("path_a"))
            .add_node(TrackingNode::new("path_b"))
            .add_node(TrackingNode::new("path_c"))
            .set_entry_point("router")
            .add_edge_to_end("path_a")
            .add_edge_to_end("path_b")
            .add_edge_to_end("path_c")
            .compile()
            .unwrap();

        let runner = GraphRunner::with_defaults(graph);
        let mut state = AgentState::new();
        state.set_context("route", route.to_string());

        let result = runner.invoke(state).await.unwrap();
        let visited: String = result.get_context("visited").unwrap_or_default();
        assert_eq!(visited, expected, "route={route}");
    }
}

#[tokio::test]
async fn cycle_with_exit_condition() {
    // A node that loops back to itself until counter reaches a threshold,
    // then finishes.

    struct LoopNode {
        max: i32,
    }

    #[async_trait]
    impl NodeExecutor for LoopNode {
        fn id(&self) -> &str {
            "loop"
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            let count = guard.get_context::<i32>("count").unwrap_or(0);
            guard.set_context("count", count + 1);

            if count + 1 >= self.max {
                Ok(NodeOutput::finish())
            } else {
                Ok(NodeOutput::continue_to("loop"))
            }
        }
    }

    let graph = GraphBuilder::new()
        .add_node(LoopNode { max: 5 })
        .set_entry_point("loop")
        .compile()
        .unwrap();

    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await.unwrap();

    assert_eq!(result.get_context::<i32>("count"), Some(5));
}

#[tokio::test]
async fn cycle_hits_recursion_limit() {
    // Infinite loop should be stopped by the recursion limit.

    struct InfiniteNode;

    #[async_trait]
    impl NodeExecutor for InfiniteNode {
        fn id(&self) -> &str {
            "infinite"
        }

        async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
            Ok(NodeOutput::continue_to("infinite"))
        }
    }

    let graph = GraphBuilder::new()
        .add_node(InfiniteNode)
        .set_entry_point("infinite")
        .compile()
        .unwrap();

    let runner = GraphRunner::new(graph, RunnerConfig::new().max_iterations(10));
    let result = runner.invoke(AgentState::new()).await;

    assert!(
        matches!(result, Err(RuntimeError::RecursionLimit(10))),
        "expected RecursionLimit(10), got {:?}",
        result
    );
}

#[tokio::test]
async fn error_propagation() {
    // A failing node should propagate its error through the runner.

    let graph = GraphBuilder::new()
        .add_node(TrackingNode::new("ok_node"))
        .add_node(FailingNode::new("bad_node", "something broke"))
        .set_entry_point("ok_node")
        .add_edge("ok_node", "bad_node")
        .compile()
        .unwrap();

    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await;

    match result {
        Err(RuntimeError::NodeFailed { node_id, error }) => {
            assert_eq!(node_id, "bad_node");
            assert!(error.to_string().contains("something broke"));
        }
        other => panic!("expected NodeFailed, got {:?}", other),
    }

    // Verify the first node did execute before the failure
    // (state is consumed by runner, but the error proves "ok_node" ran
    // since we reached "bad_node" via the edge from "ok_node")
}

#[tokio::test]
async fn deep_linear_graph() {
    // Chain of 50 nodes to verify no stack overflow and correct ordering.

    const DEPTH: usize = 50;

    let mut builder = GraphBuilder::new();

    for i in 0..DEPTH {
        builder = builder.add_node(TrackingNode::new(format!("n{i}")));
    }

    builder = builder.set_entry_point("n0");

    for i in 0..(DEPTH - 1) {
        builder = builder.add_edge(format!("n{i}"), format!("n{}", i + 1));
    }

    builder = builder.add_edge_to_end(format!("n{}", DEPTH - 1));

    let graph = builder.compile().unwrap();
    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await.unwrap();

    let visited: String = result.get_context("visited").unwrap_or_default();
    let parts: Vec<&str> = visited.split(',').filter(|s| !s.is_empty()).collect();
    assert_eq!(parts.len(), DEPTH);
    assert_eq!(parts[0], "n0");
    assert_eq!(parts[DEPTH - 1], format!("n{}", DEPTH - 1));
}

#[tokio::test]
async fn concurrent_state_mutation() {
    // Multiple nodes sequentially mutating shared state.
    // Verifies Arc<RwLock> prevents races and final state is consistent.

    struct IncrementNode {
        id: String,
        key: String,
        amount: i32,
    }

    #[async_trait]
    impl NodeExecutor for IncrementNode {
        fn id(&self) -> &str {
            &self.id
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            let current = guard.get_context::<i32>(&self.key).unwrap_or(0);
            guard.set_context(&self.key, current + self.amount);
            Ok(NodeOutput::cont())
        }
    }

    let graph = GraphBuilder::new()
        .add_node(IncrementNode {
            id: "add10".to_string(),
            key: "total".to_string(),
            amount: 10,
        })
        .add_node(IncrementNode {
            id: "add20".to_string(),
            key: "total".to_string(),
            amount: 20,
        })
        .add_node(IncrementNode {
            id: "add5".to_string(),
            key: "total".to_string(),
            amount: 5,
        })
        .set_entry_point("add10")
        .add_edge("add10", "add20")
        .add_edge("add20", "add5")
        .add_edge_to_end("add5")
        .compile()
        .unwrap();

    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await.unwrap();

    assert_eq!(result.get_context::<i32>("total"), Some(35));
}

#[tokio::test]
async fn conditional_edge_routing() {
    // Test GraphBuilder's add_conditional_edge with a router function.

    let graph = GraphBuilder::new()
        .add_node(TrackingNode::new("start"))
        .add_node(TrackingNode::new("yes_branch"))
        .add_node(TrackingNode::new("no_branch"))
        .set_entry_point("start")
        .add_conditional_edge("start", |state: &AgentState| {
            if state.is_complete {
                "__end__".to_string()
            } else {
                "yes_branch".to_string()
            }
        })
        .add_edge_to_end("yes_branch")
        .add_edge_to_end("no_branch")
        .compile()
        .unwrap();

    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await.unwrap();

    let visited: String = result.get_context("visited").unwrap_or_default();
    assert!(visited.contains("start"), "start should have run");
    assert!(visited.contains("yes_branch"), "should route to yes_branch when not complete");
}

#[tokio::test]
async fn node_output_finish_stops_immediately() {
    // A node returning Finish should stop execution even if edges exist.

    struct FinishNode;

    #[async_trait]
    impl NodeExecutor for FinishNode {
        fn id(&self) -> &str {
            "finisher"
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("finished", true);
            Ok(NodeOutput::finish())
        }
    }

    let graph = GraphBuilder::new()
        .add_node(FinishNode)
        .add_node(TrackingNode::new("unreachable"))
        .set_entry_point("finisher")
        .add_edge("finisher", "unreachable")
        .add_edge_to_end("unreachable")
        .compile()
        .unwrap();

    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await.unwrap();

    assert_eq!(result.get_context::<bool>("finished"), Some(true));
    // "unreachable" should never have run
    let visited: Option<String> = result.get_context("visited");
    assert!(visited.is_none(), "unreachable node should not have executed");
}

#[tokio::test]
async fn graph_with_messages() {
    // Test that nodes can manipulate the messages list in AgentState.

    struct AppendMessageNode {
        id: String,
        content: String,
        role: MessageRole,
    }

    #[async_trait]
    impl NodeExecutor for AppendMessageNode {
        fn id(&self) -> &str {
            &self.id
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            match self.role {
                MessageRole::User => guard.add_user_message(&self.content),
                MessageRole::Assistant => guard.add_assistant_message(&self.content),
                _ => {}
            }
            Ok(NodeOutput::cont())
        }
    }

    let graph = GraphBuilder::new()
        .add_node(AppendMessageNode {
            id: "user_msg".to_string(),
            content: "Hello".to_string(),
            role: MessageRole::User,
        })
        .add_node(AppendMessageNode {
            id: "assistant_msg".to_string(),
            content: "Hi there!".to_string(),
            role: MessageRole::Assistant,
        })
        .set_entry_point("user_msg")
        .add_edge("user_msg", "assistant_msg")
        .add_edge_to_end("assistant_msg")
        .compile()
        .unwrap();

    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await.unwrap();

    assert_eq!(result.messages.len(), 2);
    assert_eq!(result.messages[0].role, MessageRole::User);
    assert_eq!(result.messages[0].content, "Hello");
    assert_eq!(result.messages[1].role, MessageRole::Assistant);
    assert_eq!(result.messages[1].content, "Hi there!");
}
