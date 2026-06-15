//! Compliance validation against governance symlink and manifest rules.

use crate::governance::roles::{AgentRole, RoleParseError};
use crate::governance::symlinks::{SymlinkManager, SymlinkStatus, KNOWN_TARGETS};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Error during manifest validation.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ManifestError {
    /// The tag format itself is invalid.
    #[error("Line {line_number}: Tag '{tag}' is invalid: {error}")]
    InvalidTag {
        /// Line number (1-indexed).
        line_number: usize,
        /// Raw tag string.
        tag: String,
        /// Detail of the parse failure.
        error: String,
    },
    /// The boundary looks like a tag but has syntax errors (e.g. starts with `<@` but lacks `>`).
    #[error("Line {line_number}: Line starts with '<@' but does not end with '>'")]
    MalformedTagBoundary {
        /// Line number (1-indexed).
        line_number: usize,
        /// Raw line string.
        line: String,
        /// Detail of the syntax error.
        reason: String,
    },
}

/// Consolidated validation error containing all violations.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// One or more errors found in the manifest file.
    #[error("Manifest validation failed:\n{}", .0.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"))]
    Manifest(Vec<ManifestError>),

    /// One or more symlink violations found.
    #[error("Symlink integrity validation failed for target {target}: {details}")]
    Symlink {
        /// Expected SSOT target file name.
        target: PathBuf,
        /// String containing descriptions of all violations.
        details: String,
    },

    /// I/O error reading the manifest.
    #[error("I/O error during validation: {0}")]
    Io(#[from] std::io::Error),
}

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

    /// Create a new validator with custom symlink manager/master file
    pub fn with_master(base_dir: impl Into<PathBuf>, master_file: impl Into<PathBuf>) -> Self {
        let base_dir = base_dir.into();
        Self {
            symlink_mgr: SymlinkManager::new(base_dir.clone(), master_file),
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

    /// Validate a manifest string for syntactically correct role tags.
    pub fn validate_manifest_string(&self, manifest: &str) -> Result<(), ValidationError> {
        let mut errors = Vec::new();
        for (idx, line) in manifest.lines().enumerate() {
            let line_no = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with("<@") {
                if trimmed.ends_with('>') {
                    let inner = &trimmed[2..trimmed.len() - 1];
                    if let Err(e) = AgentRole::from_inner(inner) {
                        errors.push(ManifestError::InvalidTag {
                            line_number: line_no,
                            tag: trimmed.to_string(),
                            error: match e {
                                RoleParseError::Empty => "tag name is empty".to_string(),
                                RoleParseError::InvalidCharacter(c) => {
                                    format!("invalid character in tag body: '{}'", c)
                                }
                            },
                        });
                    }
                } else {
                    errors.push(ManifestError::MalformedTagBoundary {
                        line_number: line_no,
                        line: trimmed.to_string(),
                        reason: "starts with '<@' but does not end with '>'".to_string(),
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::Manifest(errors))
        }
    }

    /// Validate the manifest file at the given path.
    pub fn validate_manifest_file(&self, path: impl AsRef<Path>) -> Result<(), ValidationError> {
        let content = std::fs::read_to_string(path)?;
        self.validate_manifest_string(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_manifest_string_valid() {
        let manifest = "\
<@all>
Shared preamble.

<@builder>
Builder guidance.
";
        let validator = GovernanceValidator::new(".");
        assert!(validator.validate_manifest_string(manifest).is_ok());
    }

    #[test]
    fn test_validate_manifest_string_invalid() {
        let manifest = "\
<@all>
Shared preamble.

<@builder!>
Builder guidance.

<@
Empty tag line.
";
        let validator = GovernanceValidator::new(".");
        let result = validator.validate_manifest_string(manifest);
        assert!(result.is_err());
        if let Err(ValidationError::Manifest(errors)) = result {
            assert_eq!(errors.len(), 2);
            assert!(matches!(errors[0], ManifestError::InvalidTag { line_number: 4, .. }));
            assert!(matches!(errors[1], ManifestError::MalformedTagBoundary { line_number: 7, .. }));
        } else {
            panic!("Expected ValidationError::Manifest");
        }
    }
}
