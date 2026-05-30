import Layout from "@/components/Layout";
import CodeBlock from "@/components/CodeBlock";

export default function GettingStarted() {
  return (
    <Layout>
      <h1 className="text-3xl font-extrabold tracking-tight mb-2">Getting Started</h1>
      <p className="text-lg text-zinc-400 mb-10">Add oxidizedgraph to your project and build your first agent graph.</p>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Installation</h2>
      <CodeBlock>{`[dependencies]
oxidizedgraph = "0.1"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Minimal Example</h2>
      <CodeBlock>{`use oxidizedgraph::prelude::*;

struct ProcessNode;

#[async_trait]
impl NodeExecutor for ProcessNode {
    fn id(&self) -> &str { "process" }

    async fn execute(
        &self,
        state: SharedState,
    ) -> Result<NodeOutput, NodeError> {
        let mut guard = state.write().unwrap();
        guard.set_context("processed", true);
        Ok(NodeOutput::cont())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let graph = GraphBuilder::new()
        .add_node(ProcessNode)
        .set_entry_point("process")
        .add_edge_to_end("process")
        .compile()?;

    let runner = GraphRunner::with_defaults(graph);
    let result = runner.invoke(AgentState::new()).await?;

    println!(
        "Processed: {:?}",
        result.get_context::<bool>("processed"),
    );
    Ok(())
}`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Run Examples</h2>
      <CodeBlock>{`# Simple linear workflow
cargo run --example simple_workflow

# ReAct agent pattern
cargo run --example react_agent`}</CodeBlock>
    </Layout>
  );
}
