import Layout from "@/components/Layout";
import Callout from "@/components/Callout";
import CodeBlock from "@/components/CodeBlock";

export default function Enterprise() {
  return (
    <Layout>
      <div className="pb-10 mb-10 border-b border-zinc-800/60">
        <h1 className="text-4xl font-extrabold tracking-tight text-zinc-100">Enterprise Edition</h1>
        <p className="text-lg text-zinc-400 mt-3 max-w-2xl leading-relaxed">
          Commercial modules shipped only in <code className="text-violet-300/90 font-mono text-sm">lornu-ai/oxidizedgraph</code>.
          Multi-tenant RBAC, scoped secrets, immutable audit, and SLO/cost guardrails as first-class graph nodes.
        </p>
      </div>

      <Callout icon="🔐">
        <strong className="text-zinc-100">SSO required.</strong> This site is for licensed customers and Lornu staff.
        Sign in via Cloudflare Access at{" "}
        <a href="https://docs.oxidizedgraph.lornu.ai" className="text-violet-400 hover:underline">
          docs.oxidizedgraph.lornu.ai
        </a>
        . Open-source docs remain on{" "}
        <a href="https://docs.stevedores.org/oxidizedgraph/" className="text-violet-400 hover:underline">
          docs.stevedores.org
        </a>
        .
      </Callout>

      <h2 className="text-2xl font-bold mt-10 mb-4">Modules</h2>
      <div className="border border-zinc-800 rounded-xl overflow-hidden text-[13px] mb-8">
        <table className="w-full">
          <thead>
            <tr className="border-b border-zinc-700 text-zinc-500">
              <th className="px-5 py-3 text-left font-semibold">Module</th>
              <th className="px-5 py-3 text-left font-semibold">Purpose</th>
            </tr>
          </thead>
          <tbody className="text-zinc-400">
            <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 font-mono text-violet-300/90">tenant</td><td className="px-5 py-3">Tenant boundaries, RBAC subjects, roles, permissions</td></tr>
            <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 font-mono text-violet-300/90">secrets</td><td className="px-5 py-3">Scoped credentials, secret handles, log redaction</td></tr>
            <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 font-mono text-violet-300/90">audit</td><td className="px-5 py-3">Hash-chained audit log, compliance export</td></tr>
            <tr className="border-b border-zinc-800/60"><td className="px-5 py-3 font-mono text-violet-300/90">slo</td><td className="px-5 py-3">SLO tracking, cost budgets, spend guardrails</td></tr>
            <tr><td className="px-5 py-3 font-mono text-violet-300/90">node</td><td className="px-5 py-3">TenantGuard, BudgetGuard, AuditExport, SloRecord nodes</td></tr>
          </tbody>
        </table>
      </div>

      <h2 className="text-2xl font-bold mb-3">Quick example</h2>
      <CodeBlock>{`cargo run --example enterprise_workflow`}</CodeBlock>

      <p className="text-zinc-500 text-sm mt-8">
        Licensing: <a href="mailto:licensing@lornu.ai" className="text-violet-400 hover:underline">licensing@lornu.ai</a>
      </p>
    </Layout>
  );
}
