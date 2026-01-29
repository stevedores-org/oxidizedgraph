//! ReAct Agent Example
//!
//! This example demonstrates a ReAct (Reasoning + Acting) agent pattern
//! that alternates between calling an LLM and executing tools.
//!
//! Run with: `cargo run --example react_agent`

use oxidizedgraph::prelude::*;

// ============================================================================
// Mock LLM Provider
// ============================================================================

/// A mock LLM that simulates tool use patterns
struct MockLLM;

impl MockLLM {
    async fn generate(&self, messages: &[Message]) -> (String, Vec<ToolCall>) {
        // Look at the conversation to decide what to do
        let last = messages.last();

        match last {
            Some(msg) if msg.role == MessageRole::User => {
                // First turn: check what the user wants
                let text = &msg.content;
                if text.contains("weather") {
                    (
                        "I'll check the weather for you.".to_string(),
                        vec![ToolCall::new(
                            "call_001",
                            "get_weather",
                            serde_json::json!({"location": "San Francisco"}),
                        )],
                    )
                } else if text.contains("calculate") || text.contains("math") {
                    (
                        "Let me calculate that.".to_string(),
                        vec![ToolCall::new(
                            "call_002",
                            "calculator",
                            serde_json::json!({"expression": "42 * 17"}),
                        )],
                    )
                } else {
                    (
                        "I can help you with weather lookups and calculations. What would you like to know?".to_string(),
                        vec![],
                    )
                }
            }
            Some(msg) if msg.role == MessageRole::Tool => {
                // Tool result received, provide final answer
                (
                    format!(
                        "Based on the information I gathered: {}. Is there anything else you'd like to know?",
                        msg.content
                    ),
                    vec![],
                )
            }
            _ => (
                "How can I help you today?".to_string(),
                vec![],
            ),
        }
    }
}

// ============================================================================
// Tool Definitions
// ============================================================================

/// Weather lookup tool
struct WeatherTool;

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn description(&self) -> &str {
        "Get the current weather for a location"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city to get weather for"
                }
            },
            "required": ["location"]
        })
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, NodeError> {
        let location = arguments["location"]
            .as_str()
            .unwrap_or("Unknown");
        Ok(format!("Weather in {}: Sunny, 72°F", location))
    }
}

/// Calculator tool
struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Perform mathematical calculations"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "The math expression to evaluate"
                }
            },
            "required": ["expression"]
        })
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, NodeError> {
        let expr = arguments["expression"]
            .as_str()
            .unwrap_or("0");
        // Simple mock calculation
        if expr.contains("42 * 17") {
            Ok("714".to_string())
        } else {
            Ok("Result: computed".to_string())
        }
    }
}

// ============================================================================
// Node Definitions
// ============================================================================

/// Node that calls the LLM
struct CallModelNode {
    llm: MockLLM,
}

#[async_trait]
impl NodeExecutor for CallModelNode {
    fn id(&self) -> &str {
        "call_model"
    }

    async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
        let messages = {
            let guard = state
                .read()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.messages.clone()
        };

        println!("Calling LLM...");
        let (response, tool_calls) = self.llm.generate(&messages).await;

        {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;

            // Add assistant message
            guard.add_assistant_message(&response);

            // Store tool calls
            guard.tool_calls = tool_calls.clone();

            // Mark complete if no tool calls
            if tool_calls.is_empty() {
                guard.mark_complete();
            }

            println!("  Response: {}", response);
            if !tool_calls.is_empty() {
                println!("  Tool calls: {}", tool_calls.len());
            }
        }

        Ok(NodeOutput::cont())
    }

    fn description(&self) -> Option<&str> {
        Some("Calls the LLM to get next action")
    }
}

/// Router that decides whether to execute tools or finish
struct AgentRouter;

impl AgentRouter {
    fn route(state: &AgentState) -> String {
        if state.is_complete {
            println!("Routing to END");
            transitions::END.to_string()
        } else if state.has_pending_tool_calls() {
            println!("Routing to execute_tools");
            "execute_tools".to_string()
        } else {
            println!("Routing to END (no tool calls)");
            transitions::END.to_string()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("ReAct Agent Example");
    println!("===================\n");

    // Create tool registry
    let tools = ToolRegistry::new()
        .register(WeatherTool)
        .register(CalculatorTool);

    // Build the graph
    let graph = GraphBuilder::new()
        .name("ReAct Agent")
        .description("A ReAct agent that can use tools")
        .add_node(CallModelNode { llm: MockLLM })
        .add_node(ToolNode::new("execute_tools", tools))
        .set_entry_point("call_model")
        .add_conditional_edge("call_model", AgentRouter::route)
        .add_edge("execute_tools", "call_model")
        .compile()?;

    // Print graph structure
    println!("Graph Structure (Mermaid):");
    println!("```mermaid");
    println!("{}", graph.to_mermaid());
    println!("```\n");

    // Create runner
    let runner = GraphRunner::new(
        graph,
        RunnerConfig::default()
            .max_iterations(10)
            .tag("react")
            .tag("example"),
    );

    // Create initial state with user message
    let mut initial_state = AgentState::new();
    initial_state.messages.push(Message::system(
        "You are a helpful assistant with access to weather and calculator tools.",
    ));
    initial_state.messages.push(Message::user(
        "What's the weather like in San Francisco?",
    ));

    println!("User: What's the weather like in San Francisco?\n");
    println!("---\n");

    // Execute
    let final_state = runner.invoke(initial_state).await?;

    println!("\n---\n");
    println!("Agent Complete!");
    println!("---------------");
    println!("Total iterations: {}", final_state.iteration);
    println!("Total messages: {}", final_state.messages.len());
    println!("Is complete: {}", final_state.is_complete);

    // Print final response
    if let Some(last) = final_state.last_assistant_message() {
        println!("\nFinal Response:");
        println!("  {}", last.content);
    }

    Ok(())
}
