//! Governance configuration and per-role tool restrictions.

use serde::{Deserialize, Serialize};

use crate::tools::{Capability, ToolExecutionPolicy};

use super::roles::AgentRole;

/// Configuration for how a [`GovernanceNode`](super::node::GovernanceNode) applies guidance.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceConfig {
    /// When true, prepend a system message with composed role guidance.
    #[serde(default = "default_true")]
    pub inject_system_message: bool,
    /// When true, store a role-scoped [`ToolExecutionPolicy`] in agent context.
    #[serde(default = "default_true")]
    pub store_tool_policy: bool,
}

fn default_true() -> bool {
    true
}

impl GovernanceConfig {
    /// Permissive config for tests and examples.
    pub fn permissive() -> Self {
        Self {
            inject_system_message: true,
            store_tool_policy: true,
        }
    }

    /// Guidance-only: update context without injecting messages.
    pub fn context_only() -> Self {
        Self {
            inject_system_message: false,
            store_tool_policy: true,
        }
    }
}

/// Default tool restrictions for a governance role (fail-closed for unknown tools).
pub fn tool_policy_for_role(role: &AgentRole) -> ToolExecutionPolicy {
    match role {
        AgentRole::Architect => ToolExecutionPolicy::default()
            .allow_tool("read_file", vec![Capability::ReadFile])
            .deny_tool("shell")
            .deny_tool("write_file")
            .deny_tool("git_push"),
        AgentRole::Builder => ToolExecutionPolicy::default()
            .allow_tool("read_file", vec![Capability::ReadFile])
            .allow_tool("write_file", vec![Capability::WriteFile])
            .allow_tool("shell", vec![Capability::Shell])
            .allow_tool("git_status", vec![Capability::Git]),
        AgentRole::Auditor => ToolExecutionPolicy::default()
            .allow_tool("read_file", vec![Capability::ReadFile])
            .allow_tool("shell", vec![Capability::Shell]),
        AgentRole::Human => ToolExecutionPolicy::default()
            .allow_tool("read_file", vec![Capability::ReadFile])
            .require_approval("write_file")
            .require_approval("shell")
            .require_approval("git_push"),
        AgentRole::All | AgentRole::Custom(_) => ToolExecutionPolicy::permissive(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{PolicyDecision, ToolPolicyEngine};

    #[test]
    fn architect_cannot_invoke_shell() {
        let engine = ToolPolicyEngine::new(tool_policy_for_role(&AgentRole::Architect));
        assert_eq!(
            engine.evaluate("shell", None).unwrap(),
            PolicyDecision::Deny
        );
    }

    #[test]
    fn builder_may_invoke_shell() {
        let engine = ToolPolicyEngine::new(tool_policy_for_role(&AgentRole::Builder));
        assert_eq!(
            engine.evaluate("shell", None).unwrap(),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn human_shell_requires_approval() {
        let engine = ToolPolicyEngine::new(tool_policy_for_role(&AgentRole::Human));
        assert_eq!(
            engine.evaluate("shell", None).unwrap(),
            PolicyDecision::RequireApproval
        );
    }
}
