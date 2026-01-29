//! Git operations using libgit2
//!
//! Provides programmatic Git operations that work like the CLI.
//!
//! # Example
//!
//! ```rust,ignore
//! use oxidizedgraph::git;
//!
//! // Push current branch to origin
//! git::push(".", "origin", None)?;
//!
//! // Push specific branch
//! git::push(".", "origin", Some("feature-branch"))?;
//! ```

use git2::{Cred, PushOptions, RemoteCallbacks, Repository};
use std::env;
use std::path::Path;
use thiserror::Error;

/// Git operation errors
#[derive(Error, Debug)]
pub enum GitError {
    /// Underlying git2 error
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    /// Repository not found at path
    #[error("Repository not found at: {0}")]
    RepoNotFound(String),

    /// Remote not found
    #[error("Remote not found: {0}")]
    RemoteNotFound(String),

    /// No branch checked out (detached HEAD)
    #[error("No branch checked out")]
    NoBranch,

    /// Push operation failed
    #[error("Push failed: {0}")]
    PushFailed(String),
}

/// Result type for git operations
pub type Result<T> = std::result::Result<T, GitError>;

/// Push the current (or specified) branch to a remote.
///
/// Uses the same credential helpers as `git push`, so it works
/// seamlessly with existing Git configurations.
///
/// # Arguments
///
/// * `repo_path` - Path to the repository (use "." for current directory)
/// * `remote_name` - Name of the remote (usually "origin")
/// * `branch` - Optional branch name. If None, pushes current branch.
///
/// # Example
///
/// ```rust,ignore
/// use oxidizedgraph::git;
///
/// // Push current branch
/// git::push(".", "origin", None)?;
///
/// // Push specific branch
/// git::push("/path/to/repo", "origin", Some("main"))?;
/// ```
pub fn push(repo_path: impl AsRef<Path>, remote_name: &str, branch: Option<&str>) -> Result<()> {
    let repo = Repository::open(repo_path.as_ref())
        .map_err(|_| GitError::RepoNotFound(repo_path.as_ref().display().to_string()))?;

    let mut remote = repo
        .find_remote(remote_name)
        .map_err(|_| GitError::RemoteNotFound(remote_name.to_string()))?;

    // Set up authentication callbacks
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credential_callback);

    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    // Determine branch to push
    let branch_name = match branch {
        Some(b) => b.to_string(),
        None => {
            let head = repo.head().map_err(|_| GitError::NoBranch)?;
            head.shorthand()
                .ok_or(GitError::NoBranch)?
                .to_string()
        }
    };

    let refspec = format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name);
    remote.push(&[&refspec], Some(&mut push_opts))?;

    Ok(())
}

/// Push with progress callback
pub fn push_with_progress<F>(
    repo_path: impl AsRef<Path>,
    remote_name: &str,
    branch: Option<&str>,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(usize, usize, usize), // current, total, bytes
{
    let repo = Repository::open(repo_path.as_ref())
        .map_err(|_| GitError::RepoNotFound(repo_path.as_ref().display().to_string()))?;

    let mut remote = repo
        .find_remote(remote_name)
        .map_err(|_| GitError::RemoteNotFound(remote_name.to_string()))?;

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credential_callback);

    callbacks.push_transfer_progress(move |current, total, bytes| {
        on_progress(current, total, bytes);
    });

    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    let branch_name = match branch {
        Some(b) => b.to_string(),
        None => {
            let head = repo.head().map_err(|_| GitError::NoBranch)?;
            head.shorthand()
                .ok_or(GitError::NoBranch)?
                .to_string()
        }
    };

    let refspec = format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name);
    remote.push(&[&refspec], Some(&mut push_opts))?;

    Ok(())
}

/// Credential callback that mimics `git push` behavior
fn credential_callback(
    url: &str,
    username_from_url: Option<&str>,
    allowed_types: git2::CredentialType,
) -> std::result::Result<Cred, git2::Error> {
    // Try SSH agent first
    if allowed_types.contains(git2::CredentialType::SSH_KEY) {
        let username = username_from_url.unwrap_or("git");

        if let Ok(cred) = Cred::ssh_key_from_agent(username) {
            return Ok(cred);
        }

        // Try common SSH key locations
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        for key_name in &["id_ed25519", "id_rsa", "id_ecdsa"] {
            let private_key = Path::new(&home).join(".ssh").join(key_name);
            let public_key = Path::new(&home).join(".ssh").join(format!("{}.pub", key_name));

            if private_key.exists() {
                if let Ok(cred) = Cred::ssh_key(username, Some(&public_key), &private_key, None) {
                    return Ok(cred);
                }
            }
        }
    }

    // Try environment tokens for HTTPS
    if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
        for var in &["GITHUB_TOKEN", "GH_TOKEN", "GITLAB_TOKEN", "GIT_TOKEN"] {
            if let Ok(token) = env::var(var) {
                return Cred::userpass_plaintext("x-access-token", &token);
            }
        }

        // Use git credential helper (same as git push)
        let config = git2::Config::open_default()?;
        return Cred::credential_helper(&config, url, username_from_url);
    }

    if allowed_types.contains(git2::CredentialType::DEFAULT) {
        return Cred::default();
    }

    Err(git2::Error::from_str("no authentication available"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_error_display() {
        let err = GitError::RemoteNotFound("origin".to_string());
        assert!(err.to_string().contains("origin"));
    }
}
