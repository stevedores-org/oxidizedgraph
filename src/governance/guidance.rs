//! Role-scoped guidance composition and state integration.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::state::{AgentState, Message};

use super::config::GovernanceConfig;
use super::parser::{blocks_for, parse_blocks};
use super::roles::AgentRole;

/// Context key storing the active [`AgentRole`] tag (e.g. `"builder"`).
pub const CTX_AGENT_ROLE: &str = "agent_role";
/// Context key storing composed manifest guidance for the active role.
pub const CTX_GOVERNANCE_GUIDANCE: &str = "governance_guidance";
/// Context key storing the role-specific system prompt preamble.
pub const CTX_ROLE_SYSTEM_PROMPT: &str = "role_system_prompt";
/// Context key storing a serialized [`crate::tools::ToolExecutionPolicy`] hint.
pub const CTX_TOOL_POLICY_ROLE: &str = "tool_policy_role";

/// Composed guidance for one agent role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleGuidance {
    /// Role the guidance was composed for.
    pub role: AgentRole,
    /// Concatenated block text, separated by blank lines.
    pub text: String,
    /// Number of manifest blocks included.
    pub block_count: usize,
}

/// Load a manifest from disk (typically `AGENTS.md`).
pub fn load_manifest(path: impl AsRef<Path>) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

/// Compose role-scoped guidance from an inline manifest string.
pub fn compose_guidance(manifest: &str, role: &AgentRole) -> RoleGuidance {
    let blocks = parse_blocks(manifest);
    let selected: Vec<_> = blocks_for(&blocks, role).collect();
    let text = selected
        .iter()
        .map(|b| b.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    RoleGuidance {
        role: role.clone(),
        text,
        block_count: selected.len(),
    }
}

/// Short role preamble injected ahead of manifest guidance.
pub fn role_system_prompt(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Architect => {
            "You are the Architect agent. Focus on design, API shape, and trade-offs. \
             Do not write implementation code unless asked."
        }
        AgentRole::Builder => {
            "You are the Builder agent. Implement changes, run local checks, and keep diffs focused."
        }
        AgentRole::Auditor => {
            "You are the Auditor agent. Review for correctness, security, and policy compliance."
        }
        AgentRole::Human => {
            "You are assisting a Human operator. Pause for approval on risky actions."
        }
        AgentRole::All => "You are a general-purpose agent operating under broadcast guidance.",
        AgentRole::Custom(_) => "You are a specialized agent operating under scoped guidance.",
    }
}

/// Read the active role from agent context.
pub fn agent_role_from_state(state: &AgentState) -> Option<AgentRole> {
    let tag: String = state.get_context(CTX_AGENT_ROLE)?;
    tag.parse().ok()
}

/// Apply role guidance and policy hints to agent state.
pub fn apply_role_guidance(
    state: &mut AgentState,
    manifest: &str,
    role: &AgentRole,
    config: &GovernanceConfig,
) {
    let guidance = compose_guidance(manifest, role);
    let preamble = role_system_prompt(role);
    let system_text = if guidance.text.is_empty() {
        preamble.to_string()
    } else {
        format!("{preamble}\n\n{}", guidance.text)
    };

    state.set_context(CTX_AGENT_ROLE, role.as_tag());
    state.set_context(CTX_GOVERNANCE_GUIDANCE, &guidance.text);
    state.set_context(CTX_ROLE_SYSTEM_PROMPT, preamble);

    if config.store_tool_policy {
        state.set_context(CTX_TOOL_POLICY_ROLE, role.as_tag());
    }

    if config.inject_system_message {
        state.messages.insert(0, Message::system(system_text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
<@all>
Shared rules.

<@builder>
Builder-only rules.
";

    #[test]
    fn compose_guidance_includes_broadcast_and_role_block() {
        let g = compose_guidance(SAMPLE, &AgentRole::Builder);
        assert_eq!(g.block_count, 2);
        assert!(g.text.contains("Shared rules"));
        assert!(g.text.contains("Builder-only rules"));
    }

    #[test]
    fn apply_role_guidance_sets_context_and_system_message() {
        let mut state = AgentState::new();
        apply_role_guidance(
            &mut state,
            SAMPLE,
            &AgentRole::Builder,
            &GovernanceConfig::permissive(),
        );
        assert_eq!(agent_role_from_state(&state), Some(AgentRole::Builder));
        assert!(state
            .get_context::<String>(CTX_GOVERNANCE_GUIDANCE)
            .unwrap()
            .contains("Builder-only"));
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, crate::state::MessageRole::System);
    }
}
