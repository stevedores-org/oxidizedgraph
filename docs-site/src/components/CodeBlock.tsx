export default function CodeBlock({
  children,
  title,
}: {
  children: string;
  title?: string;
}) {
  return (
    <div className="my-4">
      {title && (
        <div className="text-[11px] font-mono font-medium text-zinc-500 bg-zinc-900 border border-zinc-800 border-b-0 rounded-t-lg px-4 py-2">
          {title}
        </div>
      )}
      <pre
        className={`bg-zinc-900 border border-zinc-800 ${
          title ? "rounded-b-lg" : "rounded-lg"
        } px-5 py-4 overflow-x-auto text-[13px] leading-relaxed`}
      >
        <code className="text-zinc-300 font-mono">{children}</code>
      </pre>
    </div>
  );
}

export function InlineCode({ children }: { children: string }) {
  return (
    <code className="font-mono text-[0.85em] bg-zinc-800/80 border border-zinc-700/40 rounded px-1.5 py-0.5 text-orange-300/90">
      {children}
    </code>
  );
}
