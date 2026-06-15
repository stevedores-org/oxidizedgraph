use oxidizedgraph::governance::{AgentDiscovery, GovernanceValidator};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_agent_discovery() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    fs::write(base.join("CLAUDE.md"), "content").unwrap();

    // Create cursor dir
    fs::create_dir_all(base.join(".cursor/rules")).unwrap();
    fs::write(base.join(".cursor/rules/swarm.mdc"), "content").unwrap();

    let discovery = AgentDiscovery::new(base);
    let agents = discovery.scan();

    assert_eq!(agents.len(), 2);
    let names: Vec<_> = agents.iter().map(|a| a.name.clone()).collect();
    assert!(
        names.contains(&"<@claude>".to_string()),
        "Names: {:?}",
        names
    );
    assert!(
        names.contains(&"<@cursor>".to_string()),
        "Names: {:?}",
        names
    );
}

#[test]
fn test_governance_validation() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    let validator = GovernanceValidator::new(base);

    // Initial: should fail because AGENTS.md is missing
    let report = validator.validate_compliance();
    assert!(!report.is_compliant);
    assert!(report.issues.iter().any(|i| i.contains("missing")));

    // Fix it
    fs::write(base.join("AGENTS.md"), "Master").unwrap();

    let mut mgr = oxidizedgraph::governance::SymlinkManager::default(base);
    mgr.sync_all().unwrap();

    let report2 = validator.validate_compliance();
    assert!(report2.is_compliant);
}
