//! Compliance validation against governance symlink and manifest rules.

use crate::governance::symlinks::{SymlinkManager, SymlinkStatus, KNOWN_TARGETS};
use std::path::PathBuf;

/// Validates compliance of an agent repository with the governance framework
pub struct GovernanceValidator {
    base_dir: PathBuf,
    symlink_mgr: SymlinkManager,
}

/// Results of a compliance check
#[derive(Debug, Clone)]
pub struct ComplianceReport {
    /// True if all checks pass
    pub is_compliant: bool,
    /// List of issues found
    pub issues: Vec<String>,
    /// Summary of symlink statuses
    pub symlinks: Vec<(PathBuf, SymlinkStatus)>,
}

impl GovernanceValidator {
    /// Create a new validator
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        let base_dir = base_dir.into();
        Self {
            symlink_mgr: SymlinkManager::default(base_dir.clone()),
            base_dir,
        }
    }

    /// Validate the compliance of the setup
    pub fn validate_compliance(&self) -> ComplianceReport {
        let mut is_compliant = true;
        let mut issues = Vec::new();
        let mut symlinks = Vec::new();

        // 1. Check if master AGENTS.md exists
        let master_path = self.base_dir.join("AGENTS.md");
        if !master_path.exists() {
            is_compliant = false;
            issues.push("Master AGENTS.md file is missing".to_string());
        }

        // 2. Check all known symlink targets
        for target in KNOWN_TARGETS {
            if target == &"AGENTS.md" {
                continue;
            }

            let path = PathBuf::from(target);
            match self.symlink_mgr.check_status(&path) {
                Ok(status) => {
                    if status != SymlinkStatus::Valid && status != SymlinkStatus::Missing {
                        is_compliant = false;
                        issues.push(format!("Symlink issue for {}: {:?}", target, status));
                    }
                    symlinks.push((path, status));
                }
                Err(e) => {
                    is_compliant = false;
                    issues.push(format!("Failed to check symlink {}: {}", target, e));
                }
            }
        }

        ComplianceReport {
            is_compliant,
            issues,
            symlinks,
        }
    }
}
