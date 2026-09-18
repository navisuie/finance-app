import { useState } from "react";
import { api } from "../../lib/api";
import type { RentVsBuyInput } from "../../lib/types";
import { Card } from "../ui";
import { Stat } from "./RetirementCalculator";
import { MoneyField, NumberField } from "./fields";
import { LineChart } from "./LineChart";
import { ScenarioBar } from "./ScenarioBar";
import { useCalculation } from "./useCalculation";

const DEFAULT: RentVsBuyInput = {
  monthlyRentCents: 250_000, // $2,500/mo
  annualRentGrowthPct: 3,
  homePriceCents: 50_000_000, // $500,000
  downPaymentCents: 10_000_000, // $100,000
  mortgageRatePct: 6.5,
  termYears: 30,
  maintenancePct: 1,
  annualAppreciationPct: 3.5,
  yearsStaying: 10,
  investmentReturnPct: 7,
};

export function RentVsBuyCalculator() {
  const [input, setInput] = useState<RentVsBuyInput>(DEFAULT);
  const { result, error } = useCalculation(api.calcRentVsBuy, input);
  const set = <K extends keyof RentVsBuyInput>(key: K, value: RentVsBuyInput[K]) => setInput((i) => ({ ...i, [key]: value }));

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <Card title="Inputs">
        <div className="grid grid-cols-2 gap-3">
          <MoneyField label="Monthly rent" cents={input.monthlyRentCents} onChange={(v) => set("monthlyRentCents", v)} />
          <NumberField label="Rent growth (%/yr)" value={input.annualRentGrowthPct} onChange={(v) => set("annualRentGrowthPct", v)} step={0.5} />
          <MoneyField label="Home price" cents={input.homePriceCents} onChange={(v) => set("homePriceCents", v)} />
          <MoneyField label="Down payment" cents={input.downPaymentCents} onChange={(v) => set("downPaymentCents", v)} />
          <NumberField label="Mortgage rate (%)" value={input.mortgageRatePct} onChange={(v) => set("mortgageRatePct", v)} step={0.1} />
          <NumberField label="Term (years)" value={input.termYears} onChange={(v) => set("termYears", v)} step={5} min={1} max={40} />
          <NumberField label="Maintenance (%/yr of value)" value={input.maintenancePct} onChange={(v) => set("maintenancePct", v)} step={0.1} />
          <NumberField label="Appreciation (%/yr)" value={input.annualAppreciationPct} onChange={(v) => set("annualAppreciationPct", v)} step={0.5} />
          <NumberField label="Years you'll stay" value={input.yearsStaying} onChange={(v) => set("yearsStaying", v)} step={1} min={1} max={40} />
          <NumberField label="Investment return (%/yr)" value={input.investmentReturnPct} onChange={(v) => set("investmentReturnPct", v)} step={0.5} />
        </div>
        <ScenarioBar calculator="rent_vs_buy" input={input} onLoad={setInput} />
      </Card>

      <Card title="Net wealth by year">
        {error ? <p className="text-sm text-red-600">{error}</p> : null}
        {result ? (
          <div className="space-y-4">
            <LineChart
              xLabels={result.byYear.map((y) => `Yr ${y.year}`)}
              series={[
                { label: "Buy (equity)", values: result.byYear.map((y) => y.buyNetWorthCents), color: "line" },
                { label: "Rent + invest", values: result.byYear.map((y) => y.rentNetWorthCents), color: "over" },
              ]}
            />
            <Stat
              label="Break-even year"
              value={result.breakEvenYear !== null ? `Year ${result.breakEvenYear}` : `Not within ${input.yearsStaying} years`}
              tone={result.breakEvenYear !== null ? "good" : undefined}
            />
            <p className="text-xs text-zinc-500">
              Ignores closing/selling costs and tax effects. A projection, not advice.
            </p>
          </div>
        ) : null}
      </Card>
    </div>
  );
}
