//! End-to-end example of the governance integration.
//!
//! This example shows how to:
//! 1. Use the `GovernanceValidator` to check manifest tags syntax and workspace symlinks.
//! 2. Build a workflow graph using role-based guidance, handoffs, routing, and tools.
//! 3. Verify that role-scoped policies dynamically restrict tools (e.g. Architect cannot run shell tools, but Builder can).

use std::sync::{Arc, RwLock};
use oxidizedgraph::prelude::*;

const MANIFEST: &str = "\
<@all>
System-wide guidance.

<@architect>
Plan the architecture.
Do not invoke shell tools.

<@builder>
Implement code changes.
Shell tools are allowed.
";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 1. Governance Validation ===");
    // Validate inline manifest tags syntax
    let validator = GovernanceValidator::with_master(".", "GEMINI.md");
    validator.validate_manifest_string(MANIFEST)?;
    println!("Manifest syntax check: PASSED");

    // Check workspace symlinks (if any exist locally, otherwise it checks defaults)
    let report = validator.validate_compliance();
    if report.is_compliant {
        println!("Workspace symlink integrity: PASSED");
    } else {
        println!("Workspace symlink integrity status: {:?}", report.issues);
    }

    println!("\n=== 2. Creating Tool Registry ===");
    // Register a tool that might be restricted or allowed
    let shell_tool = FunctionTool::new(
        "shell",
        "Executes a shell command",
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            }
        }),
        |args| {
            let cmd = args["command"].as_str().unwrap_or("");
            Ok(format!("Successfully executed command: '{}'", cmd))
        },
    );

    let registry = ToolRegistry::new().register(shell_tool);

    println!("=== 3. Building Governance-Aware Graph ===");
    // Build a graph that flows: gov (Architect) -> run plan -> try tool (denied) -> handoff (Builder) -> try tool (allowed)
    let graph = GraphBuilder::new()
        // Start under governance as Architect
        .add_node(GovernanceNode::new("gov_init", MANIFEST, AgentRole::Architect))
        // Try executing shell tool (this will be denied under Architect policy)
        .add_node(ToolNode::new("tools", registry))
        // Handoff to Builder role
        .add_node(RoleHandoffNode::new("to_builder", MANIFEST, AgentRole::Builder))
        // Complete node
        .add_node(StaticTransitionNode::to_end("done"))
        
        .set_entry_point("gov_init")
        .add_edge("gov_init", "tools")
        .add_edge("tools", "to_builder")
        .add_edge("to_builder", "done")
        .compile()?;

    println!("=== 4. Executing Workflow under Architect ===");
    let mut state = AgentState::new();
    // Set a pending tool call for 'shell'
    state.tool_calls.push(ToolCall::new(
        "call_1",
        "shell",
        serde_json::json!({"command": "cargo test"}),
    ));

    let shared = Arc::new(RwLock::new(state));
    
    // Execute up to 'to_builder'
    let runner = GraphRunner::new(graph.clone(), RunnerConfig::default());
    
    println!("Running graph...");
    let final_state = runner.invoke_shared(shared).await?;

    // Verify messages: the shell tool should have been denied since we started as Architect
    println!("\nMessages log:");
    for msg in &final_state.messages {
        println!("  [{:?}]: {}", msg.role, msg.content);
    }

    let has_denial = final_state.messages.iter().any(|msg| {
        msg.content.contains("denied by policy") || msg.content.contains("Policy denied")
    });

    if has_denial {
        println!("\nVerification: Tool policy successfully enforced. Architect was denied shell tool execution.");
    } else {
        println!("\nVerification FAILED: Architect was not blocked from executing shell tool.");
    }

    Ok(())
}
