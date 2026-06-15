//! SLO tracking, error budgets, and cost guardrails.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Reliability SLO target for a service or workflow class.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SloTarget {
    /// SLO name (e.g. `workflow_success`).
    pub name: String,
    /// Target success rate 0.0–1.0.
    pub target_success_rate: f64,
    /// Error budget as fraction (1 - target).
    pub error_budget: f64,
    /// Measurement window in hours.
    pub window_hours: u32,
}

impl SloTarget {
    /// Create an SLO target.
    pub fn new(name: impl Into<String>, target_success_rate: f64, window_hours: u32) -> Self {
        let clamped = target_success_rate.clamp(0.0, 1.0);
        Self {
            name: name.into(),
            target_success_rate: clamped,
            error_budget: 1.0 - clamped,
            window_hours,
        }
    }
}

/// Observed SLO metrics for a window.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SloObservation {
    /// Total attempts.
    pub attempts: u64,
    /// Successful attempts.
    pub successes: u64,
    /// Observed success rate.
    pub success_rate: f64,
    /// Remaining error budget fraction.
    pub remaining_error_budget: f64,
    /// Whether SLO is met.
    pub met: bool,
}

/// Tracks SLO compliance from run outcomes.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SloTracker {
    targets: HashMap<String, SloTarget>,
    attempts: HashMap<String, u64>,
    successes: HashMap<String, u64>,
}

impl SloTracker {
    /// Create an empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an SLO target.
    pub fn register(&mut self, target: SloTarget) {
        let name = target.name.clone();
        self.targets.insert(name.clone(), target);
        self.attempts.entry(name.clone()).or_insert(0);
        self.successes.entry(name).or_insert(0);
    }

    /// Record a run outcome.
    pub fn record(&mut self, slo_name: &str, success: bool) {
        *self.attempts.entry(slo_name.to_string()).or_insert(0) += 1;
        if success {
            *self.successes.entry(slo_name.to_string()).or_insert(0) += 1;
        }
    }

    /// Evaluate SLO for a named target.
    pub fn evaluate(&self, slo_name: &str) -> Option<SloObservation> {
        let target = self.targets.get(slo_name)?;
        let attempts = *self.attempts.get(slo_name).unwrap_or(&0);
        let successes = *self.successes.get(slo_name).unwrap_or(&0);
        if attempts == 0 {
            return Some(SloObservation {
                attempts: 0,
                successes: 0,
                success_rate: 1.0,
                remaining_error_budget: target.error_budget,
                met: true,
            });
        }
        let success_rate = successes as f64 / attempts as f64;
        let failures = attempts - successes;
        let failure_rate = failures as f64 / attempts as f64;
        let remaining = (target.error_budget - failure_rate).max(0.0);
        Some(SloObservation {
            attempts,
            successes,
            success_rate,
            remaining_error_budget: remaining,
            met: success_rate >= target.target_success_rate,
        })
    }

    /// Dashboard snapshot for all registered SLOs.
    pub fn dashboard(&self) -> HashMap<String, SloObservation> {
        self.targets
            .keys()
            .filter_map(|name| self.evaluate(name).map(|obs| (name.clone(), obs)))
            .collect()
    }
}

/// Cost budget for a tenant or workflow.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CostBudget {
    /// Budget id.
    pub id: String,
    /// Tenant scope.
    pub tenant_id: String,
    /// Maximum spend in abstract units (tokens, USD cents, etc.).
    pub limit: u64,
    /// Current spend.
    pub spent: u64,
}

impl CostBudget {
    /// Create a budget.
    pub fn new(id: impl Into<String>, tenant_id: impl Into<String>, limit: u64) -> Self {
        Self {
            id: id.into(),
            tenant_id: tenant_id.into(),
            limit,
            spent: 0,
        }
    }

    /// Remaining budget.
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.spent)
    }

    /// Whether spend is within limit.
    pub fn within_limit(&self) -> bool {
        self.spent <= self.limit
    }
}

/// Budget guardrail decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetDecision {
    /// Whether the operation is allowed.
    pub allowed: bool,
    /// Reason when denied.
    pub reason: String,
}

/// Enforces cost and capacity guardrails.
#[derive(Clone, Debug, Default)]
pub struct BudgetGuardrail;

impl BudgetGuardrail {
    /// Create a guardrail.
    pub fn new() -> Self {
        Self
    }

    /// Check whether an incremental cost is allowed.
    pub fn check(&self, budget: &CostBudget, incremental: u64) -> BudgetDecision {
        if budget.spent.saturating_add(incremental) > budget.limit {
            return BudgetDecision {
                allowed: false,
                reason: format!(
                    "budget {} exceeded: {} + {} > {}",
                    budget.id, budget.spent, incremental, budget.limit
                ),
            };
        }
        BudgetDecision {
            allowed: true,
            reason: String::new(),
        }
    }

    /// Apply spend after an allowed operation.
    pub fn charge(budget: &mut CostBudget, amount: u64) {
        budget.spent = budget.spent.saturating_add(amount);
    }
}

/// Context keys for enterprise observability.
pub const CTX_SLO_TRACKER: &str = "slo_tracker";
/// Context key for active cost budget.
pub const CTX_COST_BUDGET: &str = "cost_budget";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slo_met_when_above_target() {
        let mut tracker = SloTracker::new();
        tracker.register(SloTarget::new("workflow_success", 0.95, 24));
        for _ in 0..95 {
            tracker.record("workflow_success", true);
        }
        for _ in 0..5 {
            tracker.record("workflow_success", false);
        }
        let obs = tracker.evaluate("workflow_success").unwrap();
        assert!(obs.met);
        assert!((obs.success_rate - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn budget_guardrail_blocks_overspend() {
        let budget = CostBudget::new("llm-spend", "t1", 100);
        let guard = BudgetGuardrail::new();
        let decision = guard.check(&budget, 150);
        assert!(!decision.allowed);
    }
}
