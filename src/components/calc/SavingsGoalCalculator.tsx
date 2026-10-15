import { useState } from "react";
import { api } from "../../lib/api";
import { formatCents, formatDate, todayISO } from "../../lib/money";
import type { SavingsGoalInput } from "../../lib/types";
import { Card, Field } from "../ui";
import { Stat } from "./RetirementCalculator";
import { MoneyField, NumberField } from "./fields";
import { ScenarioBar } from "./ScenarioBar";
import { useCalculation } from "./useCalculation";

function inOneYear(): string {
  const d = new Date();
  d.setFullYear(d.getFullYear() + 1);
  return d.toISOString().slice(0, 10);
}

const DEFAULT: SavingsGoalInput = {
  targetCents: 2_000_000, // $20,000
  targetDate: inOneYear(),
  currentSavingsCents: 500_000, // $5,000
  hysaApyPct: 4.5,
  currentMonthlyDepositCents: 100_000, // $1,000/mo
};

export function SavingsGoalCalculator() {
  const [input, setInput] = useState<SavingsGoalInput>(DEFAULT);
  const { result, error } = useCalculation(api.calcSavingsGoal, input);
  const set = <K extends keyof SavingsGoalInput>(key: K, value: SavingsGoalInput[K]) => setInput((i) => ({ ...i, [key]: value }));

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <Card title="Inputs">
        <div className="grid grid-cols-2 gap-3">
          <MoneyField label="Target amount" cents={input.targetCents} onChange={(v) => set("targetCents", v)} />
          <Field label="Target date" type="date" min={todayISO()} value={input.targetDate} onChange={(e) => set("targetDate", e.target.value)} />
          <MoneyField label="Current savings" cents={input.currentSavingsCents} onChange={(v) => set("currentSavingsCents", v)} />
          <NumberField label="HYSA APY (%)" value={input.hysaApyPct} onChange={(v) => set("hysaApyPct", v)} min={0} />
          <div className="col-span-2">
            <MoneyField label="What you deposit monthly today" cents={input.currentMonthlyDepositCents} onChange={(v) => set("currentMonthlyDepositCents", v)} />
          </div>
        </div>
        <ScenarioBar calculator="savings_goal" input={input} onLoad={setInput} />
      </Card>

      <Card title="Result">
        {error ? <p className="text-sm text-red-600">{error}</p> : null}
        {result ? (
          <div className="space-y-4">
            {result.alreadyReached ? (
              <p className="text-sm font-medium text-emerald-600 dark:text-emerald-500">You're already there.</p>
            ) : (
              <dl className="grid grid-cols-2 gap-3 text-sm">
                <Stat label="Needed monthly to hit the date" value={formatCents(result.requiredMonthlyDepositCents)} />
                <Stat
                  label="At your current pace"
                  value={result.projectedCompletionDate ? formatDate(result.projectedCompletionDate) : "Never reaches it"}
                  tone={result.projectedCompletionDate ? "good" : "bad"}
                />
              </dl>
            )}
            <p className="text-xs text-zinc-500">A projection, not advice.</p>
          </div>
        ) : null}
      </Card>
    </div>
  );
}
