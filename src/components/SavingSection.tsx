import { useEffect, useState } from "react";
import { useAsync } from "../hooks/useAsync";
import { KIND_LABELS } from "../lib/accountKinds";
import { api, errorMessage } from "../lib/api";
import { centsToInput, parseMoney } from "../lib/money";
import type { SavingMonth } from "../lib/types";
import { Meter } from "./Meter";
import { Money } from "./privacy";
import { controlClass } from "./Select";
import { Button, Card } from "./ui";

/**
 * Money moved into savings and investment accounts this month, against a
 * standing monthly target. The opposite of a spending budget: going over
 * the target is the good outcome, so the bar never turns red.
 */
export function SavingSection({
  month,
  monthLabel,
  isCurrentMonth,
  onOpenTransactions,
}: {
  month: string;
  monthLabel: string;
  isCurrentMonth: boolean;
  onOpenTransactions?: () => void;
}) {
  const loaded = useAsync(() => api.savingMonth(month), [month]);
  const [data, setData] = useState<SavingMonth>();
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState("");
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (loaded.data) setData(loaded.data);
  }, [loaded.data]);

  const d = data;
  if (!d) return loaded.error ? <p className="text-sm text-red-600">{loaded.error}</p> : null;

  const startEdit = () => {
    setValue(d.targetCents !== null ? centsToInput(d.targetCents) : "");
    setEditing(true);
  };
  const commit = async () => {
    setEditing(false);
    const cents = value.trim() === "" ? 0 : parseMoney(value);
    if (cents === null || cents === (d.targetCents ?? 0)) return;
    try {
      setData(await api.setSavingTarget(month, cents));
      setError(undefined);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const target = d.targetCents;
  const short = target !== null ? target - d.totalCents : 0;
  const shareOfIncome = d.expectedIncomeCents > 0 ? Math.round((d.totalCents / d.expectedIncomeCents) * 100) : null;

  return (
    <Card title="Saving & investing">
      <div className="space-y-3">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <p className="text-2xl font-semibold">
              <Money cents={d.totalCents} />{" "}
              {target !== null ? (
                <span className="text-base font-normal text-zinc-500">
                  of <Money cents={target} /> target
                </span>
              ) : null}
            </p>
            <p className="text-xs text-zinc-500">
              Investing <Money cents={d.investingCents} /> · cash savings <Money cents={d.cashSavingsCents} />
              {shareOfIncome !== null ? ` · ${shareOfIncome}% of income` : ""}
            </p>
          </div>
          {editing ? (
            <input
              className={`${controlClass} w-32 py-1 text-right`}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onBlur={commit}
              onKeyDown={(e) => {
                if (e.key === "Enter") commit();
                if (e.key === "Escape") setEditing(false);
              }}
              inputMode="decimal"
              placeholder="0.00"
              aria-label="Monthly saving and investing target"
              autoFocus
            />
          ) : (
            <Button variant="secondary" className="py-1" onClick={startEdit}>
              {target !== null ? "Change target" : "Set a monthly target"}
            </Button>
          )}
        </div>

        {target !== null ? (
          <div className="space-y-1">
            <Meter used={target > 0 ? d.totalCents / target : 0} label="Saving and investing target" />
            <p className={`text-xs ${short <= 0 ? "text-emerald-600 dark:text-emerald-500" : "text-zinc-500"}`}>
              {short <= 0 ? (
                <>
                  Target met{short < 0 ? <> — <Money cents={-short} /> over</> : null}
                </>
              ) : isCurrentMonth ? (
                <>
                  <Money cents={short} /> to go this month
                </>
              ) : (
                <>
                  Fell <Money cents={short} /> short in {monthLabel.split(" ")[0]}
                </>
              )}
            </p>
          </div>
        ) : null}

        {error ? <p className="text-sm text-red-600">{error}</p> : null}

        {d.accounts.length > 0 ? (
          <ul className="divide-y divide-zinc-100 text-sm dark:divide-zinc-800">
            {d.accounts.map((a) => (
              <li key={a.accountId} className="flex items-baseline justify-between gap-3 py-1.5">
                <span className="min-w-0 truncate">
                  {a.accountName}
                  <span className="ml-2 text-xs text-zinc-400">{KIND_LABELS[a.kind] ?? a.kind}</span>
                </span>
                <Money cents={a.cents} className="tabular-nums" />
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-zinc-500">
            Nothing moved into savings or investments in {monthLabel} yet. A transfer counts once it's tagged as going
            to that account
            {onOpenTransactions ? (
              <>
                {" — "}
                <button className="hover:underline" onClick={onOpenTransactions}>
                  Possible transfers
                </button>{" "}
                on the Transactions page usually has them waiting
              </>
            ) : null}
            .
          </p>
        )}
      </div>
    </Card>
  );
}
