//! Discover agent manifests and role metadata in a repository tree.

use crate::governance::roles::AgentRole;
use std::fs;
use std::path::PathBuf;

/// Represents an agent discovered in the repository
#[derive(Debug, Clone)]
pub struct DiscoveredAgent {
    /// The name of the agent or its role
    pub name: String,
    /// Path to the agent's configuration or guidance file
    pub guidance_path: PathBuf,
    /// Inferred role for the agent
    pub inferred_role: AgentRole,
}

/// Discovers agent definitions in a repository
pub struct AgentDiscovery {
    base_dir: PathBuf,
}

impl AgentDiscovery {
    /// Create a new discovery instance
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Scan for agents in standard locations
    pub fn scan(&self) -> Vec<DiscoveredAgent> {
        let mut agents = Vec::new();

        // Standard locations to check
        let locations = [
            ("CLAUDE.md", AgentRole::Custom("claude".to_string())),
            ("GEMINI.md", AgentRole::Custom("gemini".to_string())),
            (
                ".cursor/rules/swarm.mdc",
                AgentRole::Custom("cursor".to_string()),
            ),
            (
                ".github/copilot-instructions.md",
                AgentRole::Custom("copilot".to_string()),
            ),
        ];

        for (loc, role) in locations {
            let path = self.base_dir.join(loc);
            if path.exists() {
                agents.push(DiscoveredAgent {
                    name: role.to_string(),
                    guidance_path: path,
                    inferred_role: role,
                });
            }
        }

        // Also check if AGENTS.md exists and parse roles from it
        let master_path = self.base_dir.join("AGENTS.md");
        if master_path.exists() {
            if let Ok(content) = fs::read_to_string(&master_path) {
                let blocks = crate::governance::parser::parse_blocks(&content);
                for block in blocks {
                    if block.role != AgentRole::All {
                        // Avoid duplicates if a role already has a specific file
                        if !agents.iter().any(|a| a.inferred_role == block.role) {
                            agents.push(DiscoveredAgent {
                                name: block.role.to_string(),
                                guidance_path: master_path.clone(),
                                inferred_role: block.role,
                            });
                        }
                    }
                }
            }
        }

        agents
    }
}
