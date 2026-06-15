use oxidizedgraph::prelude::*;
use std::sync::{Arc, RwLock};

#[tokio::test]
async fn test_planning_and_decomposition() {
    // 1. Setup PlanningNode
    let planning_node = PlanningNode::with_decomposer("planner", |goal| {
        vec![
            Task::new("t1", "Task 1", format!("Execute {goal} phase 1")).with_estimate(30),
            Task::new("t2", "Task 2", format!("Execute {goal} phase 2"))
                .depends_on("t1")
                .with_estimate(60),
        ]
    });

    let graph = GraphBuilder::new()
        .add_node(planning_node)
        .set_entry_point("planner")
        .add_edge_to_end("planner")
        .compile()
        .unwrap();

    let runner = GraphRunner::with_defaults(graph);
    let mut state = AgentState::new();
    state.set_context("goal", "build_compiler".to_string());

    let result = runner.invoke(state).await.unwrap();

    // 2. Assertions
    let plan = result.get_context::<EpicPlan>("epic_plan").unwrap();
    assert_eq!(plan.goal, "build_compiler");
    assert_eq!(plan.tasks.len(), 2);

    let progress = result.get_context::<PlanProgress>("plan_progress").unwrap();
    assert_eq!(progress.total_tasks, 2);
    assert_eq!(progress.completed_tasks, 0);
    assert_eq!(progress.percent_complete, 0.0);
    assert_eq!(progress.confidence_score, 1.0);
    assert!(progress.estimated_completion.is_some());
}

#[tokio::test]
async fn test_scheduler_dependency_resolving() {
    let mut plan = EpicPlan::new("build");
    plan.add_task(Task::new("a", "A", "desc"));
    plan.add_task(Task::new("b", "B", "desc").depends_on("a"));

    let scheduler = Scheduler::new();

    // Originally, only 'a' is ready
    let next = scheduler.next_tasks(&plan);
    assert_eq!(next, vec!["a".to_string()]);

    // Mark 'a' as running and then completed
    plan.update_task_status("a", TaskStatus::Running);
    assert!(scheduler.next_tasks(&plan).is_empty());

    plan.update_task_status("a", TaskStatus::Completed);
    // Now 'b' should be ready
    let next2 = scheduler.next_tasks(&plan);
    assert_eq!(next2, vec!["b".to_string()]);
}

#[tokio::test]
async fn test_scheduler_cycle_detection() {
    let mut plan = EpicPlan::new("build");
    plan.add_task(Task::new("a", "A", "desc").depends_on("b"));
    plan.add_task(Task::new("b", "B", "desc").depends_on("a"));

    let scheduler = Scheduler::new();
    assert!(scheduler.has_cycles(&plan));

    // Also verify SchedulerNode errs out on cycles
    let scheduler_node = SchedulerNode::new("scheduler");
    let state = Arc::new(RwLock::new(AgentState::new()));
    {
        let mut guard = state.write().unwrap();
        guard.set_context("epic_plan", plan);
    }

    let res = scheduler_node.execute(state).await;
    assert!(res.is_err());
    let err_str = res.err().unwrap().to_string();
    assert!(err_str.contains("cycles"));
}

#[tokio::test]
async fn test_critical_path_prioritization() {
    let mut plan = EpicPlan::new("build");
    // a has 2 downstream dependencies (b, c)
    // b has 1 downstream dependency (c)
    // d has 0 downstream dependencies
    plan.add_task(Task::new("a", "A", "desc"));
    plan.add_task(Task::new("b", "B", "desc").depends_on("a"));
    plan.add_task(Task::new("c", "C", "desc").depends_on("b"));
    plan.add_task(Task::new("d", "D", "desc"));

    let scheduler = Scheduler::new();
    let ready = scheduler.next_tasks(&plan); // 'a' and 'd' are ready
    assert!(ready.contains(&"a".to_string()));
    assert!(ready.contains(&"d".to_string()));

    let prioritized = scheduler.prioritize_tasks(&plan, &ready);
    // 'a' must be prioritized before 'd' because of transitively blocking more tasks
    assert_eq!(prioritized[0], "a".to_string());
    assert_eq!(prioritized[1], "d".to_string());
}

#[tokio::test]
async fn test_auto_replanning_and_recovery() {
    let mut plan = EpicPlan::new("build");
    plan.add_task(Task::new("t1", "Task 1", "desc"));
    plan.add_task(Task::new("t2", "Task 2", "desc").depends_on("t1"));

    // Register a recovery task for 't1'
    let recovery = Task::new("rec1", "Recovery for 1", "desc");
    let scheduler_node = SchedulerNode::new("scheduler").register_recovery("t1", recovery);

    // Simulate t1 failing
    plan.update_task_status("t1", TaskStatus::Failed);

    let state = Arc::new(RwLock::new(AgentState::new()));
    {
        let mut guard = state.write().unwrap();
        guard.set_context("epic_plan", plan);
    }

    // Execute SchedulerNode to trigger auto-replan recovery
    let output = scheduler_node.execute(Arc::clone(&state)).await.unwrap();

    // Assert that we routed to replan_injected
    if let NodeOutput::Transition(t) = output {
        assert_eq!(t, "replan_injected");
    } else {
        panic!("expected Transition output");
    }

    // Inspect mutated plan in state
    let guard = state.read().unwrap();
    let updated_plan = guard.get_context::<EpicPlan>("epic_plan").unwrap();

    // Verify recovery task 'rec1' was added to the plan
    assert!(updated_plan.tasks.contains_key("rec1"));
    let rec_task = updated_plan.get_task("rec1").unwrap();
    assert_eq!(rec_task.status, TaskStatus::Pending);

    // Verify 't1' status was reset to Pending and now depends on 'rec1'
    let t1_task = updated_plan.get_task("t1").unwrap();
    assert_eq!(t1_task.status, TaskStatus::Pending);
    assert_eq!(t1_task.dependencies, vec!["rec1".to_string()]);
}

#[tokio::test]
async fn test_progress_reporting_and_blocked_propagation() {
    let mut plan = EpicPlan::new("build");
    plan.add_task(Task::new("t1", "Task 1", "desc"));
    plan.add_task(Task::new("t2", "Task 2", "desc").depends_on("t1"));

    // Fail t1
    plan.update_task_status("t1", TaskStatus::Failed);

    // Verify that t2 is automatically marked Blocked
    let t2_task = plan.get_task("t2").unwrap();
    assert_eq!(t2_task.status, TaskStatus::Blocked);

    // Verify progress calculations
    let progress = PlanProgress::calculate(&plan);
    assert_eq!(progress.total_tasks, 2);
    assert_eq!(progress.completed_tasks, 0);
    assert_eq!(progress.failed_tasks, 1);
    assert_eq!(progress.blocked_tasks, 1);
    assert!(progress.confidence_score < 0.7); // confidence score degraded
}
