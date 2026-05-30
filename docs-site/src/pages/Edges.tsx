import Layout from "@/components/Layout";
import CodeBlock from "@/components/CodeBlock";
import Callout from "@/components/Callout";

export default function Edges() {
  return (
    <Layout>
      <h1 className="text-3xl font-extrabold tracking-tight mb-2">Edges & Routing</h1>
      <p className="text-lg text-zinc-400 mb-10">Direct and conditional edges for graph flow control.</p>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Direct Edges</h2>
      <CodeBlock>{`GraphBuilder::new()
    .add_edge("node_a", "node_b")  // a -> b
    .add_edge_to_end("node_b")     // b -> END`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Conditional Edges</h2>
      <p className="text-zinc-400 text-[15px] mb-4">
        Route dynamically based on state. Return the target node ID or <code className="text-violet-300/90 font-mono text-[13px]">transitions::END</code>.
      </p>
      <CodeBlock>{`.add_conditional_edge("agent", |state| {
    if state.is_complete {
        transitions::END.to_string()
    } else if state.tool_calls.is_empty() {
        "synthesize".to_string()
    } else {
        "tool_executor".to_string()
    }
})`}</CodeBlock>

      <Callout icon="🔄">
        Conditional edges + <code className="text-violet-300/90 font-mono text-[13px]">max_iterations</code> on the runner enable safe loops (ReAct, chain-of-thought) without infinite execution.
      </Callout>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">ContextRouterNode</h2>
      <p className="text-zinc-400 text-[15px] mb-4">Built-in node that routes based on a context key value:</p>
      <CodeBlock>{`let router = ContextRouterNode::new(
    "router",
    "intent",
    vec![
        ("search", "search_node"),
        ("chat", "chat_node"),
    ],
    "fallback_node",
);`}</CodeBlock>
    </Layout>
  );
}
