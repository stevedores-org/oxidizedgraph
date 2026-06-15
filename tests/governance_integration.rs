use oxidizedgraph::governance::{SymlinkManager, SymlinkStatus, KNOWN_TARGETS};
use oxidizedgraph::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_symlink_creation_and_validation() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    // Create master AGENTS.md
    let master_path = base.join("AGENTS.md");
    fs::write(&master_path, "Master Content").unwrap();

    let mgr = SymlinkManager::default(base);

    // Initial state: missing
    assert_eq!(
        mgr.check_status(std::path::Path::new("CLAUDE.md")).unwrap(),
        SymlinkStatus::Missing
    );

    // Sync all
    mgr.sync_all().unwrap();

    // After sync: Valid
    assert_eq!(
        mgr.check_status(std::path::Path::new("CLAUDE.md")).unwrap(),
        SymlinkStatus::Valid
    );

    // Verify it's a symlink pointing to the right place
    let claude_path = base.join("CLAUDE.md");
    assert!(claude_path.is_symlink());

    // Verify contents
    assert_eq!(fs::read_to_string(&claude_path).unwrap(), "Master Content");
}

#[tokio::test]
async fn test_governance_node_enforces_rules_in_tool_node() {
    // This integration test verifies that tool nodes respect the governance rule
    // Builder should be able to run safe tools but not forbidden ones if policy denies it
    use oxidizedgraph::tools::policy::{ToolExecutionPolicy, ToolPolicyEngine};

    let mut state = AgentState::new();
    // Simulate setting a policy in state (which GovernanceNode would do)
    state.set_context(
        oxidizedgraph::governance::CTX_TOOL_POLICY_ROLE,
        ToolExecutionPolicy::permissive().deny_tool("rm"),
    );

    let shared = std::sync::Arc::new(std::sync::RwLock::new(state));

    let tool = FunctionTool::new("rm", "remove", serde_json::json!({}), |_| Ok("ok".into()));
    let registry = ToolRegistry::new().register(tool);
    // ToolNode without explicit config policy, should pick up from state
    let node = ToolNode::new("tools", registry);

    {
        let mut guard = shared.write().unwrap();
        guard
            .tool_calls
            .push(ToolCall::new("1", "rm", serde_json::json!({})));
    }

    node.execute(shared.clone()).await.unwrap();

    let guard = shared.read().unwrap();
    let last_msg = &guard.messages.last().unwrap().content;
    assert!(
        last_msg.contains("Policy denied"),
        "Message was: {}",
        last_msg
    );
}
