//! Governance node: applies role-scoped manifest guidance to agent state.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::SharedState;

use super::config::GovernanceConfig;
use super::guidance::{apply_role_guidance, load_manifest};
use super::roles::AgentRole;

/// Error loading or applying governance configuration.
#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    /// Failed to read the master manifest from disk.
    #[error("failed to read governance manifest at {path}: {source}")]
    Io {
        /// Manifest path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
}

/// Node that loads an `AGENTS.md`-style manifest and applies role-scoped guidance.
#[derive(Clone, Debug)]
pub struct GovernanceNode {
    id: String,
    manifest: String,
    role: AgentRole,
    config: GovernanceConfig,
}

impl GovernanceNode {
    /// Create a node with an inline manifest string.
    pub fn new(
        id: impl Into<String>,
        manifest: impl Into<String>,
        role: AgentRole,
    ) -> Self {
        Self {
            id: id.into(),
            manifest: manifest.into(),
            role,
            config: GovernanceConfig::default(),
        }
    }

    /// Override governance application behavior.
    pub fn with_config(mut self, config: GovernanceConfig) -> Self {
        self.config = config;
        self
    }

    /// Load the manifest from a file path (typically `AGENTS.md`).
    pub fn from_file(
        id: impl Into<String>,
        path: impl Into<PathBuf>,
        role: AgentRole,
    ) -> Result<Self, GovernanceError> {
        let path = path.into();
        let manifest = load_manifest(&path).map_err(|source| GovernanceError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(Self::new(id, manifest, role))
    }

    /// Active role for this node.
    pub fn role(&self) -> &AgentRole {
        &self.role
    }
}

#[async_trait]
impl NodeExecutor for GovernanceNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;
        apply_role_guidance(&mut guard, &self.manifest, &self.role, &self.config);
        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Applies role-scoped governance guidance from a manifest")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::guidance::{agent_role_from_state, CTX_GOVERNANCE_GUIDANCE};
    use crate::state::AgentState;
    use std::sync::{Arc, RwLock};

    const MANIFEST: &str = "<@builder>\nRun cargo test.\n";

    #[tokio::test]
    async fn governance_node_applies_builder_guidance() {
        let node = GovernanceNode::new("gov", MANIFEST, AgentRole::Builder);
        let state = Arc::new(RwLock::new(AgentState::new()));
        node.execute(state.clone()).await.unwrap();

        let guard = state.read().unwrap();
        assert_eq!(agent_role_from_state(&guard), Some(AgentRole::Builder));
        assert!(guard
            .get_context::<String>(CTX_GOVERNANCE_GUIDANCE)
            .unwrap()
            .contains("cargo test"));
    }
}
