//! Role-based routing primitives for governance-aware graphs.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::{AgentState, SharedState};

use super::config::{tool_policy_for_role, GovernanceConfig};
use super::guidance::{agent_role_from_state, apply_role_guidance, CTX_AGENT_ROLE};
use super::roles::AgentRole;

/// Routes execution using a graph transition key derived from `agent_role` context.
///
/// Wire edges with `add_edge_with_key(from, to, role_tag)` where `role_tag` matches
/// [`AgentRole::as_tag`].
#[derive(Clone, Debug)]
pub struct RoleRouterNode {
    id: String,
    /// Transition key used when `agent_role` is missing from context.
    default_transition: String,
}

impl RoleRouterNode {
    /// Create a router that falls back to `default_transition` when no role is set.
    pub fn new(id: impl Into<String>, default_transition: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            default_transition: default_transition.into(),
        }
    }
}

#[async_trait]
impl NodeExecutor for RoleRouterNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let guard = state
            .read()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let transition = agent_role_from_state(&guard)
            .map(|r| r.as_tag().to_string())
            .unwrap_or_else(|| self.default_transition.clone());

        Ok(NodeOutput::transition(transition))
    }

    fn description(&self) -> Option<&str> {
        Some("Routes via transition key derived from agent_role context")
    }
}

/// Hand off execution to a new role, re-applying manifest guidance and tool policy hints.
#[derive(Clone, Debug)]
pub struct RoleHandoffNode {
    id: String,
    manifest: String,
    next_role: AgentRole,
    config: GovernanceConfig,
}

impl RoleHandoffNode {
    /// Create a handoff node with an inline manifest.
    pub fn new(id: impl Into<String>, manifest: impl Into<String>, next_role: AgentRole) -> Self {
        Self {
            id: id.into(),
            manifest: manifest.into(),
            next_role,
            config: GovernanceConfig::permissive(),
        }
    }

    /// Override governance application behavior.
    pub fn with_config(mut self, config: GovernanceConfig) -> Self {
        self.config = config;
        self
    }
}

#[async_trait]
impl NodeExecutor for RoleHandoffNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;
        apply_role_guidance(&mut guard, &self.manifest, &self.next_role, &self.config);
        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Switches agent_role and reapplies governance guidance")
    }
}

/// Resolve the tool policy for the active role in state (or a fallback role).
pub fn tool_policy_for_state(
    state: &AgentState,
    fallback: &AgentRole,
) -> crate::tools::ToolExecutionPolicy {
    let role = agent_role_from_state(state).unwrap_or_else(|| fallback.clone());
    tool_policy_for_role(&role)
}

/// Builder for routing to explicit node targets by role tag (continue-style routing).
#[derive(Clone, Debug, Default)]
pub struct RoleTargetRouterNode {
    id: String,
    routes: HashMap<String, String>,
    default_target: Option<String>,
}

impl RoleTargetRouterNode {
    /// Create a router keyed on `agent_role` context.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            routes: HashMap::new(),
            default_target: None,
        }
    }

    /// Route a role tag to a node id.
    pub fn route(mut self, role: &AgentRole, target: impl Into<String>) -> Self {
        self.routes.insert(role.as_tag().to_string(), target.into());
        self
    }

    /// Target when no role matches.
    pub fn default_target(mut self, target: impl Into<String>) -> Self {
        self.default_target = Some(target.into());
        self
    }
}

#[async_trait]
impl NodeExecutor for RoleTargetRouterNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let guard = state
            .read()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let tag: Option<String> = guard.get_context(CTX_AGENT_ROLE);
        if let Some(tag) = tag {
            if let Some(target) = self.routes.get(&tag) {
                return Ok(NodeOutput::continue_to(target.clone()));
            }
        }

        if let Some(ref default) = self.default_target {
            Ok(NodeOutput::continue_to(default.clone()))
        } else {
            Ok(NodeOutput::finish())
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Routes to explicit node targets based on agent_role")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;
    use std::sync::{Arc, RwLock};

    #[tokio::test]
    async fn role_router_emits_builder_transition() {
        let node = RoleRouterNode::new("router", "default");
        let mut state = AgentState::new();
        state.set_context(CTX_AGENT_ROLE, "builder");
        let shared = Arc::new(RwLock::new(state));

        let output = node.execute(shared).await.unwrap();
        assert_eq!(output.target(), Some("builder"));
    }

    #[tokio::test]
    async fn role_target_router_routes_auditor() {
        let node = RoleTargetRouterNode::new("pick")
            .route(&AgentRole::Auditor, "audit_lane")
            .default_target("fallback");
        let mut state = AgentState::new();
        state.set_context(CTX_AGENT_ROLE, "auditor");
        let shared = Arc::new(RwLock::new(state));

        let output = node.execute(shared).await.unwrap();
        assert_eq!(output.target(), Some("audit_lane"));
    }
}
