//! EPIC10 enterprise readiness workflow: tenant guard, secrets, audit export, SLO/budget.

use oxidizedgraph::prelude::*;

struct EnterpriseSetupNode;

#[async_trait]
impl NodeExecutor for EnterpriseSetupNode {
    fn id(&self) -> &str {
        "enterprise_setup"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let mut guard = state
            .write()
            .map_err(|e| NodeError::execution_failed(e.to_string()))?;

        guard.set_context(CTX_TENANT_ID, TenantId::new("acme-corp"));
        guard.set_context(
            CTX_RBAC_SUBJECT,
            RbacSubject::new(
                "ops@acme",
                TenantId::new("acme-corp"),
                vec![RbacRole::Operator],
            ),
        );
        guard.set_context(CTX_AUDIT_LOG, AuditLog::new());
        guard.set_context(CTX_COST_BUDGET, CostBudget::new("llm", "acme-corp", 500));
        guard.set_context(
            CTX_SCOPED_CREDENTIALS,
            vec![ScopedCredential::new(
                SecretHandle::new("deploy-key", "acme-corp", 1),
                vec!["git:read".into()],
            )],
        );

        let mut tracker = SloTracker::new();
        tracker.register(SloTarget::new("workflow_success", 0.95, 24));
        guard.set_context(CTX_SLO_TRACKER, tracker);
        guard.set_context("run_id", "demo-run-1".to_string());

        Ok(NodeOutput::cont())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = GraphBuilder::new()
        .add_node(EnterpriseSetupNode)
        .add_node(TenantGuardNode::new("tenant_guard", Permission::Execute))
        .add_node(SecretScopeNode::new("secret_scope", "git:read"))
        .add_node(BudgetGuardNode::new("budget_guard", 50))
        .add_node(SloRecordNode::new("slo_record", "workflow_success", true))
        .add_node(AuditExportNode::new("audit_export"))
        .add_node(EchoNode::new("complete", "Enterprise workflow complete"))
        .add_node(EchoNode::new("blocked", "Enterprise workflow blocked"))
        .set_entry_point("enterprise_setup")
        .add_edge("enterprise_setup", "tenant_guard")
        .add_edge_with_key("tenant_guard", "secret_scope", "allowed")
        .add_edge_with_key("tenant_guard", "blocked", "denied")
        .add_edge_with_key("secret_scope", "budget_guard", "scoped")
        .add_edge_with_key("secret_scope", "blocked", "missing_scope")
        .add_edge_with_key("budget_guard", "slo_record", "within_budget")
        .add_edge_with_key("budget_guard", "blocked", "over_budget")
        .add_edge("slo_record", "audit_export")
        .add_edge_with_key("audit_export", "complete", "exported")
        .add_edge_with_key("audit_export", "blocked", "failed_checks")
        .add_edge_to_end("complete")
        .add_edge_to_end("blocked")
        .compile()?;

    let runner = GraphRunner::with_defaults(graph);
    let state = AgentState::new();
    let result = runner.invoke(state).await?;

    let export: ComplianceExport = result.get_context("compliance_export").unwrap();
    let dashboard: std::collections::HashMap<String, SloObservation> =
        result.get_context("slo_dashboard").unwrap();

    println!(
        "Compliance export: {} records, chain_valid={}",
        export.records.len(),
        export.chain_valid
    );
    println!("SLO dashboard: {:?}", dashboard);
    println!("Workflow complete");

    Ok(())
}
