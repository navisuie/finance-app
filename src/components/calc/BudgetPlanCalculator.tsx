import { useState } from "react";
import { useAsync } from "../../hooks/useAsync";
import { api, errorMessage } from "../../lib/api";
import { formatCents, todayISO } from "../../lib/money";
import type { Bucket, BudgetPlanInput, BudgetPlanLine, FixedExpense, LimitInput } from "../../lib/types";
import { Modal } from "../Modal";
import { Button, Card, Field } from "../ui";
import { Money } from "../privacy";
import { Stat } from "./RetirementCalculator";
import { MoneyField, NumberField } from "./fields";
import { ScenarioBar } from "./ScenarioBar";
import { useCalculation } from "./useCalculation";

const BUCKET_LABEL: Record<Bucket, string> = { needs: "Needs", wants: "Wants", savings: "Savings" };

/** Categories a fixed cost can take over, so it isn't budgeted twice. */
const COVERS: Record<Bucket, string[]> = {
  needs: ["Housing", "Groceries", "Transportation", "Utilities", "Insurance", "Health"],
  wants: ["Dining", "Shopping", "Entertainment", "Subscriptions", "Travel"],
  savings: ["Retirement", "Emergency fund", "Other savings"],
};

const DEFAULT: BudgetPlanInput = {
  grossAnnualIncomeCents: 9_600_000, // $96,000
  taxRatePct: 25,
  fixedExpenses: [
    { label: "Rent", monthlyCents: 180_000, bucket: "needs", covers: "Housing" },
    { label: "Car payment", monthlyCents: 70_000, bucket: "needs", covers: "Transportation" },
  ],
  needsPct: 50,
  wantsPct: 30,
  savingsPct: 20,
};

export function BudgetPlanCalculator() {
  const [input, setInput] = useState<BudgetPlanInput>(DEFAULT);
  const [applying, setApplying] = useState(false);
  const { result, error } = useCalculation(api.calcBudgetPlan, input);
  const set = <K extends keyof BudgetPlanInput>(key: K, value: BudgetPlanInput[K]) =>
    setInput((i) => ({ ...i, [key]: value }));

  const updateExpense = (i: number, patch: Partial<FixedExpense>) =>
    setInput((s) => ({
      ...s,
      fixedExpenses: s.fixedExpenses.map((e, idx) => (idx === i ? { ...e, ...patch } : e)),
    }));
  const removeExpense = (i: number) =>
    setInput((s) => ({ ...s, fixedExpenses: s.fixedExpenses.filter((_, idx) => idx !== i) }));

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <Card title="What you know">
        <div className="grid grid-cols-2 gap-3">
          <MoneyField
            label="Salary (gross, per year)"
            cents={input.grossAnnualIncomeCents}
            onChange={(v) => set("grossAnnualIncomeCents", v)}
          />
          <NumberField
            label="Withheld (%)"
            value={input.taxRatePct}
            onChange={(v) => set("taxRatePct", v)}
            min={0}
            max={99}
            hint="Taxes, payroll deductions, premiums."
          />
        </div>

        <div className="mt-4">
          <p className="mb-2 text-sm font-medium">Costs you already know</p>
          <div className="space-y-2">
            {input.fixedExpenses.map((e, i) => (
              <div key={i} className="grid grid-cols-[1fr_auto] items-end gap-2 rounded-lg bg-zinc-50 p-2.5 dark:bg-zinc-800/50">
                <div className="grid grid-cols-2 gap-2">
                  <Field label="What" value={e.label} onChange={(ev) => updateExpense(i, { label: ev.target.value })} />
                  <MoneyField
                    label="Per month"
                    cents={e.monthlyCents}
                    onChange={(v) => updateExpense(i, { monthlyCents: v })}
                  />
                  <label className="space-y-1.5 text-sm">
                    <span className="block font-medium text-zinc-700 dark:text-zinc-300">Counts as</span>
                    <select
                      className="block w-full rounded-lg border-0 bg-white px-3 py-2 text-sm ring-1 ring-zinc-300 dark:bg-zinc-900 dark:ring-zinc-700"
                      value={e.bucket}
                      onChange={(ev) => updateExpense(i, { bucket: ev.target.value as Bucket, covers: null })}
                    >
                      {(["needs", "wants", "savings"] as Bucket[]).map((b) => (
                        <option key={b} value={b}>
                          {BUCKET_LABEL[b]}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="space-y-1.5 text-sm">
                    <span className="block font-medium text-zinc-700 dark:text-zinc-300">Replaces</span>
                    <select
                      className="block w-full rounded-lg border-0 bg-white px-3 py-2 text-sm ring-1 ring-zinc-300 dark:bg-zinc-900 dark:ring-zinc-700"
                      value={e.covers ?? ""}
                      onChange={(ev) => updateExpense(i, { covers: ev.target.value || null })}
                    >
                      <option value="">Nothing (add on top)</option>
                      {COVERS[e.bucket].map((c) => (
                        <option key={c} value={c}>
                          {c}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <Button variant="ghost" className="px-2 py-1 text-red-600" onClick={() => removeExpense(i)} aria-label={`Remove ${e.label}`}>
                  ✕
                </Button>
              </div>
            ))}
          </div>
          <Button
            variant="secondary"
            className="mt-2 py-1"
            onClick={() =>
              setInput((s) => ({
                ...s,
                fixedExpenses: [...s.fixedExpenses, { label: "", monthlyCents: 0, bucket: "needs", covers: null }],
              }))
            }
          >
            Add a cost
          </Button>
        </div>

        <div className="mt-4 grid grid-cols-3 gap-3">
          <NumberField label="Needs (%)" value={input.needsPct} onChange={(v) => set("needsPct", v)} min={0} max={100} />
          <NumberField label="Wants (%)" value={input.wantsPct} onChange={(v) => set("wantsPct", v)} min={0} max={100} />
          <NumberField label="Savings (%)" value={input.savingsPct} onChange={(v) => set("savingsPct", v)} min={0} max={100} />
        </div>

        <ScenarioBar calculator="budget_plan" input={input} onLoad={setInput} />
      </Card>

      <Card title="Your budget">
        {error ? <p className="text-sm text-red-600">{error}</p> : null}
        {result ? (
          <div className="space-y-4">
            <Stat label="Take-home a month" value={formatCents(result.monthlyTakeHomeCents)} />

            {result.overAllocated ? (
              <p className="rounded-lg bg-red-50 p-2.5 text-sm text-red-700 dark:bg-red-950/40 dark:text-red-400">
                Your fixed costs use up more than their share. Trim them, or shift the percentages below.
              </p>
            ) : null}

            {(["needs", "wants", "savings"] as Bucket[]).map((bucket) => {
              const summary = result.buckets.find((b) => b.bucket === bucket)!;
              const lines = result.lines.filter((l) => l.bucket === bucket);
              return (
                <div key={bucket}>
                  <div className="flex items-baseline justify-between border-b border-zinc-200 pb-1 dark:border-zinc-800">
                    <h3 className="text-sm font-semibold">{BUCKET_LABEL[bucket]}</h3>
                    <span className="text-sm text-zinc-500">
                      <Money cents={summary.targetCents} /> a month
                    </span>
                  </div>
                  <ul className="mt-1.5 space-y-1 text-sm">
                    {lines.map((l, i) => (
                      <li key={`${l.label}-${i}`} className="flex justify-between">
                        <span className={l.source === "fixed" ? "font-medium" : "text-zinc-600 dark:text-zinc-400"}>
                          {l.label || "Unnamed"}
                          {l.source === "fixed" ? <span className="ml-1.5 text-xs text-zinc-400">yours</span> : null}
                        </span>
                        <span className="tabular-nums">
                          <Money cents={l.monthlyCents} />
                        </span>
                      </li>
                    ))}
                    {lines.length === 0 ? <li className="text-zinc-500">Nothing left to allocate here.</li> : null}
                  </ul>
                </div>
              );
            })}

            <p className="text-xs text-zinc-500">
              Filled in against a {input.needsPct}/{input.wantsPct}/{input.savingsPct} split, rounded to the nearest
              $5. A starting point, not advice.
            </p>

            <Button variant="secondary" onClick={() => setApplying(true)}>
              Use this as my budget…
            </Button>
          </div>
        ) : null}
      </Card>

      {applying && result ? <ApplyPlanDialog lines={result.lines} onClose={() => setApplying(false)} /> : null}
    </div>
  );
}

/** Writes the plan's category lines into a month's budget limits. */
function ApplyPlanDialog({ lines, onClose }: { lines: BudgetPlanLine[]; onClose: () => void }) {
  const categories = useAsync(() => api.listCategories(), []);
  const [month, setMonth] = useState(todayISO().slice(0, 7));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [applied, setApplied] = useState(false);

  // Several lines can land on one category (two fixed costs both covering
  // Housing, say), so add them up before writing.
  const totals = new Map<string, number>();
  const skipped: BudgetPlanLine[] = [];
  for (const l of lines) {
    if (l.category) totals.set(l.category, (totals.get(l.category) ?? 0) + l.monthlyCents);
    else if (l.monthlyCents > 0) skipped.push(l);
  }

  const expense = (categories.data ?? []).filter((c) => c.kind === "expense");
  const matched = [...totals.entries()].flatMap(([name, cents]) => {
    const category = expense.find((c) => c.name.toLowerCase() === name.toLowerCase());
    return category ? [{ categoryId: category.id, name: category.name, cents }] : [];
  });
  const unmatched = [...totals.keys()].filter(
    (name) => !expense.some((c) => c.name.toLowerCase() === name.toLowerCase()),
  );

  const apply = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const limits: LimitInput[] = matched.map((m) => ({ categoryId: m.categoryId, limitCents: m.cents }));
      await api.applyBudgetPlan(month, limits);
      setApplied(true);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title="Use this as my budget" onClose={onClose}>
      {applied ? (
        <div className="space-y-4">
          <p className="text-sm">
            Set {matched.length} limit{matched.length === 1 ? "" : "s"} for {month}. Open Budgets to see them against
            what you've actually spent.
          </p>
          <div className="flex justify-end">
            <Button onClick={onClose}>Done</Button>
          </div>
        </div>
      ) : (
        <div className="space-y-4">
          <label className="block space-y-1.5 text-sm">
            <span className="font-medium text-zinc-700 dark:text-zinc-300">Month</span>
            <input
              type="month"
              value={month}
              onChange={(e) => setMonth(e.target.value)}
              className="block rounded-lg border-0 bg-white px-3 py-2 text-sm ring-1 ring-zinc-300 dark:bg-zinc-900 dark:ring-zinc-700"
            />
          </label>

          <div>
            <p className="mb-1.5 text-sm font-medium">These limits get set</p>
            <ul className="max-h-56 divide-y divide-zinc-100 overflow-y-auto rounded-lg text-sm ring-1 ring-zinc-200 dark:divide-zinc-800 dark:ring-zinc-800">
              {matched.map((m) => (
                <li key={m.categoryId} className="flex justify-between px-3 py-1.5">
                  <span>{m.name}</span>
                  <span className="tabular-nums">
                    <Money cents={m.cents} />
                  </span>
                </li>
              ))}
              {matched.length === 0 ? <li className="px-3 py-2 text-zinc-500">Nothing to apply yet.</li> : null}
            </ul>
          </div>

          {skipped.length > 0 || unmatched.length > 0 ? (
            <p className="text-xs text-zinc-500">
              Not applied:{" "}
              {[...skipped.map((s) => `${s.label} (covers no category)`), ...unmatched.map((u) => `${u} (no such category)`)].join(
                ", ",
              )}
              . Savings lines aren't spending categories — track those as goals or transfers.
            </p>
          ) : null}

          <p className="text-xs text-zinc-500">
            Limits for categories not listed above are left as they are.
          </p>

          {error ? <p className="text-sm text-red-600">{error}</p> : null}
          <div className="flex justify-end gap-2">
            <Button variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button onClick={apply} disabled={busy || matched.length === 0}>
              {busy ? "Applying…" : `Set ${matched.length} limit${matched.length === 1 ? "" : "s"}`}
            </Button>
          </div>
        </div>
      )}
    </Modal>
  );
}
