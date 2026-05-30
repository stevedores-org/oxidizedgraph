import { Link } from "react-router-dom";
import { ReactNode } from "react";

export function CardGrid({ children }: { children: ReactNode }) {
  return (
    <div className="grid sm:grid-cols-2 gap-3 my-6">{children}</div>
  );
}

export function Card({
  to,
  title,
  description,
  tag,
}: {
  to: string;
  title: string;
  description: string;
  tag?: string;
}) {
  return (
    <Link
      to={to}
      className="group bg-zinc-900 border border-zinc-800 rounded-xl p-5 transition-all hover:border-orange-500/40 hover:bg-zinc-800/50 hover:-translate-y-0.5"
    >
      <div className="font-mono text-sm font-semibold text-orange-400 mb-2 group-hover:text-orange-300">
        {title}
      </div>
      <div className="text-[13px] text-zinc-400 leading-relaxed">
        {description}
      </div>
      {tag && (
        <span className="inline-block mt-3 text-[10px] font-semibold text-zinc-500 bg-zinc-950 border border-zinc-800 rounded px-2 py-0.5 uppercase tracking-wider">
          {tag}
        </span>
      )}
    </Link>
  );
}
