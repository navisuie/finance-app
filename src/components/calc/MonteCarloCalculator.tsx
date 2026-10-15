import { useState } from "react";
import { api } from "../../lib/api";
import { formatCents } from "../../lib/money";
import type { MonteCarloInput } from "../../lib/types";
import { Card } from "../ui";
import { RETIREMENT_DEFAULT, RetirementFields, Stat } from "./RetirementCalculator";
import type { RetirementInput } from "../../lib/types";
import { NumberField } from "./fields";
import { LineChart } from "./LineChart";
import { ScenarioBar } from "./ScenarioBar";
import { useCalculation } from "./useCalculation";

const DEFAULT: MonteCarloInput = { ...RETIREMENT_DEFAULT, returnVolatilityPct: 15 };

export function MonteCarloCalculator() {
  const [input, setInput] = useState<MonteCarloInput>(DEFAULT);
  const { result, error } = useCalculation(api.calcRetirementMonteCarlo, input);
  const set = <K extends keyof RetirementInput>(key: K, value: RetirementInput[K]) => setInput((i) => ({ ...i, [key]: value }));

  return (
    <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <Card title="Inputs">
        <RetirementFields input={input} set={set} />
        <div className="mt-3">
          <NumberField
            label="Return volatility (%/yr)"
            value={input.returnVolatilityPct}
            onChange={(v) => setInput((i) => ({ ...i, returnVolatilityPct: v }))}
            min={0}
            hint="Higher volatility widens the range of outcomes below."
          />
        </div>
        <ScenarioBar calculator="monte_carlo" input={input} onLoad={setInput} />
      </Card>

      <Card title="Simulated markets">
        {error ? <p className="text-sm text-red-600">{error}</p> : null}
        {result ? (
          <div className="space-y-4">
            <LineChart
              xLabels={result.p50ByYearCents.map((_, i) => `${input.currentAge + i}`)}
              series={[{ label: "Median", values: result.p50ByYearCents, color: "line" }]}
              band={{ low: result.p10ByYearCents, high: result.p90ByYearCents }}
            />
            <dl className="grid grid-cols-2 gap-3 text-sm">
              <Stat
                label="Money lasts to 95"
                value={`${result.successProbabilityPct.toFixed(0)}%`}
                tone={result.successProbabilityPct >= 80 ? "good" : result.successProbabilityPct < 50 ? "bad" : undefined}
              />
              <Stat label="Median balance at retirement" value={formatCents(result.p50ByYearCents[input.retirementAge - input.currentAge])} />
            </dl>
            <p className="text-xs text-zinc-500">
              Shaded band is the 10th–90th percentile range across simulated markets. A projection, not advice.
            </p>
          </div>
        ) : null}
      </Card>
    </div>
  );
}
