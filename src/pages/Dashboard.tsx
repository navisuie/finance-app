import { Money } from "../components/privacy";
import { Button, Card } from "../components/ui";
import { useAsync } from "../hooks/useAsync";
import { api } from "../lib/api";
import { formatDate, monthRange } from "../lib/money";

export function Dashboard({ onNavigate }: { onNavigate: (section: "accounts" | "transactions") => void }) {
  const { from, to } = monthRange();
  const nw = useAsync(() => api.netWorth(), []);
  const accounts = useAsync(() => api.listAccounts(false), []);
  const month = useAsync(() => api.periodSummary(from, to), [from, to]);
  const recent = useAsync(() => api.queryTransactions({ limit: 10 }), []);
  const uncategorized = useAsync(() => api.queryTransactions({ categoryId: 0, limit: 1 }), []);

  const monthName = new Date().toLocaleDateString("en-US", { month: "long" });
  const savingsRate =
    month.data && month.data.incomeCents > 0
      ? Math.round(((month.data.incomeCents - month.data.spendingCents) / month.data.incomeCents) * 100)
      : null;
  const noAccounts = accounts.data && accounts.data.length === 0;
  const stale = (accounts.data ?? []).filter((a) => {
    if (!a.balanceAsOf) return true;
    const days = (Date.now() - new Date(a.balanceAsOf).getTime()) / 86_400_000;
    return days > 35;
  });

  return (
    <div className="max-w-5xl space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Dashboard</h1>
        <p className="text-sm text-zinc-500">Your vault is unlocked and encrypted on this Mac.</p>
      </div>

      {noAccounts ? (
        <Card>
          <p className="font-medium">Start by adding your accounts</p>
          <p className="mt-1 text-sm text-zinc-500">Add each account with its current balance to see your net worth.</p>
          <Button className="mt-4" onClick={() => onNavigate("accounts")}>
            Go to Accounts
          </Button>
        </Card>
      ) : null}

      <div className="grid grid-cols-2 gap-4">
        <Card title="Net worth">
          <Money cents={nw.data?.netCents} className="text-3xl font-semibold" />
          <p className="mt-1 text-xs text-zinc-500">
            Assets <Money cents={nw.data?.assetsCents} /> · Debts <Money cents={nw.data?.liabilitiesCents} />
          </p>
        </Card>
        <Card title={`Spending in ${monthName}`}>
          <Money cents={month.data?.spendingCents} className="text-3xl font-semibold" />
          <p className="mt-1 text-xs text-zinc-500">
            Income <Money cents={month.data?.incomeCents} /> · Budgets arrive in phase 3
          </p>
        </Card>
        <Card title={`Savings rate in ${monthName}`}>
          <p className="text-3xl font-semibold tabular-nums">{savingsRate === null ? "—" : `${savingsRate}%`}</p>
          <p className="mt-1 text-xs text-zinc-500">
            {savingsRate === null ? "Categorize a paycheck as Income to see this." : "Share of income you didn't spend. Transfers are excluded."}
          </p>
        </Card>
        <Card title="Needs attention">
          <ul className="space-y-1 text-sm">
            {uncategorized.data && uncategorized.data.total > 0 ? (
              <li>
                <button className="text-left hover:underline" onClick={() => onNavigate("transactions")}>
                  {uncategorized.data.total} uncategorized transaction{uncategorized.data.total === 1 ? "" : "s"}
                </button>
              </li>
            ) : null}
            {stale.length > 0 ? (
              <li>
                <button className="text-left hover:underline" onClick={() => onNavigate("accounts")}>
                  {stale.length} account{stale.length === 1 ? "" : "s"} with a balance older than a month
                </button>
              </li>
            ) : null}
            {uncategorized.data?.total === 0 && stale.length === 0 ? <li className="text-zinc-500">All caught up.</li> : null}
          </ul>
        </Card>
      </div>

      <Card title="Recent transactions">
        {recent.data && recent.data.items.length > 0 ? (
          <table className="w-full text-sm">
            <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
              {recent.data.items.map((t) => (
                <tr key={t.id}>
                  <td className="py-1.5 pr-3 whitespace-nowrap text-zinc-500">{formatDate(t.postedOn)}</td>
                  <td className="py-1.5 pr-3">{t.payee}</td>
                  <td className="py-1.5 pr-3 text-zinc-500">{t.categoryName ?? "Uncategorized"}</td>
                  <td className="py-1.5 text-right">
                    <Money cents={t.amountCents} colored />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="text-sm text-zinc-500">No transactions yet.</p>
        )}
      </Card>
    </div>
  );
}
