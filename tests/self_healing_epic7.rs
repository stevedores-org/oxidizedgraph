use oxidizedgraph::prelude::*;
use std::sync::{Arc, RwLock};

#[tokio::test]
async fn test_failure_classification() {
    assert_eq!(classify_failure("Compilation failed in main.rs:32"), FailureClass::Compile);
    assert_eq!(classify_failure("cargo build returned error status 101"), FailureClass::Compile);
    assert_eq!(classify_failure("test failed: assert_eq!(a, b)"), FailureClass::Test);
    assert_eq!(classify_failure("Connection refused to api.example.com"), FailureClass::Integration);
    assert_eq!(classify_failure("thread panicked at index out of bounds"), FailureClass::Runtime);
    assert_eq!(classify_failure("some weird error"), FailureClass::Unknown);
}

#[tokio::test]
async fn test_self_healing_retry_bounds() {
    // 1. Setup plan with a single task
    let mut plan = EpicPlan::new("test_goal");
    plan.add_task(Task::new("t1", "Task 1", "desc"));

    // Set custom retry policy for Compile errors: max 2 attempts
    let scheduler_node = SchedulerNode::new("scheduler")
        .with_retry_policy(FailureClass::Compile, RetryPolicy::new(2));

    // Simulate t1 failing due to Compile error
    plan.update_task_status("t1", TaskStatus::Failed);
    if let Some(t) = plan.get_task_mut("t1") {
        t.error = Some("Compilation failed at line 12".to_string());
    }

    let state = Arc::new(RwLock::new(AgentState::new()));
    {
        let mut guard = state.write().unwrap();
        guard.set_context("epic_plan", plan.clone());
    }

    // --- First attempt (attempt 1) ---
    let output1 = scheduler_node.execute(Arc::clone(&state)).await.unwrap();
    assert!(matches!(output1, NodeOutput::Transition(ref t) if t == "replan_injected"));

    {
        let guard = state.read().unwrap();
        let updated_plan = guard.get_context::<EpicPlan>("epic_plan").unwrap();
        // t1 should be pending again, depending on recovery_t1_1
        let t1 = updated_plan.get_task("t1").unwrap();
        assert_eq!(t1.status, TaskStatus::Pending);
        assert_eq!(t1.dependencies, vec!["recovery_t1_1".to_string()]);

        // recovery_t1_1 should be pending
        let rec = updated_plan.get_task("recovery_t1_1").unwrap();
        assert_eq!(rec.status, TaskStatus::Pending);

        // Check history
        let history = guard.get_context::<Vec<RecoveryRecord>>("recovery_history").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].task_id, "t1");
        assert_eq!(history[0].attempt, 1);
        assert_eq!(history[0].failure_class, FailureClass::Compile);
        assert_eq!(history[0].decision, "Inject recovery task");
    }

    // Simulate recovery_t1_1 completes, but t1 fails AGAIN
    {
        let mut guard = state.write().unwrap();
        let mut p = guard.get_context::<EpicPlan>("epic_plan").unwrap();
        p.update_task_status("recovery_t1_1", TaskStatus::Completed);
        p.update_task_status("t1", TaskStatus::Failed);
        if let Some(t) = p.get_task_mut("t1") {
            t.error = Some("Compilation failed at line 15".to_string());
        }
        guard.set_context("epic_plan", p);
    }

    // --- Second attempt (attempt 2) ---
    let output2 = scheduler_node.execute(Arc::clone(&state)).await.unwrap();
    assert!(matches!(output2, NodeOutput::Transition(ref t) if t == "replan_injected"));

    {
        let guard = state.read().unwrap();
        let updated_plan = guard.get_context::<EpicPlan>("epic_plan").unwrap();
        let t1 = updated_plan.get_task("t1").unwrap();
        assert_eq!(t1.dependencies, vec!["recovery_t1_2".to_string()]);

        let history = guard.get_context::<Vec<RecoveryRecord>>("recovery_history").unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].attempt, 2);
    }

    // Simulate recovery completes, t1 fails a THIRD time
    {
        let mut guard = state.write().unwrap();
        let mut p = guard.get_context::<EpicPlan>("epic_plan").unwrap();
        p.update_task_status("recovery_t1_2", TaskStatus::Completed);
        p.update_task_status("t1", TaskStatus::Failed);
        if let Some(t) = p.get_task_mut("t1") {
            t.error = Some("Compilation failed at line 20".to_string());
        }
        guard.set_context("epic_plan", p);
    }

    // --- Third attempt (reaches max limit of 2) ---
    let output3 = scheduler_node.execute(Arc::clone(&state)).await.unwrap();
    // Should transition to replan_needed (halt)
    assert!(matches!(output3, NodeOutput::Transition(ref t) if t == "replan_needed"));

    {
        let guard = state.read().unwrap();
        let history = guard.get_context::<Vec<RecoveryRecord>>("recovery_history").unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[2].decision, "Halted (max attempts)");
        assert!(history[2].rationale.contains("Exceeded max attempts"));
    }
}
