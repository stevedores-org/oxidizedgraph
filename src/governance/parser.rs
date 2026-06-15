//! Tag-block parser for `AGENTS.md`-style governance manifests.
//!
//! The manifest is plain Markdown sprinkled with inline role tags like
//! `<@all>` or `<@builder>`. A tag opens a *block* whose body runs from
//! the line **after** the tag up to (but not including) the next role
//! tag, or to end-of-input. The same tag can appear multiple times in
//! one manifest; each occurrence becomes a separate [`TaggedBlock`].
//!
//! This parser is the source of truth for "which lines belong to which
//! role." It is deliberately minimal:
//!
//! * lossless — every non-tag line lands in exactly one block;
//! * order-preserving — blocks come back in manifest order;
//! * tag-greedy on the same line — a line that contains only a tag (and
//!   optional surrounding whitespace) becomes the boundary; a line that
//!   contains a tag mid-prose is treated as body, not a boundary.
//!
//! Higher layers (filtering by role, applying rule lookups) build on
//! [`parse_blocks`] without re-implementing the scan.

use crate::governance::roles::AgentRole;

/// One contiguous region of a manifest scoped to a single role.
///
/// `content` is the raw text between this tag boundary and the next,
/// with surrounding blank lines preserved so the block is byte-faithful.
/// Use `content.trim()` if you want the visually-trimmed form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedBlock {
    /// The role this block is scoped to.
    pub role: AgentRole,
    /// 1-indexed line in the source where the opening `<@tag>` lives.
    pub tag_line: usize,
    /// Lines belonging to this block (between this tag and the next).
    pub content: String,
}

/// Parse a manifest into role-scoped blocks.
///
/// Lines before the first tag are dropped — the parser only emits
/// blocks that have an explicit role attached. If you need a "preamble"
/// region, prepend an `<@all>` tag in the source.
///
/// Unparseable tags (e.g. `<@ >`, `<@bad char>`) are silently ignored
/// and their would-be opening line is treated as body of the previous
/// block. This is intentional — `<@ >` is more likely a typo than a
/// real boundary, and silently dropping it preserves the rest of the
/// manifest's structure for downstream consumers.
pub fn parse_blocks(input: &str) -> Vec<TaggedBlock> {
    let mut blocks: Vec<TaggedBlock> = Vec::new();
    let mut current: Option<TaggedBlock> = None;

    for (idx, line) in input.lines().enumerate() {
        let line_no = idx + 1;
        if let Some(role) = parse_tag_line(line) {
            // Tag boundary — flush the previous block (if any) and start a
            // fresh one. Empty trailing newlines on the previous block are
            // intentionally preserved so the source text round-trips.
            if let Some(prev) = current.take() {
                blocks.push(prev);
            }
            current = Some(TaggedBlock {
                role,
                tag_line: line_no,
                content: String::new(),
            });
        } else if let Some(block) = current.as_mut() {
            // Body line of the current block.
            if !block.content.is_empty() {
                block.content.push('\n');
            }
            block.content.push_str(line);
        }
        // Lines before the first tag have no owner; drop them.
    }
    if let Some(prev) = current {
        blocks.push(prev);
    }
    blocks
}

/// Iterate the blocks scoped to a specific role.
///
/// Semantics:
/// * `target == All` returns every block — `All` is the universal listener,
///   useful for diagnostics / dumping the manifest.
/// * A concrete `target` returns blocks tagged with that role plus
///   blocks tagged `<@all>` (the broadcast).
///
/// This is intentionally *more permissive* than `AgentRole::receives` for
/// the `All` case: when you ask "what blocks apply to role X?" you want
/// X's blocks plus broadcasts; when you ask "show me everything", you
/// expect everything.
pub fn blocks_for<'a>(
    blocks: &'a [TaggedBlock],
    target: &'a AgentRole,
) -> impl Iterator<Item = &'a TaggedBlock> {
    blocks.iter().filter(move |b| match target {
        AgentRole::All => true,
        other => b.role == AgentRole::All || &b.role == other,
    })
}

/// If `line` is *exactly* a role tag (optional surrounding whitespace
/// only), return the parsed role. A line like `prose <@builder> more
/// prose` returns `None` — the tag must own the line to count as a
/// block boundary, mirroring how `AGENTS.md` actually uses these.
///
/// Strict on the inner content: we strip the outer `<@`/`>` ourselves
/// and feed the result to [`AgentRole::from_inner`] rather than the
/// decoration-tolerant `FromStr`. This means a pathological input
/// like `<@<@builder>>` produces `None` (the inner `<@builder>` is
/// not a valid bare tag) instead of silently parsing as `Builder`.
fn parse_tag_line(line: &str) -> Option<AgentRole> {
    let trimmed = line.trim();
    if !trimmed.starts_with("<@") || !trimmed.ends_with('>') {
        return None;
    }
    let inner = &trimmed[2..trimmed.len() - 1];
    AgentRole::from_inner(inner).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_block() {
        let input = "<@all>\nhello\nworld\n";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].role, AgentRole::All);
        assert_eq!(blocks[0].tag_line, 1);
        assert_eq!(blocks[0].content, "hello\nworld");
    }

    #[test]
    fn splits_on_each_tag() {
        let input =
            "<@architect>\ndesign things\n<@builder>\nbuild things\n<@auditor>\naudit things\n";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 3);
        let roles: Vec<_> = blocks.iter().map(|b| b.role.clone()).collect();
        assert_eq!(
            roles,
            vec![AgentRole::Architect, AgentRole::Builder, AgentRole::Auditor]
        );
        assert_eq!(blocks[0].content, "design things");
        assert_eq!(blocks[1].content, "build things");
        assert_eq!(blocks[2].content, "audit things");
    }

    #[test]
    fn drops_preamble_before_first_tag() {
        // Anything before the first <@role> tag has no owner — drop it.
        let input = "preamble line one\npreamble line two\n<@builder>\nbody\n";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, "body");
    }

    #[test]
    fn preserves_block_order_with_repeated_tags() {
        let input = "<@all>\nintro\n<@builder>\nfirst builder\n<@all>\nbetween broadcast\n<@builder>\nsecond builder\n";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 4);
        assert_eq!(
            blocks.iter().map(|b| b.role.clone()).collect::<Vec<_>>(),
            vec![
                AgentRole::All,
                AgentRole::Builder,
                AgentRole::All,
                AgentRole::Builder
            ]
        );
        // The 4 contents must keep the source order intact.
        assert_eq!(blocks[0].content, "intro");
        assert_eq!(blocks[1].content, "first builder");
        assert_eq!(blocks[2].content, "between broadcast");
        assert_eq!(blocks[3].content, "second builder");
    }

    #[test]
    fn mid_line_tag_is_not_a_boundary() {
        // Real prose may mention a tag mid-sentence; that must not split
        // the block, otherwise documentation about tags would be unreadable.
        let input =
            "<@all>\nThe <@builder> role is for implementation.\n<@builder>\nactual builder body\n";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].role, AgentRole::All);
        assert!(
            blocks[0].content.contains("<@builder>"),
            "mid-line tag should stay in the @all block, got: {:?}",
            blocks[0].content
        );
        assert_eq!(blocks[1].role, AgentRole::Builder);
        assert_eq!(blocks[1].content, "actual builder body");
    }

    #[test]
    fn unknown_tags_become_custom_blocks() {
        let input = "<@codex>\ntool-specific\n<@sdlc>\ndomain-specific\n";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].role, AgentRole::Custom("codex".to_string()));
        assert_eq!(blocks[1].role, AgentRole::Custom("sdlc".to_string()));
    }

    #[test]
    fn malformed_tag_is_silently_ignored() {
        // <@ > is an empty tag; it should not become a boundary, and the
        // existing block continues unbroken.
        let input = "<@builder>\nfirst\n<@ >\nsecond\n";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].role, AgentRole::Builder);
        // The malformed line and the line after it both become body.
        assert!(blocks[0].content.contains("first"));
        assert!(blocks[0].content.contains("<@ >"));
        assert!(blocks[0].content.contains("second"));
    }

    #[test]
    fn blocks_for_filters_by_role_with_broadcast() {
        let input = "<@all>\nbroadcast\n<@builder>\nbuilder only\n<@auditor>\nauditor only\n";
        let blocks = parse_blocks(input);

        let builder_view: Vec<_> = blocks_for(&blocks, &AgentRole::Builder)
            .map(|b| b.content.as_str())
            .collect();
        assert_eq!(builder_view, vec!["broadcast", "builder only"]);

        let auditor_view: Vec<_> = blocks_for(&blocks, &AgentRole::Auditor)
            .map(|b| b.content.as_str())
            .collect();
        assert_eq!(auditor_view, vec!["broadcast", "auditor only"]);

        // Targeting `All` gives every block back.
        let all_view: Vec<_> = blocks_for(&blocks, &AgentRole::All)
            .map(|b| b.content.as_str())
            .collect();
        assert_eq!(all_view, vec!["broadcast", "builder only", "auditor only"]);
    }

    #[test]
    fn empty_input_yields_no_blocks() {
        assert!(parse_blocks("").is_empty());
        assert!(parse_blocks("\n\n\n").is_empty());
        assert!(parse_blocks("only prose, no tags\nat all\n").is_empty());
    }

    #[test]
    fn tag_line_numbers_are_one_indexed() {
        let input = "<@all>\nbody-line-2\n<@builder>\nbody-line-4\n";
        let blocks = parse_blocks(input);
        assert_eq!(blocks[0].tag_line, 1);
        assert_eq!(blocks[1].tag_line, 3);
    }

    /// Regression for the F5 finding on the WS1 follow-up CRR: `tag_line`
    /// must stay accurate across many blocks with varying body lengths,
    /// not just the trivial two-block case. Future `guidance.rs` slices
    /// will surface diagnostics keyed on this value.
    #[test]
    fn tag_line_tracks_across_multi_block_input() {
        let input = "\
<@all>
broadcast body 1
broadcast body 2
broadcast body 3
<@architect>
architect body
<@builder>


builder with leading blanks
<@auditor>
final block
";
        let blocks = parse_blocks(input);
        assert_eq!(blocks.len(), 4);
        let tag_lines: Vec<usize> = blocks.iter().map(|b| b.tag_line).collect();
        // Lines: <@all>=1, body=2,3,4, <@architect>=5, body=6, <@builder>=7,
        // blanks=8,9, body=10, <@auditor>=11, final=12.
        assert_eq!(tag_lines, vec![1, 5, 7, 11]);
    }

    /// Regression for the F2 finding on the WS1 follow-up CRR: a
    /// pathological double-wrapped tag like `<@<@builder>>` must NOT
    /// silently parse as `Builder`. The strict `from_inner` path rejects
    /// the inner `<@builder>` because it contains `<` / `>` characters
    /// that aren't allowed in a bare tag body.
    #[test]
    fn nested_tag_decoration_is_not_a_boundary() {
        let input = "<@builder>\nfirst\n<@<@builder>>\nsecond\n";
        let blocks = parse_blocks(input);
        // The malformed line is body, not a boundary — the single block
        // stays open and absorbs both `first` and `second`.
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].role, AgentRole::Builder);
        assert!(blocks[0].content.contains("first"));
        assert!(blocks[0].content.contains("<@<@builder>>"));
        assert!(blocks[0].content.contains("second"));
    }
}
