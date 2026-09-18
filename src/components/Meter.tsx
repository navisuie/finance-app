/**
 * A single ratio against a limit: one bar, one track. Over-limit turns red,
 * and the state is always written out too, so colour is never the only clue.
 */
export function Meter({ used, over, label }: { used: number; over?: boolean; label?: string }) {
  const pct = Math.max(0, Math.min(1, used));
  return (
    <div
      className="h-2 w-full overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-700"
      role="img"
      aria-label={label ?? `${Math.round(used * 100)}% used`}
    >
      <div
        className="h-full rounded-full transition-[width]"
        style={{ width: `${pct * 100}%`, background: over ? "var(--viz-over)" : "var(--viz-line)" }}
      />
    </div>
  );
}
