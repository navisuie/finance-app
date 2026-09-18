import { useState } from "react";
import { api } from "../../lib/api";
import { formatCents, formatDate } from "../../lib/money";
import type { DebtInput, DebtPayoffInput, StrategyResult } from "../../lib/types";
import { Button, Card, Field } from "../ui";
import { Stat } from "./RetirementCalculator";
import { MoneyField, NumberField } from "./fields";
import { ScenarioBar } from "./ScenarioBar";
import { useCalculation } from "./useCalculation";

const DEFAULT: DebtPayoffInput = {
  debts: [
    { name: "Credit card", balanceCents: 500_000, aprBps: 2200, minimumPaymentCents: 25_000 },
    { name: "Car loan", balanceCents: 1_500_000, aprBps: 649, minimumPaymentCents: 45_000 },
  ],
  extraMonthlyCents: 20_000,
};

function newDebt(): DebtInput {
  return { name: `Debt ${Math.floor(Math.random() * 1000)}`, balanceCents: 0, aprBps: 0, minimumPaymentCents: 0 };
}

export function DebtPayoffCalculator() {
  const [input, setInput] = useState<DebtPayoffInput>(DEFAULT);
  const { result, error } = useCalculation(api.calcDebtPayoff, input);

  const updateDebt = (i: number, patch: Partial<DebtInput>) =>
    setInput((s) => ({ ...s, debts: s.debts.map((d, idx) => (idx === i ? { ...d, ...patch } : d)) }));
  const removeDebt = (i: number) => setInput((s) => ({ ...s, debts: s.debts.filter((_, idx) => idx !== i) }));

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <Card title="Debts">
        <div className="space-y-3">
          {input.debts.map((d, i) => (
            <div key={i} className="grid grid-cols-[1fr_auto] items-start gap-2 rounded-lg bg-zinc-50 p-3 dark:bg-zinc-800/50">
              <div className="grid grid-cols-2 gap-2">
                <div className="col-span-2">
                  <Field label="Name" value={d.name} onChange={(e) => updateDebt(i, { name: e.target.value })} />
                </div>
                <MoneyField label="Balance" cents={d.balanceCents} onChange={(v) => updateDebt(i, { balanceCents: v })} />
                <MoneyField label="Minimum payment" cents={d.minimumPaymentCents} onChange={(v) => updateDebt(i, { minimumPaymentCents: v })} />
                <NumberField label="APR (%)" value={d.aprBps / 100} onChange={(v) => updateDebt(i, { aprBps: Math.round(v * 100) })} step={0.1} min={0} />
              </div>
              <Button variant="ghost" className="mt-6 px-2 py-1 text-red-600" onClick={() => removeDebt(i)} aria-label={`Remove ${d.name}`}>
                ✕
              </Button>
            </div>
          ))}
        </div>
        <Button variant="secondary" className="mt-3 py-1" onClick={() => setInput((s) => ({ ...s, debts: [...s.debts, newDebt()] }))}>
          Add a debt
        </Button>
        <div className="mt-4">
          <MoneyField label="Extra monthly payment" cents={input.extraMonthlyCents} onChange={(v) => setInput((s) => ({ ...s, extraMonthlyCents: v }))} />
        </div>
        <ScenarioBar calculator="debt_payoff" input={input} onLoad={setInput} />
      </Card>

      <Card title="Avalanche vs. snowball">
        {error ? <p className="text-sm text-red-600">{error}</p> : null}
        {result ? (
          <div className="space-y-5">
            <Strategy title="Avalanche (highest APR first)" r={result.avalanche} highlight={result.avalanche.totalInterestCents <= result.snowball.totalInterestCents} />
            <Strategy title="Snowball (smallest balance first)" r={result.snowball} highlight={result.snowball.totalInterestCents < result.avalanche.totalInterestCents} />
            <p className="text-xs text-zinc-500">Avalanche minimizes total interest; snowball can feel faster by clearing small balances first.</p>
          </div>
        ) : null}
      </Card>
    </div>
  );
}

function Strategy({ title, r, highlight }: { title: string; r: StrategyResult; highlight: boolean }) {
  return (
    <div className={`rounded-lg border p-3 ${highlight ? "border-emerald-300 dark:border-emerald-800" : "border-zinc-200 dark:border-zinc-800"}`}>
      <p className="mb-2 text-sm font-medium">{title}</p>
      <dl className="grid grid-cols-2 gap-3 text-sm">
        <Stat label="Debt-free by" value={formatDate(r.payoffOn)} />
        <Stat label="Total interest" value={formatCents(r.totalInterestCents)} tone={highlight ? "good" : undefined} />
      </dl>
      <p className="mt-2 text-xs text-zinc-500">Order: {r.order.join(" → ")}</p>
    </div>
  );
}
