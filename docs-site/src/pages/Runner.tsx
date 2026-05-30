import Layout from "@/components/Layout";
import CodeBlock from "@/components/CodeBlock";

export default function Runner() {
  return (
    <Layout>
      <h1 className="text-3xl font-extrabold tracking-tight mb-2">Runner</h1>
      <p className="text-lg text-zinc-400 mb-10">Execute compiled graphs with configurable options.</p>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Basic Usage</h2>
      <CodeBlock>{`let runner = GraphRunner::with_defaults(graph);
let result = runner.invoke(AgentState::new()).await?;`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">Configuration</h2>
      <CodeBlock>{`let runner = GraphRunner::new(
    graph,
    RunnerConfig::default()
        .max_iterations(100)
        .verbose(true)
        .tag("my-workflow"),
);`}</CodeBlock>

      <h2 className="text-xl font-bold tracking-tight mt-10 mb-3 pb-2 border-b border-zinc-800/60">RunnerConfig Options</h2>
      <div className="border border-zinc-800 rounded-xl overflow-hidden text-[13px]">
        <table className="w-full"><tbody className="text-zinc-400">
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">max_iterations</td><td className="px-5 py-3">Safety limit for loop-based graphs (default: 25)</td></tr>
          <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 text-violet-400 font-mono font-medium">verbose</td><td className="px-5 py-3">Enable tracing output for each node execution</td></tr>
          <tr><td className="px-5 py-3 text-violet-400 font-mono font-medium">tag</td><td className="px-5 py-3">Label for tracing and logging</td></tr>
        </tbody></table>
      </div>
    </Layout>
  );
}
