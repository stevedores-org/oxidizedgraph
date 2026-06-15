//! Multi-tenant RBAC and tenancy isolation.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Tenant identifier for isolation boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    /// Create a tenant id.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// RBAC roles for enterprise policy enforcement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RbacRole {
    /// Read-only observer.
    Viewer,
    /// Can execute workflows in assigned tenant.
    Operator,
    /// Can manage secrets and policies.
    Admin,
    /// Platform superuser across tenants.
    PlatformAdmin,
}

/// Subject performing an action (user or service account).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RbacSubject {
    /// Subject id (email, SA name).
    pub id: String,
    /// Home tenant for the subject.
    pub tenant: TenantId,
    /// Granted roles.
    pub roles: Vec<RbacRole>,
}

impl RbacSubject {
    /// Create a subject with roles.
    pub fn new(
        id: impl Into<String>,
        tenant: TenantId,
        roles: Vec<RbacRole>,
    ) -> Self {
        Self {
            id: id.into(),
            tenant,
            roles,
        }
    }

    /// Whether the subject has a role.
    pub fn has_role(&self, role: RbacRole) -> bool {
        self.roles.contains(&role) || self.roles.contains(&RbacRole::PlatformAdmin)
    }
}

/// Permission required for an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read workflow state and audit artifacts.
    Read,
    /// Execute graphs and tools.
    Execute,
    /// Manage tenant secrets and credentials.
    ManageSecrets,
    /// Export compliance audit bundles.
    ExportAudit,
    /// Cross-tenant platform administration.
    Administer,
}

/// RBAC policy mapping roles to permissions.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RbacPolicy {
    role_permissions: HashMap<RbacRole, HashSet<Permission>>,
}

impl RbacPolicy {
    /// Default enterprise policy.
    pub fn enterprise_default() -> Self {
        let mut policy = Self::default();
        policy.grant(RbacRole::Viewer, Permission::Read);
        policy.grant(RbacRole::Operator, Permission::Read);
        policy.grant(RbacRole::Operator, Permission::Execute);
        policy.grant(RbacRole::Admin, Permission::Read);
        policy.grant(RbacRole::Admin, Permission::Execute);
        policy.grant(RbacRole::Admin, Permission::ManageSecrets);
        policy.grant(RbacRole::Admin, Permission::ExportAudit);
        policy.grant(RbacRole::PlatformAdmin, Permission::Read);
        policy.grant(RbacRole::PlatformAdmin, Permission::Execute);
        policy.grant(RbacRole::PlatformAdmin, Permission::ManageSecrets);
        policy.grant(RbacRole::PlatformAdmin, Permission::ExportAudit);
        policy.grant(RbacRole::PlatformAdmin, Permission::Administer);
        policy
    }

    /// Grant a permission to a role.
    pub fn grant(&mut self, role: RbacRole, permission: Permission) {
        self.role_permissions
            .entry(role)
            .or_default()
            .insert(permission);
    }

    /// Check whether any of the subject's roles grants the permission.
    pub fn allows(&self, subject: &RbacSubject, permission: Permission) -> bool {
        subject.roles.iter().any(|role| {
            self.role_permissions
                .get(role)
                .map(|perms| perms.contains(&permission))
                .unwrap_or(false)
        })
    }
}

/// Tenant boundary enforcement result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantBoundaryResult {
    /// Whether access is allowed.
    pub allowed: bool,
    /// Reason when denied.
    pub reason: String,
}

/// Enforces tenant isolation and RBAC checks.
#[derive(Clone, Debug, Default)]
pub struct TenantGuard;

impl TenantGuard {
    /// Create a tenant guard.
    pub fn new() -> Self {
        Self
    }

    /// Verify a subject may access a resource in `resource_tenant`.
    pub fn check_access(
        &self,
        subject: &RbacSubject,
        resource_tenant: &TenantId,
        permission: Permission,
        policy: &RbacPolicy,
    ) -> TenantBoundaryResult {
        if !policy.allows(subject, permission) {
            return TenantBoundaryResult {
                allowed: false,
                reason: format!("subject '{}' lacks {:?}", subject.id, permission),
            };
        }

        let cross_tenant = subject.tenant != *resource_tenant;
        if cross_tenant && !subject.has_role(RbacRole::PlatformAdmin) {
            return TenantBoundaryResult {
                allowed: false,
                reason: format!(
                    "tenant boundary violation: subject tenant {} cannot access {}",
                    subject.tenant.0, resource_tenant.0
                ),
            };
        }

        TenantBoundaryResult {
            allowed: true,
            reason: String::new(),
        }
    }
}

/// Context key for active tenant id.
pub const CTX_TENANT_ID: &str = "tenant_id";
/// Context key for RBAC subject.
pub const CTX_RBAC_SUBJECT: &str = "rbac_subject";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_cross_tenant_access_for_operator() {
        let guard = TenantGuard::new();
        let policy = RbacPolicy::enterprise_default();
        let subject = RbacSubject::new(
            "alice",
            TenantId::new("tenant-a"),
            vec![RbacRole::Operator],
        );
        let result = guard.check_access(
            &subject,
            &TenantId::new("tenant-b"),
            Permission::Execute,
            &policy,
        );
        assert!(!result.allowed);
        assert!(result.reason.contains("tenant boundary"));
    }

    #[test]
    fn platform_admin_cross_tenant_allowed() {
        let guard = TenantGuard::new();
        let policy = RbacPolicy::enterprise_default();
        let subject = RbacSubject::new(
            "root",
            TenantId::new("platform"),
            vec![RbacRole::PlatformAdmin],
        );
        let result = guard.check_access(
            &subject,
            &TenantId::new("tenant-b"),
            Permission::Administer,
            &policy,
        );
        assert!(result.allowed);
    }
}
