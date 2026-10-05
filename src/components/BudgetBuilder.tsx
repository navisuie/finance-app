import { useEffect, useState } from "react";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import { centsToInput, formatCents, parseMoney } from "../lib/money";
import type { BudgetMonth, LimitInput } from "../lib/types";
import { Modal } from "./Modal";
import { Money } from "./privacy";
import { Button } from "./ui";

const inputClass =
  "w-28 rounded-lg border-0 bg-white px-2.5 py-1.5 text-right text-sm ring-1 ring-zinc-300 outline-none focus:ring-2 focus:ring-brand-500 dark:bg-zinc-900 dark:ring-zinc-700";

/**
 * Builds a month's budget from what already happened: income and saving come
 * from the accounts, debt payments from the debt accounts, and each category's
 * limit starts at its recent average. Everything stays editable before it's
 * applied.
 */
export function BudgetBuilder({
  month,
  monthLabel,
  onClose,
  onApplied,
}: {
  month: string;
  monthLabel: string;
  onClose: () => void;
  onApplied: (updated: BudgetMonth) => void;
}) {
  const draft = useAsync(() => api.budgetDraft(month), [month]);
  const [edits, setEdits] = useState<Record<number, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (!draft.data) return;
    setEdits(Object.fromEntries(draft.data.lines.map((l) => [l.categoryId, centsToInput(l.suggestedCents)])));
  }, [draft.data]);

  const d = draft.data;
  const centsFor = (categoryId: number) => parseMoney(edits[categoryId] ?? "") ?? 0;
  const plannedSpending = d ? d.lines.reduce((sum, l) => sum + centsFor(l.categoryId), 0) : 0;
  const committed = d ? d.savingInvestingCents + d.debtPaymentsCents : 0;
  const leftover = d ? d.monthlyIncomeCents - committed - plannedSpending : 0;

  const apply = async () => {
    if (!d) return;
    setBusy(true);
    setError(undefined);
    try {
      const limits: LimitInput[] = d.lines.map((l) => ({
        categoryId: l.categoryId,
        limitCents: centsFor(l.categoryId),
      }));
      onApplied(await api.applyBudgetPlan(month, limits));
      onClose();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={`Build a budget for ${monthLabel}`} onClose={onClose} wide>
      {draft.error ? <p className="text-sm text-red-600">{draft.error}</p> : null}
      {d && d.monthsOfData === 0 ? (
        <p className="text-sm text-zinc-500">
          There's no history yet for the months before {monthLabel}. Import or add a month of transactions and this
          can suggest limits from what you actually spend.
        </p>
      ) : null}

      {d && d.monthsOfData > 0 ? (
        <div className="space-y-5">
          <p className="text-sm text-zinc-500">
            From the last {d.monthsOfData} month{d.monthsOfData === 1 ? "" : "s"} of your accounts.
          </p>

          <div className="grid grid-cols-3 gap-3">
            <Derived label="Income a month" cents={d.monthlyIncomeCents} />
            <Derived
              label="Saving & investing"
              cents={d.savingInvestingCents}
              hint="Transfers you tagged into savings, brokerage or retirement accounts."
            />
            <Derived
              label="Debt payments"
              cents={d.debtPaymentsCents}
              hint={d.debtPayments.map((p) => `${p.accountName} ${formatCents(p.monthlyPaymentCents)}`).join(" · ")}
            />
          </div>

          <div>
            <div className="mb-2 flex items-baseline justify-between">
              <h3 className="text-sm font-semibold">Spending limits</h3>
              <span className="text-xs text-zinc-500">Suggested from your 3-month average</span>
            </div>
            <div className="max-h-72 divide-y divide-zinc-100 overflow-y-auto rounded-lg ring-1 ring-zinc-200 dark:divide-zinc-800 dark:ring-zinc-800">
              {d.lines.map((l) => (
                <div key={l.categoryId} className="flex items-center justify-between gap-3 px-3 py-2">
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">{l.categoryName}</p>
                    <p className="text-xs text-zinc-500">
                      averaged <Money cents={l.recentAverageCents} />
                      {l.currentLimitCents !== null ? (
                        <>
                          {" · "}now <Money cents={l.currentLimitCents} />
                        </>
                      ) : null}
                    </p>
                  </div>
                  <input
                    className={inputClass}
                    inputMode="decimal"
                    aria-label={`Limit for ${l.categoryName}`}
                    value={edits[l.categoryId] ?? ""}
                    onChange={(e) => setEdits((s) => ({ ...s, [l.categoryId]: e.target.value }))}
                  />
                </div>
              ))}
            </div>
          </div>

          <div className="rounded-lg bg-zinc-50 p-3 text-sm dark:bg-zinc-800/50">
            <Row label="Income" cents={d.monthlyIncomeCents} />
            <Row label="Saving & investing" cents={-d.savingInvestingCents} />
            <Row label="Debt payments" cents={-d.debtPaymentsCents} />
            <Row label="Planned spending" cents={-plannedSpending} />
            <div className="mt-1 flex justify-between border-t border-zinc-200 pt-1 font-medium dark:border-zinc-700">
              <span>{leftover < 0 ? "Short by" : "Left over"}</span>
              <span className={leftover < 0 ? "text-red-600 dark:text-red-400" : "text-emerald-600 dark:text-emerald-500"}>
                <Money cents={Math.abs(leftover)} />
              </span>
            </div>
          </div>

          <p className="text-xs text-zinc-500">
            Applying replaces the limits for {monthLabel}. Debt payments and tagged transfers aren't spending
            categories, so they aren't listed above.
          </p>

          {error ? <p className="text-sm text-red-600">{error}</p> : null}
          <div className="flex justify-end gap-2">
            <Button variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button onClick={apply} disabled={busy}>
              {busy ? "Applying…" : "Apply budget"}
            </Button>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}

function Derived({ label, cents, hint }: { label: string; cents: number; hint?: string }) {
  return (
    <div className="rounded-lg bg-zinc-50 p-3 dark:bg-zinc-800/50">
      <p className="text-xs text-zinc-500">{label}</p>
      <p className="text-lg font-semibold">
        <Money cents={cents} />
      </p>
      {hint ? <p className="mt-0.5 text-xs text-zinc-500">{hint}</p> : null}
    </div>
  );
}

function Row({ label, cents }: { label: string; cents: number }) {
  return (
    <div className="flex justify-between text-zinc-600 dark:text-zinc-400">
      <span>{label}</span>
      <span className="tabular-nums">
        {cents < 0 ? "−" : ""}
        <Money cents={Math.abs(cents)} />
      </span>
    </div>
  );
}
