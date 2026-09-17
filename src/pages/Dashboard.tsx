import { Card } from "../components/ui";

const CARDS = [
  { title: "Net worth", note: "Assets minus debts, with a 12-month trend" },
  { title: "Spending this month", note: "Compared with your budgets" },
  { title: "Savings rate", note: "Share of income you kept" },
  { title: "Goals", note: "Progress toward each target" },
];

export function Dashboard({ hideBalances }: { hideBalances: boolean }) {
  return (
    <div className="max-w-5xl space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Dashboard</h1>
        <p className="text-sm text-zinc-500">Your vault is unlocked and encrypted. Add accounts to fill this in.</p>
      </div>
      <div className="grid grid-cols-2 gap-4">
        {CARDS.map((c) => (
          <Card key={c.title} title={c.title}>
            <p className={`text-3xl font-semibold tabular-nums ${hideBalances ? "money-hidden" : ""}`}>$0.00</p>
            <p className="mt-1 text-xs text-zinc-500">{c.note}</p>
          </Card>
        ))}
      </div>
      <Card title="Recent transactions">
        <p className="text-sm text-zinc-500">No transactions yet.</p>
      </Card>
    </div>
  );
}
