import { useEffect, useState } from "react";
import { BudgetBuilder } from "../components/BudgetBuilder";
import { Meter } from "../components/Meter";
import { Money } from "../components/privacy";
import { controlClass } from "../components/Select";
import { Button, Card } from "../components/ui";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import { centsToInput, formatCents, parseMoney, todayISO } from "../lib/money";
import type { BudgetMonth, BudgetRow } from "../lib/types";

function thisMonth(): string {
  return todayISO().slice(0, 7);
}

function shiftMonth(month: string, by: number): string {
  const [y, m] = month.split("-").map(Number);
  const total = y * 12 + (m - 1) + by;
  return `${String(Math.floor(total / 12)).padStart(4, "0")}-${String((total % 12) + 1).padStart(2, "0")}`;
}

function monthLabel(month: string): string {
  const [y, m] = month.split("-").map(Number);
  return new Date(y, m - 1, 1).toLocaleDateString("en-US", { month: "long", year: "numeric" });
}

export function BudgetsPage() {
  const [month, setMonth] = useState(thisMonth());
  const [data, setData] = useState<BudgetMonth>();
  const [error, setError] = useState<string>();
  const [building, setBuilding] = useState(false);
  const loaded = useAsync(() => api.budgetMonth(month), [month]);

  useEffect(() => {
    if (loaded.data) setData(loaded.data);
  }, [loaded.data]);

  const save = async (categoryId: number, limitCents: number) => {
    try {
      setData(await api.setBudget(categoryId, month, limitCents));
      setError(undefined);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const copyLast = async () => {
    try {
      setData(await api.copyBudgets(shiftMonth(month, -1), month));
      setError(undefined);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const d = data;
  const budgeted = d?.rows.filter((r) => r.limitCents !== null) ?? [];
  const others = d?.rows.filter((r) => r.limitCents === null) ?? [];
  const leftToSpend = d ? d.totalLimitCents - budgeted.reduce((s, r) => s + r.spentCents, 0) : 0;
  const overCount = budgeted.filter((r) => r.state === "over").length;

  return (
    <div className="max-w-4xl space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold">Budgets</h1>
        <div className="flex items-center gap-2">
          <Button variant="secondary" className="py-1" onClick={() => setMonth(shiftMonth(month, -1))}>
            ←
          </Button>
          <span className="min-w-40 text-center text-sm font-medium">{monthLabel(month)}</span>
          <Button
            variant="secondary"
            className="py-1"
            onClick={() => setMonth(shiftMonth(month, 1))}
            disabled={month >= thisMonth()}
            title={month >= thisMonth() ? "You can't budget past this month yet" : undefined}
          >
            →
          </Button>
          {month !== thisMonth() ? (
            <Button variant="ghost" className="py-1" onClick={() => setMonth(thisMonth())}>
              This month
            </Button>
          ) : null}
        </div>
      </div>

      {error ? <p className="text-sm text-red-600">{error}</p> : null}

      {d ? (
        <Card>
          <div className="flex flex-wrap items-end justify-between gap-4">
            <div>
              <p className="text-xs text-zinc-500">Spent against budgets</p>
              <p className="text-2xl font-semibold">
                <Money cents={budgeted.reduce((s, r) => s + r.spentCents, 0)} />{" "}
                <span className="text-base font-normal text-zinc-500">
                  of <Money cents={d.totalLimitCents} />
                </span>
              </p>
            </div>
            <div className="text-right text-sm">
              <p className={leftToSpend < 0 ? "text-red-600 dark:text-red-400" : ""}>
                {leftToSpend < 0 ? (
                  <>
                    <Money cents={-leftToSpend} /> over
                  </>
                ) : (
                  <>
                    <Money cents={leftToSpend} /> left
                  </>
                )}
              </p>
              <p className="text-xs text-zinc-500">
                Income <Money cents={d.incomeCents} /> · Unbudgeted spending{" "}
                <Money cents={d.unbudgetedSpentCents} />
              </p>
            </div>
          </div>
          <div className="mt-3">
            <Meter
              used={d.totalLimitCents > 0 ? budgeted.reduce((s, r) => s + r.spentCents, 0) / d.totalLimitCents : 0}
              over={leftToSpend < 0}
              label="Total budget used"
            />
          </div>
          {overCount > 0 ? (
            <p className="mt-2 text-sm text-red-600 dark:text-red-400">
              {overCount} categor{overCount === 1 ? "y is" : "ies are"} over budget this month.
            </p>
          ) : null}
        </Card>
      ) : null}

      <div className="flex items-center justify-between px-1">
        <h2 className="text-sm font-semibold text-zinc-500">Categories</h2>
        <div className="flex items-center gap-2">
          <Button variant="secondary" className="py-1" onClick={() => setBuilding(true)}>
            Build a budget
          </Button>
          <Button variant="secondary" className="py-1" onClick={copyLast}>
            Copy {monthLabel(shiftMonth(month, -1)).split(" ")[0]}'s budgets
          </Button>
        </div>
      </div>

      <Card className="divide-y divide-zinc-100 dark:divide-zinc-800">
        {[...budgeted, ...others].map((row) => (
          <BudgetLine key={row.categoryId} row={row} onSave={save} />
        ))}
        {d && d.rows.length === 0 ? (
          <p className="text-sm text-zinc-500">
            No spending categories yet. Categorize a few transactions, then set limits here.
          </p>
        ) : null}
      </Card>

      {building ? (
        <BudgetBuilder
          month={month}
          monthLabel={monthLabel(month)}
          onClose={() => setBuilding(false)}
          onApplied={setData}
        />
      ) : null}
    </div>
  );
}

function BudgetLine({ row, onSave }: { row: BudgetRow; onSave: (categoryId: number, cents: number) => void }) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(row.limitCents !== null ? centsToInput(row.limitCents) : "");

  useEffect(() => {
    setValue(row.limitCents !== null ? centsToInput(row.limitCents) : "");
  }, [row.limitCents]);

  const commit = () => {
    setEditing(false);
    const cents = value.trim() === "" ? 0 : parseMoney(value);
    if (cents === null || cents === row.limitCents) return;
    onSave(row.categoryId, cents);
  };

  const state = row.state;
  return (
    <div className="grid grid-cols-[1fr_auto] items-center gap-3 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0">
        <div className="flex items-baseline justify-between gap-3">
          <p className="font-medium">{row.categoryName}</p>
          <p className="text-sm">
            <Money cents={row.spentCents} />
            {row.limitCents !== null ? (
              <span className="text-zinc-500">
                {" "}
                of <Money cents={row.limitCents} />
              </span>
            ) : null}
          </p>
        </div>
        {row.limitCents !== null ? (
          <div className="mt-1.5 space-y-1">
            <Meter used={row.used ?? 0} over={state === "over"} label={`${row.categoryName} budget`} />
            <p
              className={`text-xs ${
                state === "over"
                  ? "text-red-600 dark:text-red-400"
                  : state === "close"
                    ? "text-amber-700 dark:text-amber-400"
                    : "text-zinc-500"
              }`}
            >
              {state === "over" ? (
                <>
                  Over by <Money cents={-(row.remainingCents ?? 0)} />
                </>
              ) : state === "close" ? (
                <>
                  Close to the limit · <Money cents={row.remainingCents ?? 0} /> left
                </>
              ) : (
                <>
                  <Money cents={row.remainingCents ?? 0} /> left
                </>
              )}
              {row.recentAverageCents > 0 ? ` · ${formatCents(row.recentAverageCents)}/mo recently` : ""}
            </p>
          </div>
        ) : (
          <p className="mt-1 text-xs text-zinc-500">
            No limit
            {row.recentAverageCents > 0 ? ` · ${formatCents(row.recentAverageCents)}/mo recently` : ""}
          </p>
        )}
      </div>
      <div className="flex items-center gap-2">
        {editing ? (
          <input
            className={`${controlClass} w-28 py-1 text-right`}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") commit();
              if (e.key === "Escape") setEditing(false);
            }}
            inputMode="decimal"
            placeholder="0.00"
            aria-label={`Budget for ${row.categoryName}`}
            autoFocus
          />
        ) : (
          <Button variant="secondary" className="py-1" onClick={() => setEditing(true)}>
            {row.limitCents !== null ? "Change" : "Set limit"}
          </Button>
        )}
        {!editing && row.limitCents === null && row.recentAverageCents > 0 ? (
          <Button
            variant="ghost"
            className="py-1 text-xs"
            onClick={() => onSave(row.categoryId, row.recentAverageCents)}
            title="Use your recent average as the limit"
          >
            Use average
          </Button>
        ) : null}
      </div>
    </div>
  );
}
