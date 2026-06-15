//! Agent roles for governance tag routing.
//!
//! The repo's `AGENTS.md` master manifest uses inline tags such as `<@all>`,
//! `<@architect>`, and `<@builder>` to scope a block of guidance to one or
//! more agent roles. This module defines the canonical [`AgentRole`] enum
//! and the parse helpers that turn a raw tag name (the part between `<@`
//! and `>`) into a typed role.
//!
//! The four roles in the issue spec (`Architect`, `Builder`, `Auditor`,
//! `Human`) are first-class variants. Everything else — tool tags like
//! `<@codex>` / `<@gemini>`, domain tags like `<@sdlc>`, the broadcast
//! `<@all>` — is preserved as either [`AgentRole::All`] or
//! [`AgentRole::Custom`] so the parser is lossless against the real
//! manifest. Higher layers decide what to do with each role.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A governance-scoped agent role parsed from an `<@role>` tag.
///
/// `All` is the broadcast tag (`<@all>`). The four named variants are the
/// canonical roles called out in issue #31; `Custom` carries any other
/// tag (tool names, domain tags) verbatim so no information is lost.
///
/// # Note on `Auditor` / `Human`
///
/// The four canonical variants come from the issue #31 *spec*, not from
/// the current `AGENTS.md`. The live manifest exercises `<@all>`,
/// `<@architect>`, and `<@builder>` plus several tool/domain tags
/// (`<@codex>`, `<@gemini>`, `<@sdlc>`, …); `Auditor` and `Human` are
/// modelled here for forward use by `GovernanceNode` rule lookups, not
/// because they appear in-tree today.
///
/// The `Hash` derive is intentional: a follow-up `guidance.rs` slice
/// will use `AgentRole` as a key in rule / dispatch tables.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// `<@all>` — guidance applies to every agent role.
    All,
    /// `<@architect>` — design / planning / API-shape decisions.
    Architect,
    /// `<@builder>` — implementation / refactor / bugfix.
    Builder,
    /// `<@auditor>` — review / verification / quality gates.
    Auditor,
    /// `<@human>` — explicitly addressed to the human-in-the-loop.
    Human,
    /// Any other tag (e.g. `<@codex>`, `<@sdlc>`). The string is the
    /// lowercase tag content with no `<@` / `>` decoration.
    Custom(String),
}

impl AgentRole {
    /// Returns the canonical lowercase tag name for this role, without the
    /// surrounding `<@` / `>`.
    pub fn as_tag(&self) -> &str {
        match self {
            Self::All => "all",
            Self::Architect => "architect",
            Self::Builder => "builder",
            Self::Auditor => "auditor",
            Self::Human => "human",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// True if `self` should receive guidance addressed to `target`.
    ///
    /// Routing is asymmetric: the broadcast role `All` matches every
    /// concrete role *when it is the target*, but a concrete role does
    /// **not** match `All` when `All` is the listener. This mirrors how
    /// `<@all>` is used in `AGENTS.md`: blocks tagged `<@all>` reach
    /// every agent, but a `<@builder>` block does not also leak into an
    /// `All`-scoped subscriber.
    pub fn receives(&self, target: &AgentRole) -> bool {
        target == &AgentRole::All || self == target
    }
}

impl fmt::Display for AgentRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<@{}>", self.as_tag())
    }
}

/// Error returned when a tag string can't be coerced into an [`AgentRole`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoleParseError {
    /// The input was empty or contained only whitespace.
    #[error("agent role tag is empty")]
    Empty,
    /// The input contained characters not allowed in a tag (whitespace,
    /// angle brackets, etc.). Tags must be `[a-z0-9_-]+` after lowercasing.
    #[error("agent role tag '{0}' contains invalid characters (allowed: a-z, 0-9, _, -)")]
    InvalidCharacter(String),
}

impl AgentRole {
    /// Parse an *undecorated* tag body (e.g. `"builder"`, `"codex"`).
    ///
    /// This is the strict counterpart to [`FromStr::from_str`], which
    /// accepts decorated forms (`<@builder>`, `@builder`). Use this when
    /// the caller has already stripped decoration and would rather treat
    /// any remaining `<@`/`@`/`>` characters as invalid input than have
    /// them silently re-stripped. `parse_tag_line` in the sibling parser
    /// module relies on this strictness — it lets a malformed input like
    /// `<@<@builder>>` produce no boundary instead of silently parsing as
    /// `Builder`.
    pub fn from_inner(s: &str) -> Result<Self, RoleParseError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(RoleParseError::Empty);
        }
        let lower = trimmed.to_ascii_lowercase();
        if !lower
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(RoleParseError::InvalidCharacter(lower));
        }

        Ok(match lower.as_str() {
            "all" => Self::All,
            "architect" => Self::Architect,
            "builder" => Self::Builder,
            "auditor" => Self::Auditor,
            "human" => Self::Human,
            _ => Self::Custom(lower),
        })
    }
}

impl FromStr for AgentRole {
    type Err = RoleParseError;

    /// Parse a tag from its raw form — accepts the inner name (`builder`)
    /// or the fully-tagged form (`<@builder>` / `@builder`), case
    /// insensitively. Unknown but well-formed tags become
    /// [`AgentRole::Custom`]. Strict callers that have already stripped
    /// decoration should prefer [`AgentRole::from_inner`] to avoid
    /// double-stripping malformed inputs.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(RoleParseError::Empty);
        }
        // Strip optional `<@...>` or `@...` decoration so callers can pass
        // either form. Once the outermost layer is gone, defer to the
        // strict `from_inner` so nested decoration doesn't get peeled
        // a second time.
        let inner = trimmed
            .strip_prefix("<@")
            .and_then(|s| s.strip_suffix('>'))
            .or_else(|| trimmed.strip_prefix('@'))
            .unwrap_or(trimmed);
        Self::from_inner(inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_roles() {
        assert_eq!("all".parse::<AgentRole>().unwrap(), AgentRole::All);
        assert_eq!(
            "architect".parse::<AgentRole>().unwrap(),
            AgentRole::Architect
        );
        assert_eq!("builder".parse::<AgentRole>().unwrap(), AgentRole::Builder);
        assert_eq!("auditor".parse::<AgentRole>().unwrap(), AgentRole::Auditor);
        assert_eq!("human".parse::<AgentRole>().unwrap(), AgentRole::Human);
    }

    #[test]
    fn accepts_decorated_forms() {
        assert_eq!(
            "<@builder>".parse::<AgentRole>().unwrap(),
            AgentRole::Builder
        );
        assert_eq!("@builder".parse::<AgentRole>().unwrap(), AgentRole::Builder);
        assert_eq!(
            "  @architect  ".parse::<AgentRole>().unwrap(),
            AgentRole::Architect
        );
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(
            "Architect".parse::<AgentRole>().unwrap(),
            AgentRole::Architect
        );
        assert_eq!("BUILDER".parse::<AgentRole>().unwrap(), AgentRole::Builder);
        assert_eq!("<@HUMAN>".parse::<AgentRole>().unwrap(), AgentRole::Human);
    }

    #[test]
    fn preserves_unknown_tags_as_custom() {
        let codex: AgentRole = "<@codex>".parse().unwrap();
        assert_eq!(codex, AgentRole::Custom("codex".to_string()));
        let sdlc: AgentRole = "sdlc".parse().unwrap();
        assert_eq!(sdlc, AgentRole::Custom("sdlc".to_string()));
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert_eq!("".parse::<AgentRole>(), Err(RoleParseError::Empty));
        assert_eq!("   ".parse::<AgentRole>(), Err(RoleParseError::Empty));
        assert_eq!("<@>".parse::<AgentRole>(), Err(RoleParseError::Empty));
    }

    #[test]
    fn rejects_invalid_characters() {
        // Spaces, uppercase non-ASCII, punctuation in the tag body.
        assert!(matches!(
            "two words".parse::<AgentRole>(),
            Err(RoleParseError::InvalidCharacter(_))
        ));
        assert!(matches!(
            "build.er".parse::<AgentRole>(),
            Err(RoleParseError::InvalidCharacter(_))
        ));
    }

    #[test]
    fn display_round_trips() {
        for role in [
            AgentRole::All,
            AgentRole::Architect,
            AgentRole::Builder,
            AgentRole::Auditor,
            AgentRole::Human,
            AgentRole::Custom("codex".to_string()),
        ] {
            // `<@role>` formatting parses back to the same role.
            let rendered = role.to_string();
            let parsed: AgentRole = rendered.parse().unwrap();
            assert_eq!(role, parsed, "round-trip failed for {role}");
        }
    }

    #[test]
    fn receives_routing_semantics() {
        // Every concrete role receives broadcasts addressed to `All`.
        for role in [
            AgentRole::Architect,
            AgentRole::Builder,
            AgentRole::Auditor,
            AgentRole::Human,
            AgentRole::Custom("codex".into()),
        ] {
            assert!(
                role.receives(&AgentRole::All),
                "{role} should receive <@all>"
            );
        }
        // A concrete tag does NOT leak into a different concrete subscriber.
        assert!(!AgentRole::Builder.receives(&AgentRole::Architect));
        // `All` subscribers do NOT pick up concrete-role traffic — that
        // would defeat the point of role-scoped guidance.
        assert!(!AgentRole::All.receives(&AgentRole::Builder));
    }

    #[test]
    fn from_inner_is_strict_about_decoration() {
        // `from_inner` is the strict variant called from `parse_tag_line`
        // after the outer `<@`/`>` has already been stripped. Any leftover
        // decoration must be rejected — that's what prevents pathological
        // inputs like `<@<@builder>>` from silently parsing as `Builder`.
        assert!(AgentRole::from_inner("<@builder>").is_err());
        assert!(AgentRole::from_inner("@builder").is_err());
        // Bare names still work.
        assert_eq!(
            AgentRole::from_inner("builder").unwrap(),
            AgentRole::Builder
        );
        assert_eq!(
            AgentRole::from_inner("codex").unwrap(),
            AgentRole::Custom("codex".into())
        );
        // Empty / whitespace-only still empty.
        assert_eq!(
            AgentRole::from_inner("").unwrap_err(),
            RoleParseError::Empty
        );
        assert_eq!(
            AgentRole::from_inner("   ").unwrap_err(),
            RoleParseError::Empty
        );
    }
}
