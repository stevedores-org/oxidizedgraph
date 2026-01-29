//! Edge definitions for oxidizedgraph
//!
//! Edges define the transitions between nodes in a graph.
//! They can be direct (always go to a specific node) or
//! conditional (choose based on state).

use crate::state::State;
use std::sync::Arc;

/// Target specification for an edge
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EdgeTarget {
    /// Go to a specific node by ID
    Node(String),
    /// End the graph execution
    End,
}

impl EdgeTarget {
    /// Create a node target
    pub fn node(id: impl Into<String>) -> Self {
        Self::Node(id.into())
    }

    /// Check if this is an end target
    pub fn is_end(&self) -> bool {
        matches!(self, Self::End)
    }

    /// Get the node ID if this is a node target
    pub fn node_id(&self) -> Option<&str> {
        match self {
            Self::Node(id) => Some(id),
            Self::End => None,
        }
    }
}

/// Conditional routing trait
///
/// Implement this trait to create custom routing logic based on state.
///
/// # Example
///
/// ```rust,ignore
/// struct ShouldContinue;
///
/// impl Router<MyState> for ShouldContinue {
///     fn route(&self, state: &MyState) -> EdgeTarget {
///         if state.is_complete {
///             EdgeTarget::End
///         } else {
///             EdgeTarget::node("process")
///         }
///     }
///
///     fn possible_targets(&self) -> Vec<EdgeTarget> {
///         vec![EdgeTarget::node("process"), EdgeTarget::End]
///     }
/// }
/// ```
pub trait Router<S: State>: Send + Sync {
    /// Determine the next edge target based on current state
    fn route(&self, state: &S) -> EdgeTarget;

    /// Return all possible targets this router might return.
    /// Used for graph validation and visualization.
    fn possible_targets(&self) -> Vec<EdgeTarget> {
        vec![]
    }
}

/// A router implemented as a closure
pub struct FnRouter<S, F>
where
    S: State,
    F: Fn(&S) -> EdgeTarget + Send + Sync,
{
    func: F,
    possible_targets: Vec<EdgeTarget>,
    _phantom: std::marker::PhantomData<S>,
}

impl<S, F> FnRouter<S, F>
where
    S: State,
    F: Fn(&S) -> EdgeTarget + Send + Sync,
{
    /// Create a new function-based router
    pub fn new(func: F) -> Self {
        Self {
            func,
            possible_targets: vec![],
            _phantom: std::marker::PhantomData,
        }
    }

    /// Specify possible targets for validation
    pub fn with_targets(mut self, targets: Vec<EdgeTarget>) -> Self {
        self.possible_targets = targets;
        self
    }
}

impl<S, F> Router<S> for FnRouter<S, F>
where
    S: State,
    F: Fn(&S) -> EdgeTarget + Send + Sync,
{
    fn route(&self, state: &S) -> EdgeTarget {
        (self.func)(state)
    }

    fn possible_targets(&self) -> Vec<EdgeTarget> {
        self.possible_targets.clone()
    }
}

/// Edge definition in the graph
pub enum Edge<S: State> {
    /// Always go to target
    Direct(EdgeTarget),
    /// Conditional routing based on state
    Conditional(Box<dyn Router<S>>),
}

impl<S: State> Edge<S> {
    /// Create a direct edge to a node
    pub fn to_node(id: impl Into<String>) -> Self {
        Self::Direct(EdgeTarget::Node(id.into()))
    }

    /// Create a direct edge to end
    pub fn to_end() -> Self {
        Self::Direct(EdgeTarget::End)
    }

    /// Create a conditional edge with a closure
    pub fn conditional_fn<F>(f: F) -> Self
    where
        F: Fn(&S) -> EdgeTarget + Send + Sync + 'static,
    {
        Self::Conditional(Box::new(FnRouter::new(f)))
    }

    /// Create a conditional edge with a router
    pub fn conditional<R: Router<S> + 'static>(router: R) -> Self {
        Self::Conditional(Box::new(router))
    }

    /// Resolve this edge to a target given the current state
    pub fn resolve(&self, state: &S) -> EdgeTarget {
        match self {
            Self::Direct(target) => target.clone(),
            Self::Conditional(router) => router.route(state),
        }
    }

    /// Get all possible targets for this edge
    pub fn possible_targets(&self) -> Vec<EdgeTarget> {
        match self {
            Self::Direct(target) => vec![target.clone()],
            Self::Conditional(router) => router.possible_targets(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    struct TestState {
        go_to_end: bool,
    }

    impl State for TestState {
        fn schema() -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
    }

    #[test]
    fn test_edge_target() {
        let target = EdgeTarget::node("my_node");
        assert_eq!(target.node_id(), Some("my_node"));
        assert!(!target.is_end());

        let target = EdgeTarget::End;
        assert!(target.is_end());
        assert_eq!(target.node_id(), None);
    }

    #[test]
    fn test_direct_edge() {
        let edge: Edge<TestState> = Edge::to_node("next");
        let state = TestState::default();

        assert_eq!(edge.resolve(&state), EdgeTarget::node("next"));
    }

    #[test]
    fn test_conditional_edge() {
        let edge: Edge<TestState> = Edge::conditional_fn(|s: &TestState| {
            if s.go_to_end {
                EdgeTarget::End
            } else {
                EdgeTarget::node("continue")
            }
        });

        let state = TestState { go_to_end: false };
        assert_eq!(edge.resolve(&state), EdgeTarget::node("continue"));

        let state = TestState { go_to_end: true };
        assert_eq!(edge.resolve(&state), EdgeTarget::End);
    }

    struct TestRouter;

    impl Router<TestState> for TestRouter {
        fn route(&self, state: &TestState) -> EdgeTarget {
            if state.go_to_end {
                EdgeTarget::End
            } else {
                EdgeTarget::node("process")
            }
        }

        fn possible_targets(&self) -> Vec<EdgeTarget> {
            vec![EdgeTarget::node("process"), EdgeTarget::End]
        }
    }

    #[test]
    fn test_router_trait() {
        let edge: Edge<TestState> = Edge::conditional(TestRouter);

        let targets = edge.possible_targets();
        assert_eq!(targets.len(), 2);
    }
}
