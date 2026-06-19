import { Link } from "react-router-dom";
import Layout from "@/components/Layout";
import Callout from "@/components/Callout";

export default function Home() {
  return (
    <Layout>
      <div className="pb-10 mb-10 border-b border-zinc-800/60">
        <p className="text-[11px] uppercase tracking-widest text-amber-500/90 font-semibold mb-2">Commercial Enterprise Edition</p>
        <h1 className="text-4xl sm:text-5xl font-extrabold tracking-tight bg-gradient-to-br from-zinc-100 to-zinc-400 bg-clip-text text-transparent leading-tight">oxidizedgraph</h1>
        <p className="text-lg text-zinc-400 mt-3 leading-relaxed max-w-xl">
          LangGraph-style agent orchestration in Rust — with enterprise tenancy, RBAC, audit trails, and SLO guardrails for production deployments.
        </p>
        <div className="flex flex-wrap gap-3 mt-6">
          <Link to="/getting-started" className="bg-violet-500 hover:bg-violet-600 text-white font-semibold px-5 py-2.5 rounded-lg transition text-sm">Get Started</Link>
          <Link to="/enterprise" className="border border-amber-700/50 hover:border-amber-500/70 px-5 py-2.5 rounded-lg transition text-sm text-amber-200/90">Enterprise</Link>
        </div>
      </div>

      <h2 className="text-2xl font-bold tracking-tight mb-3">Why oxidizedgraph?</h2>
      <div className="border border-zinc-800 rounded-xl overflow-hidden my-4 text-[13px]">
        <table className="w-full">
          <thead><tr className="border-b border-zinc-700 text-zinc-500"><th className="px-5 py-3 text-left font-semibold">Feature</th><th className="px-5 py-3 text-left font-semibold">LangGraph (Python)</th><th className="px-5 py-3 text-left font-semibold">oxidizedgraph</th></tr></thead>
          <tbody className="text-zinc-400">
            <tr className="border-b border-zinc-800/60"><td className="px-5 py-3">Parallelism</td><td className="px-5 py-3">Limited by GIL</td><td className="px-5 py-3 text-violet-400">True multi-core</td></tr>
            <tr className="border-b border-zinc-800/60"><td className="px-5 py-3">Memory / session</td><td className="px-5 py-3">~50MB</td><td className="px-5 py-3 text-violet-400">~5MB</td></tr>
            <tr className="border-b border-zinc-800/60"><td className="px-5 py-3">Startup time</td><td className="px-5 py-3">~200ms</td><td className="px-5 py-3 text-violet-400">~10ms</td></tr>
            <tr className="border-b border-zinc-800/60"><td className="px-5 py-3">Type safety</td><td className="px-5 py-3">Runtime</td><td className="px-5 py-3 text-violet-400">Compile-time</td></tr>
            <tr><td className="px-5 py-3">Binary size</td><td className="px-5 py-3">Needs Python</td><td className="px-5 py-3 text-violet-400">~15MB standalone</td></tr>
          </tbody>
        </table>
      </div>

      <Callout icon="🕸️">
        <strong className="text-zinc-100">Graph-native.</strong> Define nodes, connect them with edges (direct or conditional), and let the runner execute. State flows through the graph via a shared <code className="text-violet-300/90 font-mono text-[13px]">AgentState</code>.
      </Callout>
    </Layout>
  );
}
