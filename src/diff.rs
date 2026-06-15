//! Structural diff between two [`CompiledGraph`]s.
//!
//! Compares two compiled graphs and surfaces additions, removals, and changes
//! at the topology layer — nodes, edges, entry point, and graph metadata
//! (name + description). All diff types are [`serde::Serialize`] /
//! [`serde::Deserialize`] so consumers (e.g. an `aivcs` CLI that emits PR-body
//! summaries) can render them as JSON or Markdown without parsing internal
//! crate types.
//!
//! # What this module does NOT cover
//!
//! Prompts live inside `NodeExecutor` implementations (e.g. `LLMNode`'s
//! `LLMConfig::system_prompt`). The [`NodeExecutor`](crate::graph::NodeExecutor)
//! trait is opaque from here — this module only sees a `GraphNode`'s `id` and
//! the optional `description()` exposed by the trait. Surfacing semantic
//! prompt deltas requires either:
//!
//! 1. Adding an optional `prompt(&self) -> Option<&str>` method to
//!    `NodeExecutor` (default `None`), and a `NodeChange::PromptChanged`
//!    variant here, or
//! 2. A separate `PromptCarrier` trait that node implementations opt into
//!    and that downstream consumers downcast against.
//!
//! Tracked as a follow-up to issue #60.

use crate::graph::{transitions, CompiledGraph, EdgeType, GraphEdge};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A structural delta between two [`CompiledGraph`]s.
///
/// An empty `GraphDiff` (every field empty / `None`) means the two graphs are
/// structurally equal at the layer this module observes. All inner `Vec`s are
/// sorted by stable identifiers so the serialized form is deterministic across
/// runs — important for PR-body summaries that get diffed against history.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphDiff {
    /// Per-node additions, removals, and description changes.
    pub node_changes: Vec<NodeChange>,
    /// Per-edge additions, removals, and replacements.
    pub edge_changes: Vec<EdgeChange>,
    /// Set when the entry-point node id changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point: Option<EntryPointChange>,
    /// Set when the graph's name and/or description changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataChange>,
}

impl GraphDiff {
    /// Returns true when there are no changes at all.
    pub fn is_empty(&self) -> bool {
        self.node_changes.is_empty()
            && self.edge_changes.is_empty()
            && self.entry_point.is_none()
            && self.metadata.is_none()
    }

    /// Coarse classification of the diff for one-line operator output.
    pub fn summary(&self) -> ChangeSummary {
        let mut s = ChangeSummary::default();
        for c in &self.node_changes {
            match c {
                NodeChange::Added { .. } => s.nodes_added += 1,
                NodeChange::Removed { .. } => s.nodes_removed += 1,
                NodeChange::DescriptionChanged { .. } => s.nodes_changed += 1,
            }
        }
        for c in &self.edge_changes {
            match c {
                EdgeChange::Added(_) => s.edges_added += 1,
                EdgeChange::Removed(_) => s.edges_removed += 1,
                EdgeChange::Replaced { .. } => s.edges_replaced += 1,
            }
        }
        s.entry_changed = self.entry_point.is_some();
        s.metadata_changed = self.metadata.is_some();
        s
    }
}

/// Per-node change, identified by node `id`.
///
/// Semantic prompt changes are intentionally absent (see module docs); a
/// `DescriptionChanged` is the closest the current `NodeExecutor` trait
/// surface allows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeChange {
    /// Node id present in `after` but not `before`.
    Added {
        /// Node identifier.
        id: String,
        /// `NodeExecutor::description()` at the time of the diff.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Node id present in `before` but not `after`.
    Removed {
        /// Node identifier.
        id: String,
        /// `NodeExecutor::description()` at the time of the diff.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Node id present in both, with a changed `NodeExecutor::description()`.
    DescriptionChanged {
        /// Node identifier.
        id: String,
        /// Description in the `before` graph.
        before: Option<String>,
        /// Description in the `after` graph.
        after: Option<String>,
    },
}

impl NodeChange {
    fn id(&self) -> &str {
        match self {
            Self::Added { id, .. }
            | Self::Removed { id, .. }
            | Self::DescriptionChanged { id, .. } => id,
        }
    }
}

/// Per-edge change. Edges are matched between graphs by their
/// `(from, transition_key)` pair, mirroring the runtime's edge-lookup logic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EdgeChange {
    /// An edge present in `after` but not `before`.
    Added(EdgeShape),
    /// An edge present in `before` but not `after`.
    Removed(EdgeShape),
    /// An edge with the same `(from, transition_key)` whose destination or
    /// type changed — e.g. `direct → A` became `direct → B`, or a direct edge
    /// became conditional.
    Replaced {
        /// Source node id.
        from: String,
        /// Transition key (`None` is the default-CONTINUE path).
        #[serde(skip_serializing_if = "Option::is_none")]
        transition_key: Option<String>,
        /// Edge shape in the `before` graph.
        before: EdgeShape,
        /// Edge shape in the `after` graph.
        after: EdgeShape,
    },
}

/// Comparable summary of an edge — the parts of `GraphEdge` we can equate
/// across builds. The router closure of a conditional edge is opaque, so two
/// conditional edges with identical `(from, transition_key)` compare equal at
/// this layer; surfacing routing-logic deltas needs the same trait-extension
/// path that prompt diffs do (see module docs).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeShape {
    /// Source node id.
    pub from: String,
    /// Destination node id (empty for conditional edges resolved at runtime).
    pub to: String,
    /// Transition key (`None` is the default-CONTINUE path).
    pub transition_key: Option<String>,
    /// Edge kind — `Direct` or `Conditional`.
    pub edge_type: EdgeTypeTag,
}

/// Serializable tag for [`EdgeType`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EdgeTypeTag {
    /// Maps to [`EdgeType::Direct`].
    Direct,
    /// Maps to [`EdgeType::Conditional`].
    Conditional,
}

impl From<&EdgeType> for EdgeTypeTag {
    fn from(e: &EdgeType) -> Self {
        match e {
            EdgeType::Direct => Self::Direct,
            EdgeType::Conditional(_) => Self::Conditional,
        }
    }
}

impl From<&GraphEdge> for EdgeShape {
    fn from(e: &GraphEdge) -> Self {
        Self {
            from: e.from.clone(),
            to: e.to.clone(),
            transition_key: e.transition_key.clone(),
            edge_type: EdgeTypeTag::from(&e.edge_type),
        }
    }
}

/// The entry-point node id changed between graphs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntryPointChange {
    /// Entry point in the `before` graph.
    pub before: String,
    /// Entry point in the `after` graph.
    pub after: String,
}

/// Graph-level metadata change (name and/or description).
///
/// Each `_before` / `_after` pair is `None` when that specific field did not
/// change between graphs — so a description-only edit serializes without the
/// `name_*` fields and vice-versa.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetadataChange {
    /// Graph name in the `before` graph (present only when name changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_before: Option<String>,
    /// Graph name in the `after` graph (present only when name changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_after: Option<String>,
    /// Graph description in the `before` graph (present only when changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_before: Option<String>,
    /// Graph description in the `after` graph (present only when changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_after: Option<String>,
}

/// Coarse counts produced by [`GraphDiff::summary`] for one-line operator
/// status output ("3 nodes added, 1 edge removed, entry changed").
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ChangeSummary {
    /// Number of [`NodeChange::Added`] entries.
    pub nodes_added: usize,
    /// Number of [`NodeChange::Removed`] entries.
    pub nodes_removed: usize,
    /// Number of [`NodeChange::DescriptionChanged`] entries.
    pub nodes_changed: usize,
    /// Number of [`EdgeChange::Added`] entries.
    pub edges_added: usize,
    /// Number of [`EdgeChange::Removed`] entries.
    pub edges_removed: usize,
    /// Number of [`EdgeChange::Replaced`] entries.
    pub edges_replaced: usize,
    /// `true` when the entry point changed.
    pub entry_changed: bool,
    /// `true` when graph name or description changed.
    pub metadata_changed: bool,
}

impl CompiledGraph {
    /// Compute the structural diff against another graph, treating `self` as
    /// `before` and `other` as `after`. See the [`diff`](crate::diff) module
    /// docs for the limitations of "structural" at this trait surface.
    pub fn diff(&self, other: &CompiledGraph) -> GraphDiff {
        let node_changes = diff_nodes(self, other);
        let edge_changes = diff_edges(&self.edges, &other.edges);
        let entry_point = (self.entry_point != other.entry_point).then(|| EntryPointChange {
            before: self.entry_point.clone(),
            after: other.entry_point.clone(),
        });
        let metadata = diff_metadata(
            self.name.as_ref(),
            other.name.as_ref(),
            self.description.as_ref(),
            other.description.as_ref(),
        );
        GraphDiff {
            node_changes,
            edge_changes,
            entry_point,
            metadata,
        }
    }
}

fn node_description(g: &CompiledGraph, id: &str) -> Option<String> {
    g.get_node(id)
        .and_then(|n| n.executor.description())
        .map(str::to_string)
}

fn diff_nodes(before: &CompiledGraph, after: &CompiledGraph) -> Vec<NodeChange> {
    let before_ids = user_node_ids(before);
    let after_ids = user_node_ids(after);
    let mut changes = Vec::new();

    for id in before_ids.difference(&after_ids) {
        let description = node_description(before, id);
        changes.push(NodeChange::Removed {
            id: id.clone(),
            description,
        });
    }

    for id in after_ids.difference(&before_ids) {
        let description = node_description(after, id);
        changes.push(NodeChange::Added {
            id: id.clone(),
            description,
        });
    }

    for id in before_ids.intersection(&after_ids) {
        let b = node_description(before, id);
        let a = node_description(after, id);
        if a != b {
            changes.push(NodeChange::DescriptionChanged {
                id: id.clone(),
                before: b,
                after: a,
            });
        }
    }

    changes.sort_by(|a, b| a.id().cmp(b.id()));
    changes
}

/// All node ids present in `graph`, minus the synthetic `END` node that
/// `GraphBuilder::compile` injects — that node is identical in every graph
/// and showing it in every diff is noise.
fn user_node_ids(graph: &CompiledGraph) -> BTreeSet<String> {
    graph
        .node_indices
        .keys()
        .filter(|id| id.as_str() != transitions::END)
        .cloned()
        .collect()
}

type Key = (String, Option<String>);

fn group_by_key<'a>(
    edges: impl IntoIterator<Item = &'a GraphEdge>,
) -> BTreeMap<Key, Vec<&'a GraphEdge>> {
    let mut map: BTreeMap<Key, Vec<&'a GraphEdge>> = BTreeMap::new();
    for e in edges {
        let key = (e.from.clone(), e.transition_key.clone());
        map.entry(key).or_default().push(e);
    }
    map
}

fn diff_edges(before: &[GraphEdge], after: &[GraphEdge]) -> Vec<EdgeChange> {
    let before_filtered = before
        .iter()
        .filter(|e| e.from != transitions::END && e.to != transitions::END);
    let after_filtered = after
        .iter()
        .filter(|e| e.from != transitions::END && e.to != transitions::END);

    let before_map = group_by_key(before_filtered);
    let after_map = group_by_key(after_filtered);

    let mut changes = Vec::new();
    let keys: BTreeSet<Key> = before_map.keys().chain(after_map.keys()).cloned().collect();

    for k in keys {
        let bs = before_map.get(&k).map(Vec::as_slice).unwrap_or(&[]);
        let as_ = after_map.get(&k).map(Vec::as_slice).unwrap_or(&[]);

        match (bs.len(), as_.len()) {
            (0, _) => {
                for e in as_ {
                    changes.push(EdgeChange::Added(EdgeShape::from(*e)));
                }
            }
            (_, 0) => {
                for e in bs {
                    changes.push(EdgeChange::Removed(EdgeShape::from(*e)));
                }
            }
            (1, 1) => {
                let before_shape = EdgeShape::from(bs[0]);
                let after_shape = EdgeShape::from(as_[0]);
                if before_shape != after_shape {
                    changes.push(EdgeChange::Replaced {
                        from: k.0.clone(),
                        transition_key: k.1.clone(),
                        before: before_shape,
                        after: after_shape,
                    });
                }
            }
            _ => {
                // Multiple edges sharing the same (from, transition_key) is
                // unusual but legal in the builder API. Fall back to a raw
                // set diff so we don't silently coerce one into a `Replaced`
                // pairing with an arbitrary partner.
                let before_set: BTreeSet<EdgeShape> =
                    bs.iter().map(|e| EdgeShape::from(*e)).collect();
                let after_set: BTreeSet<EdgeShape> =
                    as_.iter().map(|e| EdgeShape::from(*e)).collect();
                for e in before_set.difference(&after_set) {
                    changes.push(EdgeChange::Removed(e.clone()));
                }
                for e in after_set.difference(&before_set) {
                    changes.push(EdgeChange::Added(e.clone()));
                }
            }
        }
    }

    changes.sort_by_key(edge_change_sort_key);
    changes
}

fn edge_change_sort_key(e: &EdgeChange) -> (String, Option<String>, String) {
    match e {
        EdgeChange::Added(s) | EdgeChange::Removed(s) => {
            (s.from.clone(), s.transition_key.clone(), s.to.clone())
        }
        EdgeChange::Replaced {
            from,
            transition_key,
            ..
        } => (from.clone(), transition_key.clone(), String::new()),
    }
}

fn diff_metadata(
    before_name: Option<&String>,
    after_name: Option<&String>,
    before_desc: Option<&String>,
    after_desc: Option<&String>,
) -> Option<MetadataChange> {
    let name_changed = before_name != after_name;
    let desc_changed = before_desc != after_desc;
    if !name_changed && !desc_changed {
        return None;
    }
    let field = |changed: bool, v: Option<&String>| changed.then(|| v.cloned()).flatten();
    Some(MetadataChange {
        name_before: field(name_changed, before_name),
        name_after: field(name_changed, after_name),
        description_before: field(desc_changed, before_desc),
        description_after: field(desc_changed, after_desc),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NodeError;
    use crate::graph::{GraphBuilder, NodeExecutor, NodeOutput};
    use crate::state::SharedState;
    use async_trait::async_trait;

    /// Minimal NodeExecutor that just returns its id (and optional description).
    /// Two harness flavors so we can drive description-change tests.
    struct StubNode {
        id: &'static str,
        description: Option<&'static str>,
    }

    #[async_trait]
    impl NodeExecutor for StubNode {
        fn id(&self) -> &str {
            self.id
        }
        fn description(&self) -> Option<&str> {
            self.description
        }
        async fn execute(&self, _state: SharedState) -> Result<NodeOutput, NodeError> {
            Ok(NodeOutput::cont())
        }
    }

    fn build(
        name: &str,
        nodes: Vec<StubNode>,
        edges: Vec<(&str, &str)>,
        entry: &str,
    ) -> CompiledGraph {
        let mut b = GraphBuilder::new().name(name);
        for n in nodes {
            b = b.add_node(n);
        }
        for (from, to) in edges {
            b = b.add_edge(from, to);
        }
        b.set_entry_point(entry).compile().unwrap()
    }

    #[test]
    fn identical_graphs_yield_empty_diff() {
        let a = build(
            "g",
            vec![
                StubNode {
                    id: "x",
                    description: Some("does x"),
                },
                StubNode {
                    id: "y",
                    description: None,
                },
            ],
            vec![("x", "y")],
            "x",
        );
        let b = build(
            "g",
            vec![
                StubNode {
                    id: "x",
                    description: Some("does x"),
                },
                StubNode {
                    id: "y",
                    description: None,
                },
            ],
            vec![("x", "y")],
            "x",
        );
        let d = a.diff(&b);
        assert!(d.is_empty(), "expected empty diff, got {d:?}");
        assert!(d.node_changes.is_empty());
        assert!(d.edge_changes.is_empty());
        assert!(d.entry_point.is_none());
        assert!(d.metadata.is_none());
    }

    #[test]
    fn added_and_removed_nodes_appear_sorted_by_id() {
        let before = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: None,
                },
                StubNode {
                    id: "b",
                    description: None,
                },
            ],
            vec![("a", "b")],
            "a",
        );
        let after = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: None,
                },
                StubNode {
                    id: "c",
                    description: Some("the new node"),
                },
            ],
            vec![("a", "c")],
            "a",
        );
        let d = before.diff(&after);

        // `b` was removed, `c` was added; sorted by id: b then c.
        assert_eq!(d.node_changes.len(), 2);
        match &d.node_changes[0] {
            NodeChange::Removed { id, .. } => assert_eq!(id, "b"),
            other => panic!("expected first change to be Removed(b), got {other:?}"),
        }
        match &d.node_changes[1] {
            NodeChange::Added { id, description } => {
                assert_eq!(id, "c");
                assert_eq!(description.as_deref(), Some("the new node"));
            }
            other => panic!("expected second change to be Added(c), got {other:?}"),
        }
    }

    #[test]
    fn description_only_change_uses_description_changed_variant() {
        let before = build(
            "g",
            vec![StubNode {
                id: "x",
                description: Some("old"),
            }],
            vec![],
            "x",
        );
        let after = build(
            "g",
            vec![StubNode {
                id: "x",
                description: Some("new"),
            }],
            vec![],
            "x",
        );
        let d = before.diff(&after);
        assert_eq!(d.node_changes.len(), 1);
        match &d.node_changes[0] {
            NodeChange::DescriptionChanged { id, before, after } => {
                assert_eq!(id, "x");
                assert_eq!(before.as_deref(), Some("old"));
                assert_eq!(after.as_deref(), Some("new"));
            }
            other => panic!("expected DescriptionChanged, got {other:?}"),
        }
    }

    #[test]
    fn edge_added_and_removed_surface_separately() {
        // Pick edges that do NOT share a (from, transition_key) — otherwise
        // a single destination change would surface as `Replaced`, not as
        // separate Added + Removed entries.
        let before = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: None,
                },
                StubNode {
                    id: "b",
                    description: None,
                },
                StubNode {
                    id: "c",
                    description: None,
                },
            ],
            vec![("a", "b"), ("b", "c")],
            "a",
        );
        let after = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: None,
                },
                StubNode {
                    id: "b",
                    description: None,
                },
                StubNode {
                    id: "c",
                    description: None,
                },
            ],
            // (a,None)→b kept; (b,None)→c removed; new (c,None)→b added.
            vec![("a", "b"), ("c", "b")],
            "a",
        );
        let d = before.diff(&after);
        assert!(d.node_changes.is_empty(), "{:?}", d.node_changes);
        assert_eq!(d.edge_changes.len(), 2, "got: {:?}", d.edge_changes);
        let kinds: Vec<&'static str> = d
            .edge_changes
            .iter()
            .map(|c| match c {
                EdgeChange::Added(_) => "added",
                EdgeChange::Removed(_) => "removed",
                EdgeChange::Replaced { .. } => "replaced",
            })
            .collect();
        assert!(kinds.contains(&"added"));
        assert!(kinds.contains(&"removed"));
    }

    #[test]
    fn edge_destination_change_surfaces_as_replaced() {
        let before = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: None,
                },
                StubNode {
                    id: "b",
                    description: None,
                },
                StubNode {
                    id: "c",
                    description: None,
                },
            ],
            vec![("a", "b")],
            "a",
        );
        let after = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: None,
                },
                StubNode {
                    id: "b",
                    description: None,
                },
                StubNode {
                    id: "c",
                    description: None,
                },
            ],
            // same (from=a, transition_key=None), new destination
            vec![("a", "c")],
            "a",
        );
        let d = before.diff(&after);
        assert_eq!(d.edge_changes.len(), 1);
        match &d.edge_changes[0] {
            EdgeChange::Replaced {
                from,
                before,
                after,
                ..
            } => {
                assert_eq!(from, "a");
                assert_eq!(before.to, "b");
                assert_eq!(after.to, "c");
            }
            other => panic!("expected Replaced, got {other:?}"),
        }
    }

    #[test]
    fn entry_point_change_is_surfaced() {
        let before = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: None,
                },
                StubNode {
                    id: "b",
                    description: None,
                },
            ],
            vec![("a", "b"), ("b", "a")],
            "a",
        );
        let after = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: None,
                },
                StubNode {
                    id: "b",
                    description: None,
                },
            ],
            vec![("a", "b"), ("b", "a")],
            "b",
        );
        let d = before.diff(&after);
        let ep = d.entry_point.as_ref().expect("entry point change expected");
        assert_eq!(ep.before, "a");
        assert_eq!(ep.after, "b");
    }

    #[test]
    fn metadata_change_only_serializes_changed_fields() {
        let before = build(
            "g-old",
            vec![StubNode {
                id: "x",
                description: None,
            }],
            vec![],
            "x",
        );
        // Same name+description change in description should only carry name fields.
        let after = build(
            "g-new",
            vec![StubNode {
                id: "x",
                description: None,
            }],
            vec![],
            "x",
        );
        let d = before.diff(&after);
        let m = d.metadata.as_ref().expect("metadata change expected");
        assert_eq!(m.name_before.as_deref(), Some("g-old"));
        assert_eq!(m.name_after.as_deref(), Some("g-new"));
        assert!(m.description_before.is_none());
        assert!(m.description_after.is_none());

        let json = serde_json::to_value(&d).unwrap();
        // No description_* fields in serialized form when only name changed.
        let meta = json.get("metadata").unwrap();
        assert!(meta.get("description_before").is_none());
        assert!(meta.get("description_after").is_none());
    }

    #[test]
    fn summary_counts_match_diff_contents() {
        // Build a diff that exercises every counter in ChangeSummary:
        //   - node added (`added`), removed (`removed`), description-changed (`a`)
        //   - edge replaced (b→removed → b→added; same `(b, None)` key)
        //   - edge added (added→removed-source new path)
        //   - edge removed (removed→a gone)
        let before = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: Some("old"),
                },
                StubNode {
                    id: "b",
                    description: None,
                },
                StubNode {
                    id: "removed",
                    description: None,
                },
            ],
            vec![("a", "b"), ("b", "removed"), ("removed", "a")],
            "a",
        );
        let after = build(
            "g",
            vec![
                StubNode {
                    id: "a",
                    description: Some("new"),
                },
                StubNode {
                    id: "b",
                    description: None,
                },
                StubNode {
                    id: "added",
                    description: None,
                },
            ],
            // a→b unchanged; (b,None) destination flips: b→removed → b→added
            // (Replaced); removed→a gone (Removed because `removed` node no
            // longer exists); new added→a edge (Added).
            vec![("a", "b"), ("b", "added"), ("added", "a")],
            "a",
        );
        let d = before.diff(&after);
        let s = d.summary();
        assert_eq!(s.nodes_added, 1, "{s:?}");
        assert_eq!(s.nodes_removed, 1, "{s:?}");
        assert_eq!(s.nodes_changed, 1, "description on `a` changed");
        assert_eq!(s.edges_added, 1, "added→a is new ({s:?})");
        assert_eq!(s.edges_removed, 1, "removed→a is gone ({s:?})");
        assert_eq!(s.edges_replaced, 1, "(b,None) target flipped ({s:?})");
        assert!(!s.entry_changed);
        assert!(!s.metadata_changed);
    }

    #[test]
    fn diff_roundtrips_through_serde_json() {
        let before = build(
            "g",
            vec![StubNode {
                id: "x",
                description: None,
            }],
            vec![],
            "x",
        );
        let after = build(
            "g",
            vec![
                StubNode {
                    id: "x",
                    description: None,
                },
                StubNode {
                    id: "y",
                    description: Some("hi"),
                },
            ],
            vec![("x", "y")],
            "x",
        );
        let d = before.diff(&after);
        let s = serde_json::to_string(&d).unwrap();
        let back: GraphDiff = serde_json::from_str(&s).unwrap();
        // Re-serialize and compare strings; the type isn't PartialEq so this is
        // the simplest way to check round-trip determinism.
        assert_eq!(s, serde_json::to_string(&back).unwrap());
    }

    #[test]
    fn edges_incident_to_end_are_filtered_out() {
        let before = build(
            "g",
            vec![StubNode {
                id: "x",
                description: None,
            }],
            vec![],
            "x",
        );
        let after = GraphBuilder::new()
            .name("g")
            .add_node(StubNode {
                id: "x",
                description: None,
            })
            .add_edge_to_end("x")
            .set_entry_point("x")
            .compile()
            .unwrap();

        let d = before.diff(&after);
        assert!(
            d.edge_changes.is_empty(),
            "expected no edge changes, got {:?}",
            d.edge_changes
        );
    }
}
