import { useState, type ReactNode } from "react";
import { Button, Logo } from "../components/ui";
import { Dashboard } from "./Dashboard";
import { SettingsPage } from "./SettingsPage";
import { Placeholder } from "./Placeholder";

type Section = "dashboard" | "accounts" | "transactions" | "budgets" | "goals" | "calculators" | "settings";

const NAV: { id: Section; label: string; icon: ReactNode }[] = [
  { id: "dashboard", label: "Dashboard", icon: <Icon d="M3 12l9-8 9 8M5 10v10h14V10" /> },
  { id: "accounts", label: "Accounts", icon: <Icon d="M3 7h18v12H3zM3 11h18" /> },
  { id: "transactions", label: "Transactions", icon: <Icon d="M4 7h16M4 12h16M4 17h10" /> },
  { id: "budgets", label: "Budgets", icon: <Icon d="M12 3v9l7 4M12 21a9 9 0 110-18 9 9 0 010 18z" /> },
  { id: "goals", label: "Goals", icon: <Icon d="M12 21a9 9 0 110-18 9 9 0 010 18zm0-5a4 4 0 110-8 4 4 0 010 8z" /> },
  { id: "calculators", label: "Calculators", icon: <Icon d="M6 3h12v18H6zM9 7h6M9 12h1M14 12h1M9 16h1M14 16h1" /> },
  { id: "settings", label: "Settings", icon: <Icon d="M12 15a3 3 0 100-6 3 3 0 000 6zM4 12h2M18 12h2M12 4v2M12 18v2" /> },
];

function Icon({ d }: { d: string }) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={d} />
    </svg>
  );
}

export function AppShell({ onLock }: { onLock: () => void }) {
  const [section, setSection] = useState<Section>("dashboard");
  const [hideBalances, setHideBalances] = useState(false);

  return (
    <div className="flex h-full">
      <aside className="flex w-60 shrink-0 flex-col border-r border-zinc-200 bg-white px-3 py-4 dark:border-zinc-800 dark:bg-zinc-900">
        <div className="mb-6 flex items-center gap-2 px-2">
          <Logo size={28} />
          <span className="font-semibold">Finance</span>
        </div>
        <nav className="flex-1 space-y-1">
          {NAV.map((n) => (
            <button
              key={n.id}
              onClick={() => setSection(n.id)}
              aria-current={section === n.id ? "page" : undefined}
              className={`flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm ${
                section === n.id
                  ? "bg-brand-50 font-medium text-brand-700 dark:bg-brand-900/40 dark:text-brand-500"
                  : "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
              }`}
            >
              {n.icon}
              {n.label}
            </button>
          ))}
        </nav>
        <div className="space-y-2 border-t border-zinc-200 pt-3 dark:border-zinc-800">
          <label className="flex items-center justify-between px-3 text-sm text-zinc-600 dark:text-zinc-400">
            Hide balances
            <input type="checkbox" checked={hideBalances} onChange={(e) => setHideBalances(e.target.checked)} />
          </label>
          <Button variant="secondary" className="w-full" onClick={onLock}>
            Lock now
          </Button>
        </div>
      </aside>
      <main className="flex-1 overflow-y-auto p-8">
        {section === "dashboard" && <Dashboard hideBalances={hideBalances} />}
        {section === "settings" && <SettingsPage />}
        {section === "accounts" && <Placeholder title="Accounts" phase={2} detail="Add your HYSA, brokerage, 401(k), Roth IRA and more, then connect them live with Plaid in phase 6." />}
        {section === "transactions" && <Placeholder title="Transactions" phase={2} detail="Manual entry and CSV import with duplicate detection." />}
        {section === "budgets" && <Placeholder title="Budgets" phase={3} detail="Monthly limits per category with progress bars." />}
        {section === "goals" && <Placeholder title="Goals" phase={3} detail="Savings targets like a house down payment or emergency fund." />}
        {section === "calculators" && <Placeholder title="Calculators" phase={4} detail="Retirement, Monte Carlo, savings goal, home affordability, rent vs buy, and debt payoff." />}
      </main>
    </div>
  );
}
