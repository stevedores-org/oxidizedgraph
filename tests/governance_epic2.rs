//! Integration tests for EPIC2 role orchestration (#18, #35, #36).

use oxidizedgraph::governance::{
    agent_role_from_state, compose_guidance, tool_policy_for_role, AgentRole, GovernanceNode,
    RoleHandoffNode, RoleRouterNode, CTX_AGENT_ROLE, CTX_GOVERNANCE_GUIDANCE,
};
use oxidizedgraph::graph::{GraphBuilder, NodeExecutor, NodeOutput};
use oxidizedgraph::prelude::*;
use oxidizedgraph::state::AgentState;
use oxidizedgraph::tools::{PolicyDecision, ToolPolicyEngine};
use std::sync::{Arc, RwLock};

const MANIFEST: &str = "\
<@all>
Shared preamble.

<@architect>
Design only.

<@builder>
Implement and test.

<@auditor>
Review gates.
";

#[test]
fn epic2_compose_guidance_per_role() {
    let architect = compose_guidance(MANIFEST, &AgentRole::Architect);
    assert_eq!(architect.block_count, 2);
    assert!(architect.text.contains("Shared preamble"));
    assert!(architect.text.contains("Design only"));
    assert!(!architect.text.contains("Implement and test"));

    let auditor = compose_guidance(MANIFEST, &AgentRole::Auditor);
    assert!(auditor.text.contains("Review gates"));
}

#[tokio::test]
async fn epic2_governance_node_sets_role_context() {
    let node = GovernanceNode::new("gov", MANIFEST, AgentRole::Builder);
    let state = Arc::new(RwLock::new(AgentState::new()));
    node.execute(state.clone()).await.unwrap();

    let guard = state.read().unwrap();
    assert_eq!(agent_role_from_state(&guard), Some(AgentRole::Builder));
    assert!(guard
        .get_context::<String>(CTX_GOVERNANCE_GUIDANCE)
        .unwrap()
        .contains("Implement and test"));
}

#[tokio::test]
async fn epic2_handoff_switches_role() {
    let gov = GovernanceNode::new("start", MANIFEST, AgentRole::Architect);
    let handoff = RoleHandoffNode::new("handoff", MANIFEST, AgentRole::Builder);
    let state = Arc::new(RwLock::new(AgentState::new()));

    gov.execute(state.clone()).await.unwrap();
    handoff.execute(state.clone()).await.unwrap();

    let guard = state.read().unwrap();
    assert_eq!(agent_role_from_state(&guard), Some(AgentRole::Builder));
}

#[tokio::test]
async fn epic2_role_router_transition_key() {
    let router = RoleRouterNode::new("route", "default");
    let mut state = AgentState::new();
    state.set_context(CTX_AGENT_ROLE, "auditor");
    let shared = Arc::new(RwLock::new(state));

    let output = router.execute(shared).await.unwrap();
    assert_eq!(output.target(), Some("auditor"));
}

#[test]
fn epic2_tool_policy_enforces_role_restrictions() {
    let architect = ToolPolicyEngine::new(tool_policy_for_role(&AgentRole::Architect));
    assert_eq!(
        architect.evaluate("shell", None).unwrap(),
        PolicyDecision::Deny
    );

    let builder = ToolPolicyEngine::new(tool_policy_for_role(&AgentRole::Builder));
    assert_eq!(
        builder.evaluate("shell", None).unwrap(),
        PolicyDecision::Allow
    );
}

#[tokio::test]
async fn epic2_graph_walks_architect_to_auditor() {
    struct MarkNode {
        id: &'static str,
        key: &'static str,
    }

    #[async_trait::async_trait]
    impl NodeExecutor for MarkNode {
        fn id(&self) -> &str {
            self.id
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context(self.key, true);
            Ok(NodeOutput::cont())
        }
    }

    let graph = GraphBuilder::new()
        .add_node(GovernanceNode::new("gov", MANIFEST, AgentRole::Architect))
        .add_node(MarkNode {
            id: "plan",
            key: "planned",
        })
        .add_node(RoleHandoffNode::new(
            "to_builder",
            MANIFEST,
            AgentRole::Builder,
        ))
        .add_node(MarkNode {
            id: "build",
            key: "built",
        })
        .add_node(RoleHandoffNode::new(
            "to_auditor",
            MANIFEST,
            AgentRole::Auditor,
        ))
        .add_node(RoleRouterNode::new("route", "default"))
        .add_node(MarkNode {
            id: "audit",
            key: "audited",
        })
        .set_entry_point("gov")
        .add_edge("gov", "plan")
        .add_edge("plan", "to_builder")
        .add_edge("to_builder", "build")
        .add_edge("build", "to_auditor")
        .add_edge("to_auditor", "route")
        .add_edge_with_key("route", "audit", "auditor")
        .compile()
        .unwrap();

    let runner = GraphRunner::new(graph, RunnerConfig::default());
    let result = runner.invoke(AgentState::new()).await.unwrap();

    assert!(result.get_context::<bool>("planned").unwrap());
    assert!(result.get_context::<bool>("built").unwrap());
    assert!(result.get_context::<bool>("audited").unwrap());
    assert_eq!(agent_role_from_state(&result), Some(AgentRole::Auditor));
}
