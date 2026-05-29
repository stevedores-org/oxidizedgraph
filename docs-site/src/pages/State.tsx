import Layout from "@/components/Layout";
import CodeBlock from "@/components/CodeBlock";

export default function State() {
  return (
    <Layout>
      <h1 className="text-3xl font-extrabold tracking-tight mb-2">State</h1>
      <p className="text-lg text-zinc-400 mb-10">How state flows between nodes in the graph.</p>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">AgentState</h2>
      <p className="text-zinc-400 text-[15px] mb-4">
        The built-in <code className="text-violet-300/90 font-mono text-[13px]">AgentState</code> provides conversation messages, tool calls, and a flexible context map.
      </p>
      <CodeBlock>{`pub struct AgentState {
    pub messages: Vec<Message>,
    pub tool_calls: Vec<ToolCall>,
    pub context: HashMap<String, Value>,
    pub iteration: usize,
    pub is_complete: bool,
}

// Access in a node:
let mut guard = state.write().unwrap();
guard.set_context("key", "value");
let val = guard.get_context::<String>("key");`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">SharedState</h2>
      <p className="text-zinc-400 text-[15px] mb-4">
        State is passed via <code className="text-violet-300/90 font-mono text-[13px]">SharedState</code> (<code className="text-violet-300/90 font-mono text-[13px]">Arc&lt;RwLock&lt;AgentState&gt;&gt;</code>), enabling safe concurrent reads across parallel branches.
      </p>
    </Layout>
  );
}
