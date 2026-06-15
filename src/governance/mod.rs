//! Governance: role-scoped guidance parsed from the master manifest.
//!
//! This module is the foundation slice of [issue #31][issue] and the Phase 2
//! EPIC2 role-orchestration slice for [issue #18][roadmap]. It provides:
//!
//! - [`AgentRole`] and tag-block parsing (`parser`, `roles`)
//! - [`GovernanceNode`] for applying manifest guidance to [`AgentState`]
//! - Role routing primitives ([`RoleRouterNode`], [`RoleHandoffNode`])
//! - Per-role tool restrictions via [`tool_policy_for_role`]
//! - Symlink sync, agent discovery, and compliance validation (Epic #30)
//!
//! [issue]: https://github.com/stevedores-org/oxidizedgraph/issues/31
//! [roadmap]: https://github.com/stevedores-org/oxidizedgraph/issues/18
//!
//! # Quick start
//!
//! ```
//! use oxidizedgraph::governance::{parse_blocks, blocks_for, AgentRole};
//!
//! let manifest = "\
//! <@all>
//! Read AGENTS.md before changing anything.
//! <@builder>
//! Run local-ci before pushing.
//! ";
//!
//! let blocks = parse_blocks(manifest);
//! let builder_view: Vec<_> = blocks_for(&blocks, &AgentRole::Builder).collect();
//! // The builder sees both the broadcast and its own block, in source order.
//! assert_eq!(builder_view.len(), 2);
//! assert_eq!(builder_view[0].role, AgentRole::All);
//! assert_eq!(builder_view[1].role, AgentRole::Builder);
//! ```

pub mod config;
pub mod discovery;
pub mod guidance;
pub mod node;
pub mod parser;
pub mod roles;
pub mod routing;
pub mod symlinks;
pub mod validator;

pub use config::{tool_policy_for_role, GovernanceConfig};
pub use discovery::{AgentDiscovery, DiscoveredAgent};
pub use guidance::{
    agent_role_from_state, apply_role_guidance, compose_guidance, load_manifest,
    role_system_prompt, RoleGuidance, CTX_AGENT_ROLE, CTX_GOVERNANCE_GUIDANCE,
    CTX_ROLE_SYSTEM_PROMPT, CTX_TOOL_POLICY_ROLE,
};
pub use node::{GovernanceError, GovernanceNode};
pub use parser::{blocks_for, parse_blocks, TaggedBlock};
pub use roles::{AgentRole, RoleParseError};
pub use routing::{tool_policy_for_state, RoleHandoffNode, RoleRouterNode, RoleTargetRouterNode};
pub use symlinks::{SymlinkManager, SymlinkStatus, KNOWN_TARGETS};
pub use validator::{ComplianceReport, GovernanceValidator, ManifestError, ValidationError};
