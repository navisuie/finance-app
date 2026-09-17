import { estimateStrength } from "../lib/password";

const COLORS = ["bg-zinc-300", "bg-red-500", "bg-amber-500", "bg-brand-500", "bg-brand-700"];

export function StrengthMeter({ password }: { password: string }) {
  const { score, label } = estimateStrength(password);
  if (!password) return null;
  return (
    <div className="space-y-1" aria-live="polite">
      <div className="flex gap-1">
        {[1, 2, 3, 4].map((i) => (
          <div
            key={i}
            className={`h-1.5 flex-1 rounded-full ${i <= score ? COLORS[score] : "bg-zinc-200 dark:bg-zinc-800"}`}
          />
        ))}
      </div>
      <p className="text-xs text-zinc-500">{label}</p>
    </div>
  );
}
