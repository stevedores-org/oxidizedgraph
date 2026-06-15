//! Enterprise readiness: RBAC, tenancy, secrets, audit, and SLO guardrails.

pub mod audit;
pub mod node;
pub mod secrets;
pub mod slo;
pub mod tenant;

pub use audit::{
    AuditEventFields, AuditLog, AuditRecord, ComplianceExport, ComplianceExporter, CTX_AUDIT_LOG,
};
pub use node::{AuditExportNode, BudgetGuardNode, SecretScopeNode, SloRecordNode, TenantGuardNode};
pub use secrets::{
    ScopedCredential, SecretHandle, SecretRedactor, SecretStore, CTX_SCOPED_CREDENTIALS,
};
pub use slo::{
    BudgetDecision, BudgetGuardrail, CostBudget, SloObservation, SloTarget, SloTracker,
    CTX_COST_BUDGET, CTX_SLO_TRACKER,
};
pub use tenant::{
    Permission, RbacPolicy, RbacRole, RbacSubject, TenantBoundaryResult, TenantGuard, TenantId,
    CTX_RBAC_SUBJECT, CTX_TENANT_ID,
};
