# Roadmap: Autonomous AI Agent Orchestration (#18)

This document tracks implementation of [Issue #18](https://github.com/stevedores-org/oxidizedgraph/issues/18) in oxidizedgraph.

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

## Delivery Phases (Remaining)

- **Phase 2 (remaining)**: EPIC5, EPIC8 — memory/retrieval, human-in-the-loop
- **Phase 3**: EPIC6, EPIC7, EPIC9 — planning, self-healing, multi-repo CI/CD
- **Phase 4**: EPIC10 — enterprise hardening

## North-Star KPIs

| KPI | Target |
|-----|--------|
| Autonomous task completion rate | ≥ 60% |
| First-pass CI success (agent PRs) | ≥ 85% |
| Mean time to recover from failed run | < 30 min |
| Human approvals per merged PR | Trending down |
| Defect escape rate | Non-inferior to human baseline |

## Related Issues

- [#18](https://github.com/stevedores-org/oxidizedgraph/issues/18) — Roadmap parent
- [#22](https://github.com/stevedores-org/oxidizedgraph/issues/22) — EPIC4 Code Quality Guardrails
- [#35](https://github.com/stevedores-org/oxidizedgraph/issues/35) — GovernanceNode
- [#36](https://github.com/stevedores-org/oxidizedgraph/issues/36) — Agent Role-Based Routing
