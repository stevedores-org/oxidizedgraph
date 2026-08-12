# Roadmap: Autonomous AI Agent Orchestration (#18)

This document tracks implementation of [Issue #18](aivcs://stevedores-org/oxidizedgraph/issues/18) in oxidizedgraph.

## Desired Outcome

Agents can reliably plan, code, test, review, recover from failures, and ship changes with bounded risk, traceability, and human oversight.

## Phase 1 Baseline (Implemented)

| Epic | Module | Capabilities |
|------|--------|--------------|
| EPIC1 Deterministic Execution Core | `execution` | `RunContext`, `TransitionLog`, `TracedRunner`, `ReplayRunner`, `StateValidator` |
| EPIC3 Tooling & Sandbox | `tools` | `ToolPolicyEngine`, `SubprocessSandbox`, `ToolNodeConfig` (timeout + policy) |
| EPIC4 Code Quality Guardrails | `guardrails`, `QualityGateNode` | `GateResult`, `ReviewFinding`, `RiskClassifier`, merge routing |

## Phase 2 Baseline — EPIC2 Role Orchestration (Implemented)

| Component | Module | Capabilities |
|-----------|--------|--------------|
| GovernanceNode | `governance::node` | Loads manifest, applies role-scoped guidance to `AgentState` |
| Role routing | `governance::routing` | `RoleRouterNode`, `RoleHandoffNode`, `RoleTargetRouterNode` |
| Tool restrictions | `governance::config` | `tool_policy_for_role` — architect read-only, builder implement, auditor verify, human approval |
| Guidance composition | `governance::guidance` | `compose_guidance`, `role_system_prompt`, context keys (`agent_role`, `governance_guidance`) |

### Examples

```bash
cargo run --example autonomous_dev_workflow
cargo run --example role_orchestration_workflow
cargo run --example planning_workflow
cargo run --example hitl_workflow
```

### API Quick Reference

```rust
use oxidizedgraph::prelude::*;

// Traced execution with run correlation
let runner = TracedRunner::with_context(graph, RunContext::new(), RunnerConfig::default());
let result = runner.invoke(AgentState::new()).await?;

// Quality gate in graph (routes: passed | review | needs_approval | gate_failed)
let gate = QualityGateNode::new("quality_gate", QualityGateConfig::rust_defaults());

// Role orchestration (EPIC2)
let graph = GraphBuilder::new()
    .add_node(GovernanceNode::new("gov", manifest, AgentRole::Builder))
    .add_node(RoleHandoffNode::new("to_auditor", manifest, AgentRole::Auditor))
    .add_node(RoleRouterNode::new("route", "default"))
    // wire edges with add_edge_with_key(..., role.as_tag())
    .compile()?;

let policy = tool_policy_for_role(&AgentRole::Architect);
```

Quality gates use `RiskClassifier::approval_route` so medium-risk changes route to a `review` edge when checks pass.

## Phase 3 Baseline — EPIC6 Planning & Autonomy (Implemented)

| Component | Module | Capabilities |
|-----------|--------|--------------|
| Plan model | `planning::plan` | `EpicPlan`, `Task`, `TaskStatus`, recovery injection |
| Scheduler | `planning::scheduler` | Dependency resolution, cycle detection, critical-path prioritization |
| Progress | `planning::progress` | `PlanProgress` with confidence score and ETA estimates |
| Graph nodes | `planning::node` | `PlanningNode` (decomposition), `SchedulerNode` (schedule + auto-replan) |

### Example

```bash
cargo run --example planning_workflow
```

### API Quick Reference

```rust
use oxidizedgraph::prelude::*;

let planner = PlanningNode::with_decomposer("planner", |goal| {
    vec![Task::new("t1", "Step 1", format!("Work on {goal}"))]
});

let scheduler = SchedulerNode::new("scheduler")
    .register_recovery("t1", Task::new("recover", "Recover", "Fix failure"));

let progress = PlanProgress::calculate(&plan);
```

## Phase 2 Baseline — EPIC8 Human-in-the-Loop (Implemented)

| Component | Module | Capabilities |
|-----------|--------|--------------|
| Approval policy | `hitl::policy` | `ApprovalMatrix`, `ApprovalPolicy` — risk class → pause/allow/deny |
| Checkpoint types | `hitl::types` | `ApprovalRequest`, `ApprovalDecision`, `ExplanationPayload`, immutable `ApprovalEvent` audit log |
| Graph nodes | `hitl::node` | `ApprovalCheckpointNode`, `GrantApprovalNode`, `ResumeNode` |
| Operator timeline | `hitl::timeline` | `RunTimeline` merges transition log + approval events |

### Example

```bash
cargo run --example hitl_workflow
```

## Phase 2 Baseline — EPIC5 Memory & Retrieval (Implemented)

| Component | Module | Capabilities |
|-----------|--------|--------------|
| Repository index | `memory` | `RepositoryIndex`, `RetrievalQuery` — lexical ranking with path/symbol filters |
| Episodic memory | `memory` | `AgentMemoryStore`, `EpisodicMemory` — prior runs keyed by task/run/repo |
| Decision memory | `memory` | `DecisionMemory` — queryable rationale for major changes |
| Context packing | `memory` | `ContextPacker`, `ContextPolicy` — token-budgeted prompt assembly |

### Example

```bash
cargo run --example memory_workflow
cargo run --example multirepo_cicd_workflow
```

### API Quick Reference

```rust
use oxidizedgraph::prelude::*;

let mut index = RepositoryIndex::new();
index.index_document(RepositoryDocument::source(repo, path, content));

let hits = index.query(&RetrievalQuery::new("context packing").repo(repo));
let store = AgentMemoryStore::new();
let packed = ContextPacker::new(8_000).pack(&hits, &episodes, &decisions, &ContextPolicy::default());
```

## Phase 3 Baseline — EPIC9 Multi-Repo CI/CD (Implemented)

| Component | Module | Capabilities |
|-----------|--------|--------------|
| Change graph | `cicd::change_graph` | `CrossRepoChangeGraph`, `RepoChange` — dependency-aware multi-repo changes |
| Coordinator | `cicd::coordinator` | `MultiRepoCoordinator` — ordered execution, failure propagation |
| CI aggregation | `cicd::ci_aggregate` | `CiAggregator`, `CiAggregateReport` — objective-level CI consolidation |
| Release gating | `cicd::release` | `ReleaseOrchestrator`, `ReleaseBatch` — blocks rollout on downstream breakage |
| Graph nodes | `cicd::node` | `MultiRepoCoordinatorNode`, `CiAggregateNode`, `ReleaseGateNode` |

### Example

```bash
cargo run --example multirepo_cicd_workflow
cargo run --example enterprise_workflow
```

### API Quick Reference

```rust
use oxidizedgraph::prelude::*;

let guard = TenantGuard::new();
let result = guard.check_access(&subject, &tenant, Permission::Execute, &RbacPolicy::enterprise_default());

let redactor = SecretRedactor::enterprise_default();
let safe = redactor.redact(log_line);

let export = ComplianceExporter::new().export_tenant(&audit_log, "acme-corp");
```

## Phase 4 Baseline — EPIC10 Enterprise Readiness (Implemented)

| Component | Module | Capabilities |
|-----------|--------|--------------|
| Tenancy & RBAC | `enterprise::tenant` | `TenantGuard`, `RbacPolicy`, `Permission` — cross-tenant boundary enforcement |
| Secrets | `enterprise::secrets` | `SecretHandle`, `ScopedCredential`, `SecretRedactor` — scoped access without log exposure |
| Audit | `enterprise::audit` | `AuditLog` hash chain, `ComplianceExporter` — immutable compliance export |
| SLO & budget | `enterprise::slo` | `SloTracker`, `BudgetGuardrail`, `CostBudget` — error budget and spend guardrails |
| Graph nodes | `enterprise::node` | `TenantGuardNode`, `SecretScopeNode`, `BudgetGuardNode`, `SloRecordNode`, `AuditExportNode` |

### Example

```bash
cargo run --example enterprise_workflow
```

## Delivery Phases

All roadmap epics for #18 are implemented on `develop`.

## North-Star KPIs

| KPI | Target |
|-----|--------|
| Autonomous task completion rate | ≥ 60% |
| First-pass CI success (agent PRs) | ≥ 85% |
| Mean time to recover from failed run | < 30 min |
| Human approvals per merged PR | Trending down |
| Defect escape rate | Non-inferior to human baseline |

## Related Issues

- [#18](aivcs://stevedores-org/oxidizedgraph/issues/18) — Roadmap parent
- [#22](aivcs://stevedores-org/oxidizedgraph/issues/22) — EPIC4 Code Quality Guardrails
- [#24](aivcs://stevedores-org/oxidizedgraph/issues/24) — EPIC6 Planning and Long-Horizon Autonomy
- [#23](aivcs://stevedores-org/oxidizedgraph/issues/23) — EPIC5 Memory, Context, and Retrieval
- [#27](aivcs://stevedores-org/oxidizedgraph/issues/27) — EPIC9 Multi-Repo and CI/CD Orchestration
- [#28](aivcs://stevedores-org/oxidizedgraph/issues/28) — EPIC10 Enterprise Readiness
- [#26](aivcs://stevedores-org/oxidizedgraph/issues/26) — EPIC8 Human-in-the-Loop Controls
- [#35](aivcs://stevedores-org/oxidizedgraph/issues/35) — GovernanceNode
- [#36](aivcs://stevedores-org/oxidizedgraph/issues/36) — Agent Role-Based Routing
