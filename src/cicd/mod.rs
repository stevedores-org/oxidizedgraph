//! Multi-repo and CI/CD orchestration (EPIC9).
//!
//! Coordinates autonomous changes across repositories with dependency-aware
//! scheduling, CI signal aggregation, and release gating.

mod change_graph;
mod ci_aggregate;
mod coordinator;
mod node;
mod release;

pub use change_graph::{CrossRepoChangeGraph, RepoChange, RepoChangeStatus};
pub use ci_aggregate::{CiAggregateReport, CiAggregator, CiCheckSignal, CiConclusion};
pub use coordinator::MultiRepoCoordinator;
pub use node::{
    CiAggregateNode, CompleteRepoChangeNode, MultiRepoCoordinatorNode, ReleaseGateNode,
    CTX_CHANGE_GRAPH, CTX_CI_AGGREGATE, CTX_CURRENT_REPO_CHANGE, CTX_RELEASE_GATE,
};
pub use release::{ReleaseBatch, ReleaseGateResult, ReleaseOrchestrator};
