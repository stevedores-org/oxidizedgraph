use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_file;
#[cfg(windows)]
use std::os::windows::fs::symlink_file;

/// Represents the status of a symlink
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymlinkStatus {
    /// Symlink exists and correctly points to the master
    Valid,
    /// Symlink exists but points to the wrong target
    IncorrectTarget(PathBuf),
    /// File exists but is not a symlink (might be a real file)
    NotASymlink,
    /// Symlink is broken (target doesn't exist)
    Broken,
    /// Symlink or file does not exist
    Missing,
}

/// Known symlink targets for the governance framework
pub const KNOWN_TARGETS: &[&str] = &[
    "CLAUDE.md",
    ".cursor/rules/swarm.mdc",
    "AGENTS.md",
    ".github/copilot-instructions.md",
    "GEMINI.md",
    ".agent/rules/orchestration.md",
];

/// Manages symlinks for agent guidance
pub struct SymlinkManager {
    /// The master file that all symlinks should point to
    master_file: PathBuf,
    /// Base directory where paths are resolved
    base_dir: PathBuf,
}

impl SymlinkManager {
    /// Create a new symlink manager
    pub fn new(base_dir: impl Into<PathBuf>, master_file: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            master_file: master_file.into(),
        }
    }

    /// Default setup assuming AGENTS.md in the root as master
    pub fn default(base_dir: impl Into<PathBuf>) -> Self {
        Self::new(base_dir, "AGENTS.md")
    }

    /// Check the status of a specific target
    pub fn check_status(&self, target_rel_path: &Path) -> io::Result<SymlinkStatus> {
        let full_path = self.base_dir.join(target_rel_path);
        let master_full_path = self.base_dir.join(&self.master_file);

        if !full_path.exists() && !full_path.is_symlink() {
            return Ok(SymlinkStatus::Missing);
        }

        let metadata = fs::symlink_metadata(&full_path)?;
        if !metadata.is_symlink() {
            return Ok(SymlinkStatus::NotASymlink);
        }

        let target = fs::read_link(&full_path)?;

        // Target may be relative to the symlink's directory, so we canonicalize both
        let canonical_master = master_full_path
            .canonicalize()
            .unwrap_or_else(|_| master_full_path.clone());
        let canonical_target = if target.is_absolute() {
            target.clone()
        } else {
            full_path
                .parent()
                .unwrap_or(&self.base_dir)
                .join(&target)
                .canonicalize()
                .unwrap_or(target.clone())
        };

        if !canonical_master.exists() {
            return Ok(SymlinkStatus::Broken);
        }

        if canonical_target == canonical_master {
            Ok(SymlinkStatus::Valid)
        } else {
            Ok(SymlinkStatus::IncorrectTarget(target))
        }
    }

    /// Ensure a symlink points to the master file
    pub fn sync_symlink(&self, target_rel_path: &Path) -> io::Result<()> {
        let full_path = self.base_dir.join(target_rel_path);
        let master_full_path = self.base_dir.join(&self.master_file);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let status = self.check_status(target_rel_path)?;
        match status {
            SymlinkStatus::Valid => return Ok(()),
            SymlinkStatus::NotASymlink
            | SymlinkStatus::IncorrectTarget(_)
            | SymlinkStatus::Broken => {
                fs::remove_file(&full_path)?;
            }
            SymlinkStatus::Missing => {}
        }

        // Compute relative path from the symlink directory to the master file
        let src_dir = full_path.parent().unwrap_or(&self.base_dir);
        let relative_target = pathdiff::diff_paths(&master_full_path, src_dir)
            .unwrap_or_else(|| master_full_path.clone());

        symlink_file(&relative_target, &full_path)
    }

    /// Synchronize all known targets
    pub fn sync_all(&self) -> io::Result<Vec<(PathBuf, Result<(), io::Error>)>> {
        let mut results = Vec::new();
        for target in KNOWN_TARGETS {
            if target == &self.master_file.to_string_lossy() {
                continue;
            }
            let path = PathBuf::from(target);
            let res = self.sync_symlink(&path);
            results.push((path, res));
        }
        Ok(results)
    }
}
