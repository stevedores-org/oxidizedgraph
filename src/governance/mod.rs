//! Governance: role-scoped guidance parsed from the master manifest.
//!
//! This module is the foundation slice of [issue #31][issue]. It provides
//! the parse layer used by every other governance feature: the
//! [`AgentRole`] enum and a tag-block parser that turns an `AGENTS.md`-
//! style manifest into role-scoped [`TaggedBlock`]s.
//!
//! Higher-level pieces (config loading, `GovernanceNode`, state
//! integration) build on these primitives in follow-up PRs.
//!
//! [issue]: https://github.com/stevedores-org/oxidizedgraph/issues/31
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

pub mod parser;
pub mod roles;

pub use parser::{blocks_for, parse_blocks, TaggedBlock};
pub use roles::{AgentRole, RoleParseError};
