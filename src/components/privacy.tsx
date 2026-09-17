import { createContext, useContext } from "react";
import { formatCents } from "../lib/money";

export const PrivacyContext = createContext(false);

/** Displays an amount, masked when "Hide balances" is on (the real value is
 * not rendered at all, so it can't be read from a screenshot or zoom). */
export function Money({
  cents,
  signed,
  colored,
  className = "",
}: {
  cents: number | null | undefined;
  signed?: boolean;
  colored?: boolean;
  className?: string;
}) {
  const hidden = useContext(PrivacyContext);
  if (cents === null || cents === undefined) return <span className={`text-zinc-400 ${className}`}>—</span>;
  const color = colored
    ? cents < 0
      ? "text-red-600 dark:text-red-400"
      : cents > 0
        ? "text-brand-700 dark:text-brand-500"
        : ""
    : "";
  return (
    <span className={`tabular-nums ${hidden ? "" : color} ${className}`} aria-label={hidden ? "Hidden amount" : undefined}>
      {hidden ? "$•••••" : formatCents(cents, { signed })}
    </span>
  );
}
