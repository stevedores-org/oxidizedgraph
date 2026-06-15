//! Graph nodes for enterprise policy enforcement.

use async_trait::async_trait;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::state::SharedState;

use super::audit::{AuditEventFields, AuditLog, ComplianceExporter, CTX_AUDIT_LOG};
use super::secrets::{SecretRedactor, CTX_SCOPED_CREDENTIALS};
use super::slo::{BudgetGuardrail, CostBudget, CTX_COST_BUDGET, CTX_SLO_TRACKER, SloTracker};
use super::tenant::{
    Permission, RbacPolicy, RbacSubject, TenantGuard, TenantId, CTX_RBAC_SUBJECT, CTX_TENANT_ID,
};

/// Enforces tenant boundary and RBAC before continuing execution.
#[derive(Clone, Debug)]
pub struct TenantGuardNode {
    id: String,
    guard: TenantGuard,
    policy: RbacPolicy,
    permission: Permission,
}

impl TenantGuardNode {
    /// Create a tenant guard node requiring `permission`.
    pub fn new(id: impl Into<String>, permission: Permission) -> Self {
        Self {
            id: id.into(),
            guard: TenantGuard::new(),
            policy: RbacPolicy::enterprise_default(),
            permission,
        }
    }

    /// Override the RBAC policy.
    pub fn with_policy(mut self, policy: RbacPolicy) -> Self {
        self.policy = policy;
        self
    }
}

#[async_trait]
impl NodeExecutor for TenantGuardNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let subject: RbacSubject = guard
            .get_context(CTX_RBAC_SUBJECT)
            .ok_or_else(|| NodeError::execution_failed("missing rbac_subject".to_string()))?;
        let tenant: TenantId = guard
            .get_context(CTX_TENANT_ID)
            .ok_or_else(|| NodeError::execution_failed("missing tenant_id".to_string()))?;

        let result = self
            .guard
            .check_access(&subject, &tenant, self.permission, &self.policy);

        let mut audit: AuditLog = guard.get_context(CTX_AUDIT_LOG).unwrap_or_default();
        let redactor = SecretRedactor::enterprise_default();
        let run_id = guard
            .get_context::<String>("run_id")
            .unwrap_or_else(|| "unknown".into());
        let detail = redactor.redact(&format!(
            "permission={:?} tenant={}",
            self.permission, tenant.0
        ));
        audit.append(AuditEventFields::new(
            tenant.0.clone(),
            run_id,
            subject.id.clone(),
            "tenant_guard",
            "workflow",
            if result.allowed {
                "allowed"
            } else {
                "denied"
            },
            detail,
        ));
        guard.set_context(CTX_AUDIT_LOG, audit);

        if result.allowed {
            Ok(NodeOutput::transition("allowed"))
        } else {
            Ok(NodeOutput::transition("denied"))
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Enforces tenant isolation and RBAC permissions")
    }
}

/// Exports compliance audit bundle for the active tenant.
#[derive(Clone, Debug, Default)]
pub struct AuditExportNode {
    id: String,
    exporter: ComplianceExporter,
}

impl AuditExportNode {
    /// Create an audit export node.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            exporter: ComplianceExporter::new(),
        }
    }
}

#[async_trait]
impl NodeExecutor for AuditExportNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let tenant: TenantId = guard
            .get_context(CTX_TENANT_ID)
            .ok_or_else(|| NodeError::execution_failed("missing tenant_id".to_string()))?;
        let audit: AuditLog = guard
            .get_context(CTX_AUDIT_LOG)
            .ok_or_else(|| NodeError::execution_failed("missing audit_log".to_string()))?;

        let export = self.exporter.export_tenant(&audit, &tenant.0);
        let passes = self.exporter.passes_internal_checks(&export);
        guard.set_context("compliance_export", export);

        if passes {
            Ok(NodeOutput::transition("exported"))
        } else {
            Ok(NodeOutput::transition("failed_checks"))
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Exports tenant-scoped compliance audit bundle")
    }
}

/// Applies budget guardrails before expensive operations.
#[derive(Clone, Debug, Default)]
pub struct BudgetGuardNode {
    id: String,
    guard: BudgetGuardrail,
    incremental_cost: u64,
}

impl BudgetGuardNode {
    /// Create a budget guard node with incremental cost estimate.
    pub fn new(id: impl Into<String>, incremental_cost: u64) -> Self {
        Self {
            id: id.into(),
            guard: BudgetGuardrail::new(),
            incremental_cost,
        }
    }
}

#[async_trait]
impl NodeExecutor for BudgetGuardNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let mut budget: CostBudget = guard
            .get_context(CTX_COST_BUDGET)
            .ok_or_else(|| NodeError::execution_failed("missing cost_budget".to_string()))?;

        let decision = self.guard.check(&budget, self.incremental_cost);
        if decision.allowed {
            BudgetGuardrail::charge(&mut budget, self.incremental_cost);
            guard.set_context(CTX_COST_BUDGET, budget);
            Ok(NodeOutput::transition("within_budget"))
        } else {
            Ok(NodeOutput::transition("over_budget"))
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Enforces tenant cost budget guardrails")
    }
}

/// Records workflow outcome against registered SLO targets.
#[derive(Clone, Debug)]
pub struct SloRecordNode {
    id: String,
    slo_name: String,
    success: bool,
}

impl SloRecordNode {
    /// Create an SLO record node.
    pub fn new(id: impl Into<String>, slo_name: impl Into<String>, success: bool) -> Self {
        Self {
            id: id.into(),
            slo_name: slo_name.into(),
            success,
        }
    }
}

#[async_trait]
impl NodeExecutor for SloRecordNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let mut tracker: SloTracker = guard
            .get_context(CTX_SLO_TRACKER)
            .unwrap_or_default();
        tracker.record(&self.slo_name, self.success);
        let dashboard = tracker.dashboard();
        guard.set_context(CTX_SLO_TRACKER, tracker);
        guard.set_context("slo_dashboard", dashboard);

        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Records run outcomes for SLO tracking")
    }
}

/// Ensures scoped credentials are present without exposing secret material.
#[derive(Clone, Debug, Default)]
pub struct SecretScopeNode {
    id: String,
    required_scope: String,
}

impl SecretScopeNode {
    /// Create a node that validates a required credential scope.
    pub fn new(id: impl Into<String>, required_scope: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            required_scope: required_scope.into(),
        }
    }
}

#[async_trait]
impl NodeExecutor for SecretScopeNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let guard = state
            .read()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        let credentials: Vec<super::secrets::ScopedCredential> =
            guard.get_context(CTX_SCOPED_CREDENTIALS).unwrap_or_default();

        let has_scope = credentials
            .iter()
            .any(|c| c.allows_scope(&self.required_scope));

        if has_scope {
            Ok(NodeOutput::transition("scoped"))
        } else {
            Ok(NodeOutput::transition("missing_scope"))
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Validates scoped credentials without resolving secret material")
    }
}
