import { useEffect, useState, type ReactNode } from "react";
import { api } from "../lib/api";
import { Button, Logo } from "../components/ui";
import { Dashboard } from "./Dashboard";
import { SettingsPage } from "./SettingsPage";
import { Placeholder } from "./Placeholder";
import { AccountsPage } from "./AccountsPage";
import { BudgetsPage } from "./BudgetsPage";
import { GoalsPage } from "./GoalsPage";
import { TransactionsPage } from "./TransactionsPage";
import { PrivacyContext } from "../components/privacy";
import { EnvironmentSwitch } from "../components/EnvironmentSwitch";
import { announceEnvironmentChange, usePlaidStatus } from "../hooks/usePlaidStatus";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
  const [syncNote, setSyncNote] = useState<string>();
  const { status: plaid, reload: reloadPlaid } = usePlaidStatus();
  const sandbox = plaid?.environment === "sandbox";

  // The window title says which data you're looking at, even when the app is
  // behind another window.
  useEffect(() => {
    if (!plaid) return;
    try {
      getCurrentWindow()
        .setTitle(sandbox ? "Finance — SANDBOX (test data)" : "Finance")
        .catch(() => {});
    } catch {
      // Not running inside the app window (e.g. a browser preview).
    }
  }, [plaid, sandbox]);
  const [dataVersion, setDataVersion] = useState(0);

  // After unlocking, refresh bank data in the background if it's older than 6 hours.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const st = await api.plaidStatus();
        if (!st.configured) return;
        const items = await api.plaidListItems();
        const stale = items.some(
          (i) =>
            i.environment === st.environment &&
            i.linkedAccountCount > 0 &&
            i.status !== "needs_reauth" &&
            (!i.lastSyncedAt || Date.now() - Date.parse(i.lastSyncedAt) > 6 * 3600_000),
        );
        if (!stale || cancelled) return;
        setSyncNote("Syncing banks…");
        const out = await api.plaidSyncAll();
        if (cancelled) return;
        const failed = out.filter((o) => o.error).length;
        const added = out.reduce((n, o) => n + (o.result?.transactions.added ?? 0), 0);
        setSyncNote(
          failed > 0
            ? `${failed} bank${failed === 1 ? "" : "s"} couldn't sync. See Accounts.`
            : `Banks synced · ${added} new transaction${added === 1 ? "" : "s"}`,
        );
        setDataVersion((v) => v + 1);
      } catch {
        if (!cancelled) setSyncNote(undefined);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="flex h-full">
      <aside className="flex w-60 shrink-0 flex-col border-r border-zinc-200 bg-white px-3 py-4 dark:border-zinc-800 dark:bg-zinc-900">
        <div className="mb-6 flex items-center gap-2 px-2">
          <Logo size={28} />
          <span className="font-semibold">Finance</span>
          {plaid ? (
            <span
              className={`ml-auto rounded px-1.5 py-0.5 text-[10px] font-semibold ${
                sandbox
                  ? "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
                  : "bg-brand-50 text-brand-700 dark:bg-brand-900/40 dark:text-brand-500"
              }`}
              title={sandbox ? "Plaid Sandbox: test banks and fake data" : "Plaid Production: your real banks"}
            >
              {sandbox ? "SANDBOX" : "LIVE"}
            </span>
          ) : null}
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
          {syncNote ? (
            <button className="block px-3 text-left text-xs text-zinc-500 hover:underline" onClick={() => setSection("accounts")}>
              {syncNote}
            </button>
          ) : null}
          <label className="flex items-center justify-between px-3 text-sm text-zinc-600 dark:text-zinc-400">
            Hide balances
            <input type="checkbox" checked={hideBalances} onChange={(e) => setHideBalances(e.target.checked)} />
          </label>
          <Button variant="secondary" className="w-full" onClick={onLock}>
            Lock now
          </Button>
        </div>
      </aside>
      <main className="flex-1 overflow-y-auto">
        {sandbox && plaid ? (
          <div className="sticky top-0 z-10 flex flex-wrap items-center gap-3 border-b border-amber-200 bg-amber-100 px-8 py-2 text-sm text-amber-900 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-100">
            <span className="font-medium">Sandbox mode</span>
            <span className="text-xs">
              You're looking at test data only. Your real accounts, balances and totals come back in Production.
            </span>
            <span className="ml-auto flex items-center gap-2">
              <EnvironmentSwitch
                environment={plaid.environment}
                size="small"
                onChanged={() => {
                  announceEnvironmentChange();
                  reloadPlaid();
                  setDataVersion((v) => v + 1);
                }}
              />
            </span>
          </div>
        ) : null}
        <div className="p-8">
          <PrivacyContext.Provider value={hideBalances}>
            {section === "dashboard" && <Dashboard key={dataVersion} onNavigate={setSection} />}
            {section === "settings" && <SettingsPage />}
            {section === "accounts" && (
              <AccountsPage key={dataVersion} onOpenSettings={() => setSection("settings")} />
            )}
            {section === "transactions" && <TransactionsPage key={dataVersion} />}
            {section === "budgets" && <BudgetsPage key={dataVersion} />}
            {section === "goals" && <GoalsPage key={dataVersion} />}
            {section === "calculators" && (
              <Placeholder
                title="Calculators"
                phase={4}
                detail="Retirement, Monte Carlo, savings goal, home affordability, rent vs buy, and debt payoff."
              />
            )}
          </PrivacyContext.Provider>
        </div>
      </main>
    </div>
  );
}
