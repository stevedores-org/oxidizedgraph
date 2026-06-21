import { Link, useLocation } from "react-router-dom";

interface NavItem { label: string; href: string; external?: boolean; }
interface NavSection { title: string; items: NavItem[]; }

const OSS_DOCS = "https://docs.stevedores.org/oxidizedgraph";
const navigation: NavSection[] = [
  { title: "Overview", items: [
    { label: "Introduction", href: "/" },
    { label: "Getting Started", href: "/getting-started" },
    { label: "Core Concepts", href: "/concepts" },
    { label: "Enterprise Edition", href: "/enterprise" },
  ]},
  { title: "API", items: [
    { label: "State", href: "/api/state" },
    { label: "Nodes", href: "/api/nodes" },
    { label: "Edges & Routing", href: "/api/edges" },
    { label: "Runner", href: "/api/runner" },
  ]},
  { title: "Open Source", items: [
    { label: "Community docs", href: OSS_DOCS, external: true },
    { label: "GitHub (OSS)", href: "https://github.com/stevedores-org/oxidizedgraph", external: true },
    { label: "crates.io", href: "https://crates.io/crates/oxidizedgraph", external: true },
  ]},
];

export default function Sidebar() {
  const { pathname } = useLocation();
  return (
    <nav className="w-64 shrink-0 bg-zinc-900/60 border-r border-zinc-800 fixed top-0 left-0 bottom-0 overflow-y-auto hidden lg:flex flex-col">
      <div className="px-5 pt-6 pb-4 border-b border-zinc-800/60">
        <Link to="/" className="flex items-center gap-2.5">
          <span className="text-violet-500 font-mono font-bold text-lg">🕸️</span>
          <span className="font-bold text-lg tracking-tight text-zinc-100">oxidizedgraph</span>
        </Link>
        <p className="text-[10px] uppercase tracking-widest text-amber-500/90 mt-2 font-semibold">
          Commercial · SSO
        </p>
      </div>
      <div className="flex-1 py-3">
        {navigation.map((s) => (
          <div key={s.title} className="mb-1">
            <div className="px-5 py-2 text-[11px] font-semibold text-zinc-500 uppercase tracking-widest">{s.title}</div>
            {s.items.map((item) => {
              const active = !item.external && pathname === item.href;
              const cls = `flex items-center gap-2 px-5 py-[7px] text-[13px] border-l-2 transition-all ${active ? "border-violet-500 text-violet-400 bg-violet-500/[0.06] font-medium" : "border-transparent text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/40"}`;
              return item.external ? (
                <a key={item.href} href={item.href} className={cls}>{item.label}<span className="ml-auto text-[10px] text-zinc-600">&nearr;</span></a>
              ) : (
                <Link key={item.href} to={item.href} className={cls}>{item.label}</Link>
              );
            })}
          </div>
        ))}
      </div>
      <div className="px-5 py-4 border-t border-zinc-800/60 text-xs text-zinc-500 flex flex-col gap-1">
        <span className="text-zinc-600">Licensed customers &amp; Lornu staff</span>
        <a href="mailto:licensing@lornu.ai" className="hover:text-violet-400 transition">licensing@lornu.ai</a>
      </div>
    </nav>
  );
}
