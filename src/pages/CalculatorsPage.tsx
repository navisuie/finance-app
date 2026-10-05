import { useState } from "react";
import { BudgetPlanCalculator } from "../components/calc/BudgetPlanCalculator";
import { DebtPayoffCalculator } from "../components/calc/DebtPayoffCalculator";
import { HomeAffordabilityCalculator } from "../components/calc/HomeAffordabilityCalculator";
import { MonteCarloCalculator } from "../components/calc/MonteCarloCalculator";
import { RentVsBuyCalculator } from "../components/calc/RentVsBuyCalculator";
import { RetirementCalculator } from "../components/calc/RetirementCalculator";
import { SavingsGoalCalculator } from "../components/calc/SavingsGoalCalculator";
import { Card } from "../components/ui";
import type { CalculatorId } from "../lib/types";

const TILES: { id: CalculatorId; title: string; blurb: string }[] = [
  { id: "retirement", title: "Retirement projection", blurb: "Will your savings support the retirement you want." },
  { id: "monte_carlo", title: "Monte Carlo retirement", blurb: "The odds your money lasts, across simulated markets." },
  { id: "savings_goal", title: "Savings goal", blurb: "What to set aside monthly to hit a target by a date." },
  { id: "home", title: "Home affordability", blurb: "Max home price under the 28/36 rule." },
  { id: "rent_vs_buy", title: "Rent vs. buy", blurb: "Net wealth under each path, year by year." },
  { id: "debt_payoff", title: "Debt payoff", blurb: "Avalanche vs. snowball, payoff date and total interest." },
  { id: "budget_plan", title: "Budget from a salary", blurb: "Give a salary and the costs you know; it fills in the rest." },
];

export function CalculatorsPage() {
  const [selected, setSelected] = useState<CalculatorId>();

  if (selected) {
    const tile = TILES.find((t) => t.id === selected)!;
    return (
      <div className="max-w-5xl space-y-4">
        <div className="flex items-center gap-3">
          <button className="text-sm text-zinc-500 hover:underline" onClick={() => setSelected(undefined)}>
            ← Calculators
          </button>
          <h1 className="text-xl font-semibold">{tile.title}</h1>
        </div>
        <Calculator id={selected} />
      </div>
    );
  }

  return (
    <div className="max-w-5xl space-y-4">
      <h1 className="text-2xl font-semibold">Calculators</h1>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {TILES.map((t) => (
          <button key={t.id} onClick={() => setSelected(t.id)} className="text-left">
            <Card className="h-full transition hover:ring-2 hover:ring-brand-500">
              <p className="font-medium">{t.title}</p>
              <p className="mt-1 text-sm text-zinc-500">{t.blurb}</p>
            </Card>
          </button>
        ))}
      </div>
    </div>
  );
}

function Calculator({ id }: { id: CalculatorId }) {
  switch (id) {
    case "retirement":
      return <RetirementCalculator />;
    case "monte_carlo":
      return <MonteCarloCalculator />;
    case "savings_goal":
      return <SavingsGoalCalculator />;
    case "home":
      return <HomeAffordabilityCalculator />;
    case "rent_vs_buy":
      return <RentVsBuyCalculator />;
    case "debt_payoff":
      return <DebtPayoffCalculator />;
    case "budget_plan":
      return <BudgetPlanCalculator />;
  }
}
