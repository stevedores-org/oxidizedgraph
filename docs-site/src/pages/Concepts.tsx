import Layout from "@/components/Layout";
import CodeBlock from "@/components/CodeBlock";
import Callout from "@/components/Callout";

export default function Concepts() {
  return (
    <Layout>
      <h1 className="text-3xl font-extrabold tracking-tight mb-2">Core Concepts</h1>
      <p className="text-lg text-zinc-400 mb-10">State, Nodes, Edges, and the Runner — the four primitives.</p>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">State</h2>
      <p className="text-zinc-400 text-[15px] mb-4">
        State flows through the graph between nodes. <code className="text-violet-300/90 font-mono text-[13px]">AgentState</code> is the built-in state type.
      </p>
      <CodeBlock>{`pub struct AgentState {
    pub messages: Vec<Message>,
    pub tool_calls: Vec<ToolCall>,
    pub context: HashMap<String, Value>,
    pub iteration: usize,
    pub is_complete: bool,
}`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Nodes</h2>
      <p className="text-zinc-400 text-[15px] mb-4">
        Nodes implement <code className="text-violet-300/90 font-mono text-[13px]">NodeExecutor</code> and transform state.
      </p>
      <CodeBlock>{`#[async_trait]
impl NodeExecutor for MyNode {
    fn id(&self) -> &str { "my_node" }

    async fn execute(
        &self,
        state: SharedState,
    ) -> Result<NodeOutput, NodeError> {
        // Transform state...
        Ok(NodeOutput::cont())       // Continue
        // Ok(NodeOutput::finish())  // End
        // Ok(NodeOutput::continue_to("x"))  // Route
    }
}`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Built-in Nodes</h2>
      <div className="border border-zinc-800 rounded-xl overflow-hidden text-[13px] mb-6">
        <table className="w-full"><tbody className="text-zinc-400">
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">EchoNode</td><td className="px-5 py-3">Stores a message in context</td></tr>
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">DelayNode</td><td className="px-5 py-3">Adds a configurable delay</td></tr>
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">FunctionNode</td><td className="px-5 py-3">Create nodes from closures</td></tr>
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">LLMNode</td><td className="px-5 py-3">Call LLM providers</td></tr>
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">ToolNode</td><td className="px-5 py-3">Execute pending tool calls</td></tr>
          <tr><td className="px-5 py-3 text-violet-400 font-mono font-medium">ConditionalNode</td><td className="px-5 py-3">Routes based on a predicate</td></tr>
        </tbody></table>
      </div>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Edges</h2>
      <CodeBlock>{`GraphBuilder::new()
    .add_edge("node_a", "node_b")         // Direct
    .add_edge_to_end("node_b")            // To END
    .add_conditional_edge("agent", |s| {  // Conditional
        if s.is_complete {
            transitions::END.to_string()
        } else {
            "continue".to_string()
        }
    })`}</CodeBlock>

      <Callout icon="🎯">
        Conditional edges enable patterns like ReAct loops, where the agent decides whether to call a tool or finish.
      </Callout>
    </Layout>
  );
}
