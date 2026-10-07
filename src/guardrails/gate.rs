//! Quality gate node for CI-style checks in agent graphs.

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

use crate::error::NodeError;
use crate::graph::{NodeExecutor, NodeOutput};
use crate::guardrails::findings::{GateCheck, GateResult, ReviewFinding};
use crate::guardrails::risk::{ChangeRisk, RiskClassifier};
use crate::state::SharedState;

use std::collections::HashSet;

/// Default list of executables allowed to run as quality gates.
pub const DEFAULT_ALLOWED_PROGRAMS: &[&str] = &[
    "cargo", "git", "npm", "yarn", "pnpm", "bun", "pytest", "python", "python3", "go", "make",
    "clippy", "ruff", "flake8", "deno", "gradle", "mvn", "dotnet", "true", "false", "echo",
];

/// Specification for a command to run as a quality gate.
#[derive(Clone, Debug)]
pub struct CommandSpec {
    /// Check name for reporting
    pub name: String,
    /// Program to execute
    pub program: String,
    /// Arguments
    pub args: Vec<String>,
}

impl CommandSpec {
    /// Validates the command spec against security restrictions and allowed programs.
    pub fn validate(&self, allowed_programs: &HashSet<String>) -> Result<(), String> {
        let prog = self.program.trim();
        if prog.is_empty() {
            return Err("Program name cannot be empty".to_string());
        }

        if prog.contains('/') || prog.contains('\\') || prog.contains("..") {
            return Err(format!(
                "Program name contains invalid path characters: '{}'",
                self.program
            ));
        }

        if prog.contains('\0')
            || prog.chars().any(|c| {
                c.is_whitespace() || c == ';' || c == '&' || c == '|' || c == '`' || c == '$'
            })
        {
            return Err(format!(
                "Program name contains invalid or metacharacters: '{}'",
                self.program
            ));
        }

        let lower_prog = prog.to_lowercase();
        let forbidden_shells = [
            "sh",
            "bash",
            "zsh",
            "ksh",
            "csh",
            "tcsh",
            "cmd",
            "cmd.exe",
            "powershell",
            "pwsh",
        ];
        if forbidden_shells.contains(&lower_prog.as_str()) {
            return Err(format!(
                "Shell executable is not permitted as quality gate program: '{}'",
                self.program
            ));
        }

        if !allowed_programs.contains(prog) {
            return Err(format!(
                "Program '{}' is not in the allowed commands list for quality gates",
                self.program
            ));
        }

        for (i, arg) in self.args.iter().enumerate() {
            if arg.contains('\0') {
                return Err(format!("Argument at index {} contains null byte", i));
            }
        }

        Ok(())
    }
}

/// Runs shell commands (injectable for tests).
#[async_trait]
pub trait CommandRunner: Send + Sync {
    /// Execute a command and return (exit_code, combined_output).
    async fn run(&self, spec: &CommandSpec) -> Result<(i32, String), String>;
}

/// Default command runner using tokio subprocess with binary validation.
#[derive(Clone, Debug)]
pub struct ProcessCommandRunner {
    allowed_programs: HashSet<String>,
}

impl ProcessCommandRunner {
    /// Create a new `ProcessCommandRunner` with default allowed programs.
    pub fn new() -> Self {
        let allowed = DEFAULT_ALLOWED_PROGRAMS
            .iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            allowed_programs: allowed,
        }
    }

    /// Create with custom allowed programs.
    #[allow(dead_code)]
    pub fn with_allowed_programs<I, S>(programs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed_programs: programs.into_iter().map(|s| s.into()).collect(),
        }
    }

    /// Add an allowed program name to the runner.
    #[allow(dead_code)]
    pub fn allow_program(mut self, program: impl Into<String>) -> Self {
        self.allowed_programs.insert(program.into());
        self
    }

    /// Validate command specification before execution.
    pub fn validate_spec(&self, spec: &CommandSpec) -> Result<(), String> {
        spec.validate(&self.allowed_programs)
    }
}

impl Default for ProcessCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandRunner for ProcessCommandRunner {
    async fn run(&self, spec: &CommandSpec) -> Result<(i32, String), String> {
        self.validate_spec(spec)?;

        let output = tokio::process::Command::new(&spec.program)
            .args(&spec.args)
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let code = output.status.code().unwrap_or(-1);
        let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
        if !output.stderr.is_empty() {
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        Ok((code, combined))
    }
}

/// Mock runner for unit tests.
#[derive(Clone, Default)]
pub struct MockCommandRunner {
    results: std::collections::HashMap<String, (i32, String)>,
}

impl MockCommandRunner {
    /// Create an empty mock.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a result for a check name.
    pub fn with_result(
        mut self,
        name: impl Into<String>,
        exit_code: i32,
        output: impl Into<String>,
    ) -> Self {
        self.results.insert(name.into(), (exit_code, output.into()));
        self
    }
}

#[async_trait]
impl CommandRunner for MockCommandRunner {
    async fn run(&self, spec: &CommandSpec) -> Result<(i32, String), String> {
        self.results
            .get(&spec.name)
            .cloned()
            .ok_or_else(|| format!("no mock result for {}", spec.name))
    }
}

/// Configuration for quality gate checks.
#[derive(Clone, Debug)]
pub struct QualityGateConfig {
    /// Commands to run (in order)
    pub checks: Vec<CommandSpec>,
    /// Whether failing checks block merge routing
    pub block_on_failure: bool,
}

impl QualityGateConfig {
    /// Standard Rust project gates.
    pub fn rust_defaults() -> Self {
        Self {
            checks: vec![
                CommandSpec {
                    name: "fmt".to_string(),
                    program: "cargo".to_string(),
                    args: vec!["fmt".to_string(), "--".to_string(), "--check".to_string()],
                },
                CommandSpec {
                    name: "clippy".to_string(),
                    program: "cargo".to_string(),
                    args: vec![
                        "clippy".to_string(),
                        "--all-targets".to_string(),
                        "--".to_string(),
                        "-D".to_string(),
                        "warnings".to_string(),
                    ],
                },
                CommandSpec {
                    name: "test".to_string(),
                    program: "cargo".to_string(),
                    args: vec!["test".to_string()],
                },
            ],
            block_on_failure: true,
        }
    }
}

/// Node that runs quality gates and routes based on pass/fail and risk.
pub struct QualityGateNode {
    id: String,
    config: QualityGateConfig,
    runner: Arc<dyn CommandRunner>,
    risk_classifier: RiskClassifier,
}

impl QualityGateNode {
    /// Create a quality gate node with the default process runner.
    pub fn new(id: impl Into<String>, config: QualityGateConfig) -> Self {
        Self::with_runner(id, config, Arc::new(ProcessCommandRunner::default()))
    }

    /// Create with a custom command runner (for tests).
    pub fn with_runner(
        id: impl Into<String>,
        config: QualityGateConfig,
        runner: Arc<dyn CommandRunner>,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            runner,
            risk_classifier: RiskClassifier::new(),
        }
    }

    async fn run_checks(&self) -> GateResult {
        let mut checks = Vec::new();
        let mut all_findings = Vec::new();

        for spec in &self.config.checks {
            let start = Instant::now();
            let (exit_code, output) = match self.runner.run(spec).await {
                Ok(r) => r,
                Err(e) => {
                    let finding = ReviewFinding::error(&spec.name, e);
                    all_findings.push(finding.clone());
                    checks.push(GateCheck {
                        name: spec.name.clone(),
                        passed: false,
                        exit_code: Some(-1),
                        duration_ms: start.elapsed().as_millis() as u64,
                        log_excerpt: finding.message.clone(),
                        findings: vec![finding],
                    });
                    continue;
                }
            };

            let passed = exit_code == 0;
            let mut findings = Vec::new();
            if !passed {
                findings.push(
                    ReviewFinding::error(
                        format!("gate.{}", spec.name),
                        format!("Check '{}' failed with exit code {}", spec.name, exit_code),
                    )
                    .at("Cargo.toml", 1),
                );
                all_findings.extend(findings.clone());
            }

            checks.push(GateCheck {
                name: spec.name.clone(),
                passed,
                exit_code: Some(exit_code),
                duration_ms: start.elapsed().as_millis() as u64,
                log_excerpt: truncate_log(&output, 2000),
                findings,
            });
        }

        let all_passed = checks.iter().all(|c| c.passed);
        if all_passed {
            GateResult::pass(checks)
        } else {
            GateResult::fail(checks, all_findings)
        }
    }
}

fn truncate_log(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

#[async_trait]
impl NodeExecutor for QualityGateNode {
    fn id(&self) -> &str {
        &self.id
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let gate_result = self.run_checks().await;

        let change_risk = {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard
                .get_context::<ChangeRisk>("change_risk")
                .unwrap_or_default()
        };

        let risk = self.risk_classifier.classify(&change_risk);
        let effective_pass = gate_result.passed || !self.config.block_on_failure;
        let route = RiskClassifier::approval_route(risk, effective_pass);
        let merge_blocker = self.risk_classifier.merge_blocker(effective_pass, risk);

        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("gate_result", gate_result.clone());
            guard.set_context("change_risk_level", risk);
            guard.set_context("merge_blocker", merge_blocker.clone());
            guard.set_context("gate_passed", gate_result.passed);
        }

        Ok(NodeOutput::transition(route))
    }

    fn description(&self) -> Option<&str> {
        Some("Runs lint/typecheck/test gates and routes on pass/fail/risk")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardrails::risk::RiskLevel;
    use crate::state::AgentState;
    use std::sync::RwLock;

    #[tokio::test]
    async fn test_quality_gate_pass_routes_passed() {
        let config = QualityGateConfig {
            checks: vec![CommandSpec {
                name: "mock_check".to_string(),
                program: "true".to_string(),
                args: vec![],
            }],
            block_on_failure: true,
        };
        let runner = Arc::new(MockCommandRunner::new().with_result("mock_check", 0, "ok"));
        let node = QualityGateNode::with_runner("gate", config, runner);

        let state = Arc::new(RwLock::new(AgentState::new()));
        let output = node.execute(state.clone()).await.unwrap();
        assert_eq!(output.target(), Some("passed"));

        let guard = state.read().unwrap();
        assert_eq!(guard.get_context::<bool>("gate_passed"), Some(true));
    }

    #[tokio::test]
    async fn test_quality_gate_fail_routes_gate_failed() {
        let config = QualityGateConfig {
            checks: vec![CommandSpec {
                name: "mock_check".to_string(),
                program: "false".to_string(),
                args: vec![],
            }],
            block_on_failure: true,
        };
        let runner = Arc::new(MockCommandRunner::new().with_result("mock_check", 1, "failed"));
        let node = QualityGateNode::with_runner("gate", config, runner);

        let state = Arc::new(RwLock::new(AgentState::new()));
        let output = node.execute(state).await.unwrap();
        assert_eq!(output.target(), Some("gate_failed"));
    }

    #[tokio::test]
    async fn test_high_risk_requires_approval_even_when_gates_pass() {
        let config = QualityGateConfig {
            checks: vec![CommandSpec {
                name: "mock_check".to_string(),
                program: "true".to_string(),
                args: vec![],
            }],
            block_on_failure: true,
        };
        let runner = Arc::new(MockCommandRunner::new().with_result("mock_check", 0, "ok"));
        let node = QualityGateNode::with_runner("gate", config, runner);

        let mut agent_state = AgentState::new();
        agent_state.set_context(
            "change_risk",
            ChangeRisk {
                destructive_git: true,
                ..Default::default()
            },
        );
        let state = Arc::new(RwLock::new(agent_state));
        let output = node.execute(state).await.unwrap();
        assert_eq!(output.target(), Some("needs_approval"));
    }

    #[tokio::test]
    async fn test_medium_risk_routes_review_when_gates_pass() {
        let config = QualityGateConfig {
            checks: vec![CommandSpec {
                name: "mock_check".to_string(),
                program: "true".to_string(),
                args: vec![],
            }],
            block_on_failure: true,
        };
        let runner = Arc::new(MockCommandRunner::new().with_result("mock_check", 0, "ok"));
        let node = QualityGateNode::with_runner("gate", config, runner);

        let mut agent_state = AgentState::new();
        agent_state.set_context(
            "change_risk",
            ChangeRisk {
                files_changed: 10,
                used_shell: true,
                ..Default::default()
            },
        );
        let state = Arc::new(RwLock::new(agent_state));
        let output = node.execute(state.clone()).await.unwrap();
        assert_eq!(output.target(), Some("review"));

        let guard = state.read().unwrap();
        assert_eq!(
            guard.get_context::<RiskLevel>("change_risk_level"),
            Some(RiskLevel::Medium)
        );
    }

    #[tokio::test]
    async fn test_non_blocking_failure_still_marks_merge_blocker() {
        let config = QualityGateConfig {
            checks: vec![CommandSpec {
                name: "mock_check".to_string(),
                program: "false".to_string(),
                args: vec![],
            }],
            block_on_failure: false,
        };
        let runner = Arc::new(MockCommandRunner::new().with_result("mock_check", 1, "failed"));
        let node = QualityGateNode::with_runner("gate", config, runner);

        let state = Arc::new(RwLock::new(AgentState::new()));
        let output = node.execute(state.clone()).await.unwrap();
        assert_eq!(output.target(), Some("passed"));

        let guard = state.read().unwrap();
        assert_eq!(guard.get_context::<bool>("gate_passed"), Some(false));
        let blocker = guard
            .get_context::<crate::guardrails::risk::MergeBlocker>("merge_blocker")
            .expect("merge_blocker");
        assert!(!blocker.blocked);
    }

    #[tokio::test]
    async fn test_process_command_runner_security_validation() {
        let runner = ProcessCommandRunner::default();

        // Allowed program succeeds validation
        let valid_spec = CommandSpec {
            name: "test_cargo".to_string(),
            program: "cargo".to_string(),
            args: vec!["check".to_string()],
        };
        assert!(runner.validate_spec(&valid_spec).is_ok());

        // Unallowed program fails
        let unallowed_spec = CommandSpec {
            name: "test_malicious".to_string(),
            program: "malicious_binary".to_string(),
            args: vec![],
        };
        assert!(runner.validate_spec(&unallowed_spec).is_err());

        // Path separator in program fails
        let path_sep_spec = CommandSpec {
            name: "test_path".to_string(),
            program: "/usr/bin/cargo".to_string(),
            args: vec![],
        };
        assert!(runner.validate_spec(&path_sep_spec).is_err());

        // Shell binary fails
        let shell_spec = CommandSpec {
            name: "test_sh".to_string(),
            program: "sh".to_string(),
            args: vec!["-c".to_string(), "whoami".to_string()],
        };
        assert!(runner.validate_spec(&shell_spec).is_err());

        // Shell metacharacters fail
        let meta_spec = CommandSpec {
            name: "test_meta".to_string(),
            program: "cargo;whoami".to_string(),
            args: vec![],
        };
        assert!(runner.validate_spec(&meta_spec).is_err());

        // Null byte in args fails
        let null_arg_spec = CommandSpec {
            name: "test_null".to_string(),
            program: "cargo".to_string(),
            args: vec!["check\0".to_string()],
        };
        assert!(runner.validate_spec(&null_arg_spec).is_err());
    }

    #[tokio::test]
    async fn test_process_command_runner_custom_allowed() {
        let runner = ProcessCommandRunner::with_allowed_programs(["custom_check"])
            .allow_program("another_check");
        let spec1 = CommandSpec {
            name: "test_custom1".to_string(),
            program: "custom_check".to_string(),
            args: vec![],
        };
        let spec2 = CommandSpec {
            name: "test_custom2".to_string(),
            program: "another_check".to_string(),
            args: vec![],
        };
        assert!(runner.validate_spec(&spec1).is_ok());
        assert!(runner.validate_spec(&spec2).is_ok());
    }
}
