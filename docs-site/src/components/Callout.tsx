import { ReactNode } from "react";

export default function Callout({
  icon = "⚡",
  children,
}: {
  icon?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex gap-3 bg-orange-500/[0.06] border border-orange-500/20 rounded-lg px-5 py-4 my-6 text-[13px] text-zinc-300 leading-relaxed">
      <span className="text-base shrink-0">{icon}</span>
      <div>{children}</div>
    </div>
  );
}
