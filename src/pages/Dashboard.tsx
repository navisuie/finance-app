import { useContext } from "react";
import { Meter } from "../components/Meter";
import { NetWorthChart } from "../components/NetWorthChart";
import { Money, PrivacyContext } from "../components/privacy";
import { Button, Card } from "../components/ui";
import { useAsync } from "../hooks/useAsync";
import { api } from "../lib/api";
import type { BudgetMonth } from "../lib/types";
import { formatCents, formatDate, monthRange } from "../lib/money";

export function Dashboard({
  onNavigate,
}: {
  onNavigate: (section: "accounts" | "transactions" | "budgets" | "goals") => void;
}) {
  const hidden = useContext(PrivacyContext);
  const { from, to } = monthRange();
  const nw = useAsync(() => api.netWorth(), []);
  const accounts = useAsync(() => api.listAccounts(false), []);
  const month = useAsync(() => api.periodSummary(from, to), [from, to]);
  const income = useAsync(() => api.incomeBaseline(from.slice(0, 7)), [from]);
  const recent = useAsync(() => api.queryTransactions({ limit: 10 }), []);
  const uncategorized = useAsync(() => api.queryTransactions({ categoryId: 0, limit: 1 }), []);
  const transfers = useAsync(() => api.listTransferCandidates(), []);
  const history = useAsync(() => api.netWorthHistory(13), []);
  const budgets = useAsync(() => api.budgetMonth(from.slice(0, 7)), [from]);
  const goals = useAsync(() => api.listGoals(), []);

  const monthName = new Date().toLocaleDateString("en-US", { month: "long" });
  // Pay arrives in lumps — a paycheck landing on the 31st counts in the next
  // month — so the rate is measured against what you usually bring in when
  // this month is still behind it.
  const expectedIncome = income.data?.expectedCents ?? month.data?.incomeCents ?? 0;
  const savingsRate =
    month.data && expectedIncome > 0
      ? Math.round(((expectedIncome - month.data.spendingCents) / expectedIncome) * 100)
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
            Income <Money cents={month.data?.incomeCents} />
          </p>
        </Card>
        <Card title={`Savings rate in ${monthName}`}>
          <p
            className={`text-3xl font-semibold tabular-nums ${
              savingsRate !== null && savingsRate < 0 ? "text-red-600 dark:text-red-500" : ""
            }`}
          >
            {savingsRate === null ? "—" : `${savingsRate}%`}
          </p>
          {month.data && savingsRate !== null ? (
            <p className="mt-1 text-xs text-zinc-500">
              {income.data?.estimated ? "Expected income" : "Income"} <Money cents={expectedIncome} /> · spent so far{" "}
              <Money cents={month.data.spendingCents} /> · saved &amp; invested{" "}
              <Money cents={month.data.savedCents} />
            </p>
          ) : null}
          {income.data?.estimated ? (
            <p className="mt-1 text-xs text-zinc-500">
              Estimated from the last {income.data.monthsOfData} month{income.data.monthsOfData === 1 ? "" : "s"} you
              were paid, because only <Money cents={income.data.receivedCents} /> has landed in {monthName} so far.
            </p>
          ) : null}
          {savingsRate === null ? (
            <p className="mt-1 text-xs text-zinc-500">Categorize a paycheck as Income to see this.</p>
          ) : savingsRate < 0 && month.data ? (
            <button
              className="mt-1 text-left text-xs text-amber-700 hover:underline dark:text-amber-500"
              onClick={() => onNavigate("transactions")}
            >
              {month.data.uncategorizedSpendingCents > 0
                ? `${formatCents(month.data.uncategorizedSpendingCents)} of this is uncategorized, so it counts as spending. Money moved to savings or investments only counts as saving once it's tagged as a transfer.`
                : "Spending is above income this month. Check that money moved to savings or investments is tagged as a transfer, and that your pay is categorized as Income."}
            </button>
          ) : (
            <p className="mt-1 text-xs text-zinc-500">
              Share of income you didn't spend. Saving and investing count as saved, not spent.
            </p>
          )}
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
            {transfers.data && transfers.data.length > 0 ? (
              <li>
                <button className="text-left hover:underline" onClick={() => onNavigate("transactions")}>
                  {transfers.data.length} possible transfer{transfers.data.length === 1 ? "" : "s"} to review
                </button>
              </li>
            ) : null}
            {uncategorized.data?.total === 0 && stale.length === 0 && (transfers.data?.length ?? 0) === 0 ? (
              <li className="text-zinc-500">All caught up.</li>
            ) : null}
          </ul>
        </Card>
      </div>

      {history.data ? (
        <Card title="Net worth over the last year">
          <NetWorthChart points={history.data} hidden={hidden} />
        </Card>
      ) : null}

      <div className="grid grid-cols-2 gap-4">
        <Card title={`Budgets in ${monthName}`}>
          {budgets.data && budgets.data.totalLimitCents > 0 ? (
            <BudgetSummary data={budgets.data} onOpen={() => onNavigate("budgets")} />
          ) : (
            <>
              <p className="text-sm text-zinc-500">No limits set for this month.</p>
              <Button className="mt-3" variant="secondary" onClick={() => onNavigate("budgets")}>
                Set budgets
              </Button>
            </>
          )}
        </Card>
        <Card title="Goals">
          {goals.data && goals.data.length > 0 ? (
            <ul className="space-y-3">
              {goals.data.slice(0, 3).map((g) => (
                <li key={g.id}>
                  <div className="flex items-baseline justify-between text-sm">
                    <button className="font-medium hover:underline" onClick={() => onNavigate("goals")}>
                      {g.name}
                    </button>
                    <span className="text-zinc-500">
                      <Money cents={g.currentCents} /> of <Money cents={g.targetCents} />
                    </span>
                  </div>
                  <div className="mt-1">
                    <Meter used={g.progress} label={`${g.name} progress`} />
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <>
              <p className="text-sm text-zinc-500">No goals yet.</p>
              <Button className="mt-3" variant="secondary" onClick={() => onNavigate("goals")}>
                Add a goal
              </Button>
            </>
          )}
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

function BudgetSummary({ data, onOpen }: { data: BudgetMonth; onOpen: () => void }) {
  const budgeted = data.rows.filter((r) => r.limitCents !== null);
  const spent = budgeted.reduce((s, r) => s + r.spentCents, 0);
  const over = budgeted.filter((r) => r.state === "over");
  const left = data.totalLimitCents - spent;
  return (
    <div className="space-y-2">
      <p className="text-2xl font-semibold">
        <Money cents={spent} />{" "}
        <span className="text-base font-normal text-zinc-500">
          of <Money cents={data.totalLimitCents} />
        </span>
      </p>
      <Meter used={data.totalLimitCents > 0 ? spent / data.totalLimitCents : 0} over={left < 0} label="Budget used" />
      <p className="text-xs text-zinc-500">
        {left >= 0 ? (
          <>
            <Money cents={left} /> left this month
          </>
        ) : (
          <>
            <Money cents={-left} /> over this month
          </>
        )}
        {over.length > 0 ? ` · ${over.length} categor${over.length === 1 ? "y" : "ies"} over` : ""}
      </p>
      <button className="text-xs text-brand-700 hover:underline dark:text-brand-500" onClick={onOpen}>
        Open budgets
      </button>
    </div>
  );
}
