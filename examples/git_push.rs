//! Git push example using git2
//!
//! Demonstrates pushing to GitHub using libgit2 bindings.
//!
//! Usage: cargo run --example git_push

use git2::{Cred, PushOptions, RemoteCallbacks, Repository};
use std::env;
use std::path::Path;

fn main() -> Result<(), git2::Error> {
    // Open repository (current directory or specify path)
    let repo_path = env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let repo = Repository::open(Path::new(&repo_path))?;

    println!("Opened repository: {:?}", repo.path());

    // Get the remote (default: origin)
    let remote_name = env::args().nth(2).unwrap_or_else(|| "origin".to_string());
    let mut remote = repo.find_remote(&remote_name)?;

    println!("Remote '{}' URL: {:?}", remote_name, remote.url());

    // Set up callbacks for authentication
    let mut callbacks = RemoteCallbacks::new();

    // Credential callback - tries multiple auth methods
    callbacks.credentials(|url, username_from_url, allowed_types| {
        println!("Auth requested for: {}", url);
        println!("Username hint: {:?}", username_from_url);
        println!("Allowed types: {:?}", allowed_types);

        // Try SSH agent first (most common for GitHub)
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            let username = username_from_url.unwrap_or("git");

            // Try SSH agent
            if let Ok(cred) = Cred::ssh_key_from_agent(username) {
                println!("Using SSH agent");
                return Ok(cred);
            }

            // Fall back to default SSH key
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let private_key = Path::new(&home).join(".ssh/id_ed25519");
            let public_key = Path::new(&home).join(".ssh/id_ed25519.pub");

            // Try ed25519 key
            if private_key.exists() {
                println!("Using SSH key: {:?}", private_key);
                return Cred::ssh_key(username, Some(&public_key), &private_key, None);
            }

            // Try RSA key
            let private_key = Path::new(&home).join(".ssh/id_rsa");
            let public_key = Path::new(&home).join(".ssh/id_rsa.pub");
            if private_key.exists() {
                println!("Using SSH key: {:?}", private_key);
                return Cred::ssh_key(username, Some(&public_key), &private_key, None);
            }
        }

        // Try user/pass or token for HTTPS
        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            // GitHub personal access token
            if let Ok(token) = env::var("GITHUB_TOKEN") {
                println!("Using GITHUB_TOKEN");
                return Cred::userpass_plaintext("x-access-token", &token);
            }

            // Or GH_TOKEN (used by gh CLI)
            if let Ok(token) = env::var("GH_TOKEN") {
                println!("Using GH_TOKEN");
                return Cred::userpass_plaintext("x-access-token", &token);
            }
        }

        // Default credentials from git config
        if allowed_types.contains(git2::CredentialType::DEFAULT) {
            println!("Using default credentials");
            return Cred::default();
        }

        // Try git credential helper (like git push uses)
        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            println!("Trying git credential helper...");
            let config = git2::Config::open_default()?;
            return Cred::credential_helper(&config, url, username_from_url);
        }

        Err(git2::Error::from_str("no authentication available"))
    });

    // Progress callback
    callbacks.push_update_reference(|refname, status| {
        match status {
            None => println!("Updated {}", refname),
            Some(msg) => println!("Failed to update {}: {}", refname, msg),
        }
        Ok(())
    });

    callbacks.push_transfer_progress(|current, total, bytes| {
        println!("Transfer: {}/{} objects, {} bytes", current, total, bytes);
    });

    // Configure push options
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    // Get current branch
    let head = repo.head()?;
    let branch_name = head
        .shorthand()
        .ok_or_else(|| git2::Error::from_str("HEAD is not a branch"))?;

    // Build refspec: push current branch to same name on remote
    let refspec = format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name);
    println!("Pushing: {}", refspec);

    // Push!
    remote.push(&[&refspec], Some(&mut push_opts))?;

    println!("Push complete!");
    Ok(())
}
