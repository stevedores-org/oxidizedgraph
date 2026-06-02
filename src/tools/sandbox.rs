//! Sandboxed subprocess execution with timeouts.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

/// Sandbox execution error.
#[derive(Error, Debug, PartialEq)]
pub enum SandboxError {
    /// Command exceeded the configured timeout
    #[error("Command timed out after {0:?}")]
    Timeout(Duration),

    /// Process failed to start or returned non-zero exit
    #[error("Command failed: {0}")]
    CommandFailed(String),

    /// Working directory is outside allowed roots
    #[error("Working directory not allowed: {0}")]
    InvalidWorkingDir(String),
}

/// Configuration for subprocess sandbox runs.
#[derive(Clone, Debug)]
pub struct SandboxConfig {
    /// Maximum execution time
    pub timeout: Duration,
    /// Allowed working directory roots (empty = any)
    pub allowed_roots: Vec<PathBuf>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            allowed_roots: Vec::new(),
        }
    }
}

impl SandboxConfig {
    /// Create config with a timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            ..Self::default()
        }
    }

    /// Restrict execution to paths under the given root.
    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.allowed_roots.push(root);
        self
    }
}

/// Trait for sandboxed command execution.
#[async_trait::async_trait]
pub trait SandboxExecutor: Send + Sync {
    /// Run a shell command and return combined stdout+stderr on success.
    async fn run(&self, command: &str, working_dir: Option<&Path>) -> Result<String, SandboxError>;
}

/// Subprocess-based sandbox with timeout and optional cwd restriction.
#[derive(Clone, Debug, Default)]
pub struct SubprocessSandbox {
    config: SandboxConfig,
}

impl SubprocessSandbox {
    /// Create a subprocess sandbox.
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    fn validate_working_dir(&self, dir: &Path) -> Result<(), SandboxError> {
        if self.config.allowed_roots.is_empty() {
            return Ok(());
        }
        let canonical = dir
            .canonicalize()
            .map_err(|e| SandboxError::InvalidWorkingDir(e.to_string()))?;
        for root in &self.config.allowed_roots {
            let root_canon = root
                .canonicalize()
                .map_err(|e| SandboxError::InvalidWorkingDir(e.to_string()))?;
            if canonical.starts_with(&root_canon) {
                return Ok(());
            }
        }
        Err(SandboxError::InvalidWorkingDir(format!(
            "{canonical:?} not under allowed roots"
        )))
    }
}

#[async_trait::async_trait]
impl SandboxExecutor for SubprocessSandbox {
    async fn run(&self, command: &str, working_dir: Option<&Path>) -> Result<String, SandboxError> {
        if let Some(dir) = working_dir {
            self.validate_working_dir(dir)?;
        }

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let future = async {
            let output = cmd
                .output()
                .await
                .map_err(|e| SandboxError::CommandFailed(e.to_string()))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(SandboxError::CommandFailed(stderr.to_string()));
            }
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        };

        timeout(self.config.timeout, future)
            .await
            .map_err(|_| SandboxError::Timeout(self.config.timeout))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subprocess_sandbox_echo() {
        let sandbox = SubprocessSandbox::new(SandboxConfig::with_timeout(Duration::from_secs(5)));
        let out = sandbox.run("echo hello", None).await.unwrap();
        assert_eq!(out, "hello");
    }

    #[tokio::test]
    async fn test_subprocess_sandbox_timeout() {
        let sandbox = SubprocessSandbox::new(SandboxConfig::with_timeout(Duration::from_millis(50)));
        let result = sandbox.run("sleep 2", None).await;
        assert!(matches!(result, Err(SandboxError::Timeout(_))));
    }
}
