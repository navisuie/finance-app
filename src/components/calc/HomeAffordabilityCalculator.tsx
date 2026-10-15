import { useState } from "react";
import { api } from "../../lib/api";
import { formatCents } from "../../lib/money";
import type { HomeAffordabilityInput } from "../../lib/types";
import { Card } from "../ui";
import { Stat } from "./RetirementCalculator";
import { MoneyField, NumberField } from "./fields";
import { ScenarioBar } from "./ScenarioBar";
import { useCalculation } from "./useCalculation";

const DEFAULT: HomeAffordabilityInput = {
  grossAnnualIncomeCents: 12_000_000, // $120,000
  monthlyDebtsCents: 40_000, // $400/mo
  downPaymentCents: 6_000_000, // $60,000
  mortgageRatePct: 6.5,
  termYears: 30,
  propertyTaxPct: 1.2,
  monthlyInsuranceCents: 15_000, // $150/mo
  monthlyHoaCents: 0,
};

const ROWS: { key: "principalInterestCents" | "taxCents" | "insuranceCents" | "hoaCents" | "pmiCents"; label: string }[] = [
  { key: "principalInterestCents", label: "Principal & interest" },
  { key: "taxCents", label: "Property tax" },
  { key: "insuranceCents", label: "Insurance" },
  { key: "hoaCents", label: "HOA" },
  { key: "pmiCents", label: "PMI" },
];

export function HomeAffordabilityCalculator() {
  const [input, setInput] = useState<HomeAffordabilityInput>(DEFAULT);
  const { result, error } = useCalculation(api.calcHomeAffordability, input);
  const set = <K extends keyof HomeAffordabilityInput>(key: K, value: HomeAffordabilityInput[K]) => setInput((i) => ({ ...i, [key]: value }));

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <Card title="Inputs">
        <div className="grid grid-cols-2 gap-3">
          <MoneyField label="Gross annual income" cents={input.grossAnnualIncomeCents} onChange={(v) => set("grossAnnualIncomeCents", v)} />
          <MoneyField label="Other monthly debts" cents={input.monthlyDebtsCents} onChange={(v) => set("monthlyDebtsCents", v)} />
          <MoneyField label="Down payment" cents={input.downPaymentCents} onChange={(v) => set("downPaymentCents", v)} />
          <NumberField label="Mortgage rate (%)" value={input.mortgageRatePct} onChange={(v) => set("mortgageRatePct", v)} />
          <NumberField label="Term (years)" value={input.termYears} onChange={(v) => set("termYears", v)} min={1} max={40} />
          <NumberField label="Property tax (%/yr)" value={input.propertyTaxPct} onChange={(v) => set("propertyTaxPct", v)} />
          <MoneyField label="Insurance (monthly)" cents={input.monthlyInsuranceCents} onChange={(v) => set("monthlyInsuranceCents", v)} />
          <MoneyField label="HOA (monthly)" cents={input.monthlyHoaCents} onChange={(v) => set("monthlyHoaCents", v)} />
        </div>
        <ScenarioBar calculator="home" input={input} onLoad={setInput} />
      </Card>

      <Card title="What you can afford">
        {error ? <p className="text-sm text-red-600">{error}</p> : null}
        {result ? (
          <div className="space-y-4">
            <dl className="grid grid-cols-2 gap-3 text-sm">
              <Stat label="Max home price" value={formatCents(result.maxHomePriceCents)} tone="good" />
              <Stat label="Max loan" value={formatCents(result.maxLoanCents)} />
            </dl>
            <div>
              <p className="mb-1.5 text-xs text-zinc-500">Monthly payment — {formatCents(result.monthlyPayment.totalCents)}/mo</p>
              <div className="flex h-3 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
                {ROWS.filter((r) => result.monthlyPayment[r.key] > 0).map((r, i) => (
                  <div
                    key={r.key}
                    style={{ width: `${(result.monthlyPayment[r.key] / result.monthlyPayment.totalCents) * 100}%`, opacity: 1 - i * 0.15 }}
                    className="h-full bg-[var(--viz-line)]"
                    title={`${r.label}: ${formatCents(result.monthlyPayment[r.key])}`}
                  />
                ))}
              </div>
              <ul className="mt-2 space-y-0.5 text-sm">
                {ROWS.filter((r) => result.monthlyPayment[r.key] > 0).map((r) => (
                  <li key={r.key} className="flex justify-between">
                    <span className="text-zinc-500">{r.label}</span>
                    <span className="tabular-nums">{formatCents(result.monthlyPayment[r.key])}</span>
                  </li>
                ))}
              </ul>
            </div>
            <p className="text-xs text-zinc-500">
              Under the 28% (housing) / 36% (total debt) rule. A projection, not advice.
            </p>
          </div>
        ) : null}
      </Card>
    </div>
  );
}
