use oxidizedgraph::governance::{SymlinkManager, SymlinkStatus};
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
    use oxidizedgraph::tools::policy::ToolExecutionPolicy;

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

#[test]
fn test_manifest_tag_validation() {
    use oxidizedgraph::governance::{GovernanceValidator, ManifestError, ValidationError};

    let validator = GovernanceValidator::new(".");

    // Valid manifest
    let valid_manifest = "\
<@all>
Broadcast guidance text.

<@architect>
Architect guidance text.
";
    assert!(validator.validate_manifest_string(valid_manifest).is_ok());

    // Invalid manifest (malformed tags)
    let bad_manifest = "\
<@all>
Broadcast

<@builder!>
Bad tag body.

<@architect
Missing closing bracket.
";

    let result = validator.validate_manifest_string(bad_manifest);
    assert!(result.is_err());
    if let Err(ValidationError::Manifest(errors)) = result {
        assert_eq!(errors.len(), 2);
        assert!(matches!(
            errors[0],
            ManifestError::InvalidTag { line_number: 4, .. }
        ));
        assert!(matches!(
            errors[1],
            ManifestError::MalformedTagBoundary { line_number: 7, .. }
        ));
    } else {
        panic!("Expected ValidationError::Manifest");
    }
}

#[tokio::test]
async fn test_role_conditional_and_branch_routing() {
    use std::sync::{Arc, RwLock};

    // 1. Test ConditionalNode::role_is
    let cond =
        ConditionalNode::role_is("cond_role", AgentRole::Builder, "is_builder", "not_builder");

    // Builder role
    let mut state = AgentState::new();
    state.set_context("agent_role", "builder".to_string());
    let shared = Arc::new(RwLock::new(state));
    let out = cond.execute(shared).await.unwrap();
    assert_eq!(out.target(), Some("is_builder"));

    // Architect role
    let mut state = AgentState::new();
    state.set_context("agent_role", "architect".to_string());
    let shared = Arc::new(RwLock::new(state));
    let out = cond.execute(shared).await.unwrap();
    assert_eq!(out.target(), Some("not_builder"));

    // 2. Test BranchNode::branch_on_role
    let branch = BranchNode::new("branch_role", "fallback")
        .branch_on_role(AgentRole::Builder, "to_build")
        .branch_on_role(AgentRole::Architect, "to_design");

    let mut state = AgentState::new();
    state.set_context("agent_role", "architect".to_string());
    let shared = Arc::new(RwLock::new(state));
    let out = branch.execute(shared).await.unwrap();
    assert_eq!(out.target(), Some("to_design"));
}

#[tokio::test]
async fn test_llm_node_system_prompt_governance_integration() {
    use oxidizedgraph::nodes::llm::LLMResponse;
    use std::sync::{Arc, Mutex};

    struct MockProvider {
        captured_prompt: Arc<Mutex<Option<String>>>,
    }
    #[async_trait::async_trait]
    impl LLMProvider for MockProvider {
        async fn generate(&self, _m: &[Message], c: &LLMConfig) -> Result<LLMResponse, NodeError> {
            let mut guard = self.captured_prompt.lock().unwrap();
            *guard = c.system_prompt.clone();
            Ok(LLMResponse::text("Mock Output"))
        }
        fn name(&self) -> &str {
            "mock"
        }
    }

    let captured = Arc::new(Mutex::new(None));
    let provider = MockProvider {
        captured_prompt: captured.clone(),
    };
    let config = LLMConfig::default().system_prompt("user_system_prompt");
    let llm_node = LLMNode::new("llm", provider, config);

    let manifest = "\
<@builder>
Builder guidance text.
";

    // Setup governance nodes
    let gov_node = GovernanceNode::new("gov", manifest, AgentRole::Builder);

    let graph = GraphBuilder::new()
        .add_node(gov_node)
        .add_node(llm_node)
        .set_entry_point("gov")
        .add_edge("gov", "llm")
        .compile()
        .unwrap();

    let runner = GraphRunner::new(graph, RunnerConfig::default());
    let final_state = runner.invoke(AgentState::new()).await.unwrap();

    // Check captured prompt
    let prompt = captured.lock().unwrap().clone().unwrap();
    assert!(prompt.contains("You are the Builder agent"));
    assert!(prompt.contains("Builder guidance text"));
    assert!(prompt.contains("user_system_prompt"));

    // Verify state complete
    assert!(final_state.is_complete);
}

#[tokio::test]
async fn test_dynamic_tool_policy_architect() {
    use std::sync::{Arc, RwLock};

    let tool = FunctionTool::new("shell", "shell description", serde_json::json!({}), |_| {
        Ok("ok".into())
    });
    let registry = ToolRegistry::new().register(tool);

    // Create ToolNode with permissive config policy by default.
    let tool_node = ToolNode::with_config("tools", registry, ToolNodeConfig::default());

    // Case 1: Architect (restricted)
    let mut state = AgentState::new();
    state.set_context("agent_role", "architect".to_string());
    state
        .tool_calls
        .push(ToolCall::new("1", "shell", serde_json::json!({})));
    let shared = Arc::new(RwLock::new(state));

    tool_node.execute(shared.clone()).await.unwrap();
    let guard = shared.read().unwrap();
    assert!(guard.messages[0].content.contains("denied by policy"));
}

#[tokio::test]
async fn test_dynamic_tool_policy_builder() {
    use std::sync::{Arc, RwLock};

    // Case 2: Builder (allowed)
    let tool = FunctionTool::new("shell", "shell description", serde_json::json!({}), |_| {
        Ok("ok".into())
    });
    let registry = ToolRegistry::new().register(tool);
    let tool_node = ToolNode::with_config("tools", registry, ToolNodeConfig::default());

    let mut state = AgentState::new();
    state.set_context("agent_role", "builder".to_string());
    state
        .tool_calls
        .push(ToolCall::new("2", "shell", serde_json::json!({})));
    let shared = Arc::new(RwLock::new(state));

    tool_node.execute(shared.clone()).await.unwrap();
    let guard = shared.read().unwrap();
    assert!(guard.messages[0].content.contains("ok"));
}
