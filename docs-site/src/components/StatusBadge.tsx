const variants = {
  done: "bg-green-500/10 text-green-400 border-green-500/20",
  wip: "bg-orange-500/10 text-orange-400 border-orange-500/20",
  planned: "bg-zinc-800 text-zinc-500 border-zinc-700",
} as const;

export default function StatusBadge({
  status,
}: {
  status: "done" | "wip" | "planned";
}) {
  const label = status === "done" ? "Done" : status === "wip" ? "In Progress" : "Planned";
  return (
    <span
      className={`inline-block text-[10px] font-semibold border rounded px-2 py-0.5 uppercase tracking-wider ${variants[status]}`}
    >
      {label}
    </span>
  );
}
