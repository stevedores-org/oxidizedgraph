import Layout from "@/components/Layout";
import CodeBlock from "@/components/CodeBlock";

export default function Nodes() {
  return (
    <Layout>
      <h1 className="text-3xl font-extrabold tracking-tight mb-2">Nodes</h1>
      <p className="text-lg text-zinc-400 mb-10">Custom and built-in node implementations.</p>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">NodeExecutor Trait</h2>
      <CodeBlock>{`#[async_trait]
pub trait NodeExecutor: Send + Sync {
    fn id(&self) -> &str;

    async fn execute(
        &self,
        state: SharedState,
    ) -> Result<NodeOutput, NodeError>;
}`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">NodeOutput</h2>
      <p className="text-zinc-400 text-[15px] mb-4">Return values control graph flow:</p>
      <div className="border border-zinc-800 rounded-xl overflow-hidden text-[13px] mb-6">
        <table className="w-full"><tbody className="text-zinc-400">
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">NodeOutput::cont()</td><td className="px-5 py-3">Continue to next node via edges</td></tr>
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">NodeOutput::finish()</td><td className="px-5 py-3">End graph execution</td></tr>
          <tr><td className="px-5 py-3 text-violet-400 font-mono font-medium">NodeOutput::continue_to("x")</td><td className="px-5 py-3">Route to a specific node</td></tr>
        </tbody></table>
      </div>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">FunctionNode</h2>
      <p className="text-zinc-400 text-[15px] mb-4">Create nodes from closures for quick prototyping:</p>
      <CodeBlock>{`let node = FunctionNode::new("greet", |state| {
    Box::pin(async move {
        let mut guard = state.write().unwrap();
        guard.set_context("greeting", "Hello!");
        Ok(NodeOutput::cont())
    })
});`}</CodeBlock>
    </Layout>
  );
}
