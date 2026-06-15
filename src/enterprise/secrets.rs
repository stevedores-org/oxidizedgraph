//! Secret access abstraction and log redaction.

use serde::{Deserialize, Serialize};

/// Opaque secret handle — never stores raw secret material in events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretHandle {
    /// Secret name in the backing store.
    pub name: String,
    /// Tenant that owns the secret.
    pub tenant: String,
    /// Secret version for rotation tracking.
    pub version: u32,
}

impl SecretHandle {
    /// Create a handle.
    pub fn new(name: impl Into<String>, tenant: impl Into<String>, version: u32) -> Self {
        Self {
            name: name.into(),
            tenant: tenant.into(),
            version,
        }
    }

    /// Safe reference string for logs (no material).
    pub fn redacted_ref(&self) -> String {
        format!("secret://{}/{}@v{}", self.tenant, self.name, self.version)
    }
}

/// Scoped credential binding a secret to a permission scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedCredential {
    /// Secret handle.
    pub handle: SecretHandle,
    /// Allowed scopes (e.g. `git:read`, `dns:write`).
    pub scopes: Vec<String>,
}

impl ScopedCredential {
    /// Create a scoped credential.
    pub fn new(handle: SecretHandle, scopes: Vec<String>) -> Self {
        Self { handle, scopes }
    }

    /// Whether the credential allows a scope.
    pub fn allows_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope || s == "*")
    }
}

/// In-memory secret store for tests and local development.
#[derive(Clone, Debug, Default)]
pub struct SecretStore {
    secrets: std::collections::HashMap<String, String>,
}

impl SecretStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store secret material keyed by handle ref (never log the value).
    pub fn put(&mut self, handle: &SecretHandle, value: impl Into<String>) {
        self.secrets.insert(handle.redacted_ref(), value.into());
    }

    /// Resolve secret material for an authorized scope.
    pub fn resolve(&self, credential: &ScopedCredential, scope: &str) -> Option<&str> {
        if !credential.allows_scope(scope) {
            return None;
        }
        self.secrets
            .get(&credential.handle.redacted_ref())
            .map(|s| s.as_str())
    }
}

/// Redacts secret-like substrings from log/event text.
#[derive(Clone, Debug, Default)]
pub struct SecretRedactor {
    patterns: Vec<String>,
}

impl SecretRedactor {
    /// Create a redactor with default sensitive key names.
    pub fn enterprise_default() -> Self {
        Self {
            patterns: vec![
                "password".into(),
                "token".into(),
                "api_key".into(),
                "secret".into(),
                "private_key".into(),
                "authorization".into(),
            ],
        }
    }

    /// Add a key pattern to redact from `key=value` style strings.
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.patterns.push(pattern.into());
        self
    }

    /// Redact sensitive values in text. Returns sanitized string safe for logs.
    pub fn redact(&self, input: &str) -> String {
        let mut output = input.to_string();
        let lines: Vec<String> = input.lines().map(str::to_string).collect();
        for line in lines {
            let lower = line.to_ascii_lowercase();
            for pattern in &self.patterns {
                if lower.contains(pattern) {
                    if let Some((key, _)) = line.split_once('=') {
                        let replacement = format!("{key}=[REDACTED]");
                        output = output.replace(&line, &replacement);
                    } else if line.contains("Bearer ") {
                        output = output.replace(&line, "Authorization: Bearer [REDACTED]");
                    }
                }
            }
        }
        // Redact ghs_ and ghp_ GitHub tokens
        for token in find_tokens(output.as_str(), "ghs_")
            .into_iter()
            .chain(find_tokens(output.as_str(), "ghp_"))
        {
            output = output.replace(&token, "[REDACTED_TOKEN]");
        }
        output
    }

    /// Returns true when input appears to contain raw secret material.
    pub fn contains_exposed_secret(&self, input: &str) -> bool {
        let redacted = self.redact(input);
        redacted.contains("[REDACTED]") || redacted.contains("[REDACTED_TOKEN]")
    }
}

fn find_tokens(input: &str, prefix: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in input.split_whitespace() {
        if word.starts_with(prefix) {
            tokens.push(
                word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                    .to_string(),
            );
        }
    }
    tokens
}

/// Context key for scoped credentials list.
pub const CTX_SCOPED_CREDENTIALS: &str = "scoped_credentials";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_handle_never_embeds_material_in_ref() {
        let handle = SecretHandle::new("cf-token", "tenant-a", 2);
        assert_eq!(handle.redacted_ref(), "secret://tenant-a/cf-token@v2");
    }

    #[test]
    fn redactor_strips_token_patterns() {
        let redactor = SecretRedactor::enterprise_default();
        let sanitized =
            redactor.redact("api_key=supersecret123\nAuthorization: Bearer ghs_abc123token");
        assert!(!sanitized.contains("supersecret123"));
        assert!(!sanitized.contains("ghs_abc123token"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn scoped_credential_enforces_scope() {
        let handle = SecretHandle::new("deploy", "t1", 1);
        let cred = ScopedCredential::new(handle.clone(), vec!["dns:write".into()]);
        let mut store = SecretStore::new();
        store.put(&handle, "sekrit");
        assert!(store.resolve(&cred, "dns:write").is_some());
        assert!(store.resolve(&cred, "git:read").is_none());
    }
}
