//! Integration tests for the governance foundation types (issue #31, slice 1).
//!
//! Unit tests for individual parsing rules live next to the source in
//! `src/governance/{roles,parser}.rs`. The tests here exercise the
//! public API surface (`oxidizedgraph::governance::*`) and validate
//! end-to-end behavior on inputs that look like real `AGENTS.md` files.

use oxidizedgraph::governance::{blocks_for, parse_blocks, AgentRole};

/// A realistic-shape manifest slice: broadcast preamble, then several
/// role-scoped blocks including tool tags this repo actually uses.
const SAMPLE_MANIFEST: &str = "\
<@all>
# Repo conventions
Run `local-ci` before pushing.

<@architect>
Bias toward the existing `NodeExecutor` trait. Don't add a parallel
abstraction for `GovernanceNode`.

<@builder>
Run `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings`
before each commit.

<@auditor>
Reject PRs that introduce new modules without doc comments
(`#![warn(missing_docs)]` is on at the crate root).

<@codex>
Tool-specific guidance for the Codex agent goes here.
";

#[test]
fn parses_real_shape_manifest_into_five_blocks() {
    let blocks = parse_blocks(SAMPLE_MANIFEST);
    assert_eq!(
        blocks.len(),
        5,
        "expected one block per <@role> tag, got {}",
        blocks.len()
    );
    let roles: Vec<_> = blocks.iter().map(|b| b.role.clone()).collect();
    assert_eq!(
        roles,
        vec![
            AgentRole::All,
            AgentRole::Architect,
            AgentRole::Builder,
            AgentRole::Auditor,
            AgentRole::Custom("codex".into()),
        ]
    );
}

#[test]
fn builder_view_includes_broadcast_and_own_block_only() {
    let blocks = parse_blocks(SAMPLE_MANIFEST);
    let view: Vec<_> = blocks_for(&blocks, &AgentRole::Builder).collect();

    assert_eq!(view.len(), 2, "builder should see <@all> + <@builder> only");
    assert_eq!(view[0].role, AgentRole::All);
    assert_eq!(view[1].role, AgentRole::Builder);
    assert!(view[1].content.contains("cargo fmt"));
}

#[test]
fn custom_role_view_does_not_pick_up_canonical_blocks() {
    let blocks = parse_blocks(SAMPLE_MANIFEST);
    let codex = AgentRole::Custom("codex".into());
    let view: Vec<_> = blocks_for(&blocks, &codex).collect();

    // Custom role should still see broadcasts, plus its own block.
    let role_seq: Vec<_> = view.iter().map(|b| b.role.clone()).collect();
    assert_eq!(role_seq, vec![AgentRole::All, codex.clone()]);
    // No leakage from <@builder>/<@architect>/<@auditor>.
    assert!(view
        .iter()
        .all(|b| b.role == AgentRole::All || b.role == codex));
}

#[test]
fn round_trip_role_tags_are_canonical() {
    // Every canonical role's Display form must parse back to the same role
    // — this is the contract `GovernanceNode` will rely on for routing.
    for role in [
        AgentRole::All,
        AgentRole::Architect,
        AgentRole::Builder,
        AgentRole::Auditor,
        AgentRole::Human,
    ] {
        let rendered = role.to_string();
        let parsed: AgentRole = rendered.parse().expect("canonical roles must round-trip");
        assert_eq!(role, parsed);
    }
}

#[test]
fn empty_manifest_yields_no_blocks() {
    assert!(parse_blocks("").is_empty());
    assert!(parse_blocks("\n\n").is_empty());
}

#[test]
fn parser_is_lossless_within_a_block() {
    // The exact bytes between two tag lines must survive parsing. This is
    // the contract that lets a future `guidance.rs` slice content out
    // verbatim (e.g. for diffing against the prior manifest).
    let manifest =
        "<@builder>\nline-A\n  indented\n\n  blank-line above\nline-B\n<@auditor>\nother\n";
    let blocks = parse_blocks(manifest);
    assert_eq!(blocks.len(), 2);
    assert_eq!(
        blocks[0].content,
        "line-A\n  indented\n\n  blank-line above\nline-B"
    );
}
