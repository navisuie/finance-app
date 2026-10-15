import { useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { formatCents } from "../../lib/money";
import type { RetirementInput } from "../../lib/types";
import { Card } from "../ui";
import { MoneyField, NumberField } from "./fields";
import { LineChart } from "./LineChart";
import { ScenarioBar } from "./ScenarioBar";
import { useCalculation } from "./useCalculation";
import { useRetirementBalance } from "./useRetirementBalance";

export const RETIREMENT_DEFAULT: RetirementInput = {
  currentAge: 30,
  retirementAge: 65,
  balanceCents: 5_000_000, // $50,000
  monthlyContributionCents: 150_000, // $1,500/mo
  employerMatchPct: 50,
  expectedReturnPct: 7,
  inflationPct: 3,
  desiredAnnualIncomeCents: 8_000_000, // $80,000/yr
  withdrawalRatePct: 4,
};

export function RetirementFields<T extends RetirementInput>({ input, set }: { input: T; set: <K extends keyof RetirementInput>(key: K, value: RetirementInput[K]) => void }) {
  const accountBalance = useRetirementBalance();
  const autofilled = useRef(false);

  // Pre-fill from real account balances once, the first time they're
  // available — never overwrites an edit you've already made.
  useEffect(() => {
    if (autofilled.current || accountBalance.loading || accountBalance.count === 0) return;
    autofilled.current = true;
    set("balanceCents", accountBalance.totalCents);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountBalance.loading, accountBalance.count]);

  return (
    <div className="grid grid-cols-2 gap-3">
      <NumberField label="Current age" value={input.currentAge} onChange={(v) => set("currentAge", v)} min={16} max={100} />
      <NumberField label="Retirement age" value={input.retirementAge} onChange={(v) => set("retirementAge", v)} min={16} max={100} />
      <MoneyField
        label="Current balance (401k + Roth + brokerage + HYSA)"
        cents={input.balanceCents}
        onChange={(v) => set("balanceCents", v)}
        hint={
          accountBalance.count > 0 ? (
            <span>
              {accountBalance.count} account{accountBalance.count === 1 ? "" : "s"} ({accountBalance.label}) total{" "}
              {formatCents(accountBalance.totalCents)}.{" "}
              <button type="button" className="hover:underline" onClick={() => set("balanceCents", accountBalance.totalCents)}>
                Use this
              </button>
            </span>
          ) : (
            "No 401(k), Roth IRA, brokerage or HYSA accounts yet — add one on the Accounts page to pull this in automatically."
          )
        }
      />
      <MoneyField label="Your monthly contribution" cents={input.monthlyContributionCents} onChange={(v) => set("monthlyContributionCents", v)} />
      <NumberField label="Employer match (%)" value={input.employerMatchPct} onChange={(v) => set("employerMatchPct", v)} min={0} max={200} hint="Applied on top of your contribution." />
      <NumberField label="Expected return (%/yr)" value={input.expectedReturnPct} onChange={(v) => set("expectedReturnPct", v)} />
      <NumberField label="Inflation (%/yr)" value={input.inflationPct} onChange={(v) => set("inflationPct", v)} />
      <NumberField label="Withdrawal rate (%)" value={input.withdrawalRatePct} onChange={(v) => set("withdrawalRatePct", v)} min={0.5} />
      <div className="col-span-2">
        <MoneyField label="Desired annual retirement income (today's $)" cents={input.desiredAnnualIncomeCents} onChange={(v) => set("desiredAnnualIncomeCents", v)} />
      </div>
    </div>
  );
}

export function RetirementCalculator() {
  const [input, setInput] = useState<RetirementInput>(RETIREMENT_DEFAULT);
  const { result, error } = useCalculation(api.calcRetirement, input);
  const set = <K extends keyof RetirementInput>(key: K, value: RetirementInput[K]) =>
    setInput((i) => ({ ...i, [key]: value }));

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <Card title="Inputs">
        <RetirementFields input={input} set={set} />
        <ScenarioBar calculator="retirement" input={input} onLoad={setInput} />
      </Card>

      <Card title="Projection">
        {error ? <p className="text-sm text-red-600">{error}</p> : null}
        {result ? (
          <div className="space-y-4">
            <LineChart
              xLabels={result.balanceByYearCents.map((_, i) => `${input.currentAge + i}`)}
              series={[{ label: "Balance", values: result.balanceByYearCents, color: "line" }]}
            />
            <dl className="grid grid-cols-2 gap-3 text-sm">
              <Stat label="Balance at retirement (today's $)" value={formatCents(result.projectedBalanceCents)} />
              <Stat label="Sustainable annual income" value={formatCents(result.sustainableAnnualIncomeCents)} />
              <Stat
                label={result.gapCents >= 0 ? "Surplus" : "Shortfall"}
                value={formatCents(Math.abs(result.gapCents))}
                tone={result.gapCents >= 0 ? "good" : "bad"}
              />
              <Stat label="Monthly savings needed to close it" value={formatCents(result.requiredMonthlySavingsCents)} />
            </dl>
            <p className="text-xs text-zinc-500">
              A projection, not advice. All figures are in today's dollars (inflation-adjusted).
            </p>
          </div>
        ) : null}
      </Card>
    </div>
  );
}

export function Stat({ label, value, tone }: { label: string; value: string; tone?: "good" | "bad" }) {
  return (
    <div>
      <dt className="text-xs text-zinc-500">{label}</dt>
      <dd className={`text-lg font-semibold tabular-nums ${tone === "good" ? "text-emerald-600 dark:text-emerald-500" : tone === "bad" ? "text-red-600 dark:text-red-500" : ""}`}>
        {value}
      </dd>
    </div>
  );
}
