use oxidizedgraph::prelude::*;

#[test]
fn test_nix_diagnostic_error_creation() {
    let err = NodeError::DiagnosticError {
        target: "flake.nix".to_string(),
        message: "Syntax error: unexpected '}'".to_string(),
        line: Some(42),
    };

    assert!(err.to_string().contains("Diagnostic error in flake.nix"));
    assert!(err.to_string().contains("Syntax error: unexpected '}'"));
}

#[test]
fn test_governance_violation_error() {
    let err = NodeError::GovernanceViolation("Unauthorized tool usage".to_string());
    assert_eq!(
        err.to_string(),
        "Governance violation: Unauthorized tool usage"
    );
}
