//! Integration tests for EPIC10 enterprise readiness.

use oxidizedgraph::prelude::*;

#[test]
fn tenant_boundary_blocks_cross_tenant_operator() {
    let guard = TenantGuard::new();
    let policy = RbacPolicy::enterprise_default();
    let subject = RbacSubject::new("bob", TenantId::new("tenant-a"), vec![RbacRole::Operator]);
    let result = guard.check_access(
        &subject,
        &TenantId::new("tenant-b"),
        Permission::Execute,
        &policy,
    );
    assert!(!result.allowed);
}

#[test]
fn secret_redactor_prevents_log_exposure() {
    let redactor = SecretRedactor::enterprise_default();
    let raw = "api_key=not-for-logs token=abc123";
    let sanitized = redactor.redact(raw);
    assert!(!sanitized.contains("not-for-logs"));
    assert!(sanitized.contains("[REDACTED]"));
}

#[test]
fn audit_export_passes_compliance_checks() {
    let mut log = AuditLog::new();
    log.append(AuditEventFields::new(
        "acme",
        "run-1",
        "alice",
        "execute",
        "workflow",
        "allowed",
        "sanitized event",
    ));
    let exporter = ComplianceExporter::new();
    let export = exporter.export_tenant(&log, "acme");
    assert!(export.chain_valid);
    assert!(exporter.passes_internal_checks(&export));
}

#[test]
fn slo_tracker_meets_target() {
    let mut tracker = SloTracker::new();
    tracker.register(SloTarget::new("workflow_success", 0.9, 24));
    for _ in 0..9 {
        tracker.record("workflow_success", true);
    }
    tracker.record("workflow_success", false);
    let obs = tracker.evaluate("workflow_success").unwrap();
    assert!(obs.met);
}

#[test]
fn budget_guardrail_blocks_overspend() {
    let budget = CostBudget::new("spend", "t1", 100);
    let guard = BudgetGuardrail::new();
    assert!(!guard.check(&budget, 200).allowed);
}

#[tokio::test]
async fn tenant_guard_node_denies_cross_tenant() {
    let graph = GraphBuilder::new()
        .add_node(TenantGuardNode::new("guard", Permission::Execute))
        .add_node(EchoNode::new("end", "guard complete"))
        .set_entry_point("guard")
        .add_edge_with_key("guard", "end", "allowed")
        .add_edge_with_key("guard", "end", "denied")
        .add_edge_to_end("end")
        .compile()
        .unwrap();

    let mut state = AgentState::new();
    state.set_context(CTX_TENANT_ID, TenantId::new("tenant-b"));
    state.set_context(
        CTX_RBAC_SUBJECT,
        RbacSubject::new("alice", TenantId::new("tenant-a"), vec![RbacRole::Operator]),
    );
    state.set_context(CTX_AUDIT_LOG, AuditLog::new());

    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(state).await.unwrap();
    let audit: AuditLog = result.get_context(CTX_AUDIT_LOG).unwrap();
    assert!(audit.records().iter().any(|r| r.outcome == "denied"));
}
