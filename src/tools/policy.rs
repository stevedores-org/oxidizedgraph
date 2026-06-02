//! Command and tool policy engine for bounded-risk execution.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Capability required to invoke a tool.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Read files from the workspace
    ReadFile,
    /// Write or modify files
    WriteFile,
    /// Execute shell commands
    Shell,
    /// Network access
    Network,
    /// Git operations (commit, push)
    Git,
}

/// Policy decision for a tool invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Allow execution
    Allow,
    /// Deny execution
    Deny,
    /// Require human approval before execution
    RequireApproval,
}

/// Policy violation with diagnosable context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyViolation {
    /// Tool name
    pub tool_name: String,
    /// Reason for denial
    pub reason: String,
    /// Required capability that was missing or blocked
    pub capability: Option<Capability>,
}

impl PolicyViolation {
    /// Format for logs and agent messages.
    pub fn message(&self) -> String {
        match &self.capability {
            Some(cap) => format!(
                "Policy denied tool '{}': {} (capability: {:?})",
                self.tool_name, self.reason, cap
            ),
            None => format!("Policy denied tool '{}': {}", self.tool_name, self.reason),
        }
    }
}

/// Per-tool execution policy.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ToolExecutionPolicy {
    /// Capabilities granted to tools by name (default: empty = deny unknown)
    pub tool_capabilities: std::collections::HashMap<String, Vec<Capability>>,
    /// Tool names explicitly denied
    pub denied_tools: HashSet<String>,
    /// Tool names requiring human approval
    pub approval_required: HashSet<String>,
    /// Commands or substrings blocked for shell tools
    pub blocked_patterns: Vec<String>,
}

impl ToolExecutionPolicy {
    /// Permissive policy for tests and local development.
    pub fn permissive() -> Self {
        Self {
            tool_capabilities: std::collections::HashMap::from([(
                "shell".to_string(),
                vec![Capability::Shell],
            )]),
            denied_tools: HashSet::new(),
            approval_required: HashSet::new(),
            blocked_patterns: vec!["rm -rf /".to_string(), "sudo ".to_string()],
        }
    }

    /// Register capabilities for a tool.
    pub fn allow_tool(mut self, name: impl Into<String>, caps: Vec<Capability>) -> Self {
        self.tool_capabilities.insert(name.into(), caps);
        self
    }

    /// Deny a tool by name.
    pub fn deny_tool(mut self, name: impl Into<String>) -> Self {
        self.denied_tools.insert(name.into());
        self
    }

    /// Require approval for a tool.
    pub fn require_approval(mut self, name: impl Into<String>) -> Self {
        self.approval_required.insert(name.into());
        self
    }
}

/// Evaluates tool calls against execution policy.
#[derive(Clone, Debug, Default)]
pub struct ToolPolicyEngine {
    policy: ToolExecutionPolicy,
}

impl ToolPolicyEngine {
    /// Create an engine with the given policy.
    pub fn new(policy: ToolExecutionPolicy) -> Self {
        Self { policy }
    }

    /// Evaluate whether a tool may run.
    pub fn evaluate(&self, tool_name: &str, command_hint: Option<&str>) -> Result<PolicyDecision, PolicyViolation> {
        if self.policy.denied_tools.contains(tool_name) {
            return Err(PolicyViolation {
                tool_name: tool_name.to_string(),
                reason: "tool is explicitly denied".to_string(),
                capability: None,
            });
        }

        if self.policy.approval_required.contains(tool_name) {
            return Ok(PolicyDecision::RequireApproval);
        }

        if let Some(cmd) = command_hint {
            for pattern in &self.policy.blocked_patterns {
                if cmd.contains(pattern) {
                    return Err(PolicyViolation {
                        tool_name: tool_name.to_string(),
                        reason: format!("command matches blocked pattern '{pattern}'"),
                        capability: Some(Capability::Shell),
                    });
                }
            }
        }

        if self.policy.tool_capabilities.contains_key(tool_name) {
            return Ok(PolicyDecision::Allow);
        }

        // Unknown tools denied by default (fail closed)
        Err(PolicyViolation {
            tool_name: tool_name.to_string(),
            reason: "tool not registered in policy (fail closed)".to_string(),
            capability: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deny_unknown_tool() {
        let engine = ToolPolicyEngine::new(ToolExecutionPolicy::default());
        let result = engine.evaluate("unknown_tool", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_block_dangerous_command() {
        let engine = ToolPolicyEngine::new(ToolExecutionPolicy::permissive());
        let result = engine.evaluate("shell", Some("sudo rm -rf /"));
        assert!(result.is_err());
    }

    #[test]
    fn test_require_approval() {
        let policy = ToolExecutionPolicy::permissive().require_approval("git_push");
        let engine = ToolPolicyEngine::new(policy);
        assert_eq!(
            engine.evaluate("git_push", None).unwrap(),
            PolicyDecision::RequireApproval
        );
    }
}
