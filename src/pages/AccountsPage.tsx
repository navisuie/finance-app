import { useCallback, useState } from "react";
import { AccountForm, BalanceForm } from "../components/AccountForm";
import { Connections } from "../components/Connections";
import { useConfirm } from "../components/Confirm";
import { ImportWizard } from "../components/ImportWizard";
import { Money } from "../components/privacy";
import { TransactionList } from "../components/TransactionList";
import { Button, Card } from "../components/ui";
import { useAsync } from "../hooks/useAsync";
import { GROUPS, KIND_LABELS } from "../lib/accountKinds";
import { api, errorMessage } from "../lib/api";
import { formatDate } from "../lib/money";
import { formatCents } from "../lib/money";
import type { Account, HoldingView, Projection } from "../lib/types";

export function AccountsPage({
  onDataChanged,
  onOpenSettings,
}: {
  onDataChanged?: () => void;
  onOpenSettings?: () => void;
}) {
  const [showArchived, setShowArchived] = useState(false);
  const [adding, setAdding] = useState(false);
  const [openId, setOpenId] = useState<number>();
  const plaid = useAsync(() => api.plaidStatus(), []);
  const sandbox = plaid.data?.environment === "sandbox";
  // The backend scopes this to the active environment: test accounts in
  // Sandbox, real accounts in Production.
  const accounts = useAsync(() => api.listAccounts(true), [plaid.data?.environment]);
  const nw = useAsync(() => api.netWorth(), [accounts.data]);

  const items = useAsync(() => api.plaidListItems(), []);
  const reloadAll = useCallback(() => {
    accounts.reload();
    items.reload();
    onDataChanged?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const list = (accounts.data ?? []).filter((a) => showArchived || !a.archived);

  if (openId !== undefined) {
    return (
      <AccountDetail
        id={openId}
        onBack={() => {
          setOpenId(undefined);
          accounts.reload();
        }}
        onChanged={() => {
          accounts.reload();
          onDataChanged?.();
        }}
      />
    );
  }

  return (
    <div className="max-w-5xl space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Accounts</h1>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-zinc-500">
            <input type="checkbox" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} />
            Show archived
          </label>
          <Button onClick={() => setAdding(true)}>Add account</Button>
        </div>
      </div>

      {nw.data ? (
        <div className="grid grid-cols-3 gap-4">
          <Card title="Net worth">
            <Money cents={nw.data.netCents} className="text-2xl font-semibold" />
          </Card>
          <Card title="Assets">
            <Money cents={nw.data.assetsCents} className="text-2xl font-semibold" />
          </Card>
          <Card title="Debts">
            <Money cents={nw.data.liabilitiesCents} className="text-2xl font-semibold" />
          </Card>
        </div>
      ) : null}

      {plaid.data && items.data ? (
        <Connections
          status={plaid.data}
          items={items.data}
          onChanged={reloadAll}
          onEnvironmentChanged={() => {
            plaid.reload();
            items.reload();
          }}
          onOpenSettings={onOpenSettings}
        />
      ) : null}

      {accounts.error ? <p className="text-sm text-red-600">{accounts.error}</p> : null}

      {accounts.data && list.length === 0 ? (
        <Card>
          <p className="font-medium">{sandbox ? "No test accounts yet" : "No accounts yet"}</p>
          <p className="mt-1 text-sm text-zinc-500">
            {sandbox
              ? "Sandbox is empty. Connect a test bank above to try syncing; your real accounts are waiting in Production."
              : "Connect a bank above to add accounts automatically, or add your HYSA, checking, brokerage, 401(k), Roth IRA, cards and loans by hand."}
          </p>
          <Button className="mt-4" onClick={() => setAdding(true)}>
            Add your first account
          </Button>
        </Card>
      ) : null}

      {GROUPS.map((g) => {
        const inGroup = list.filter((a) => g.kinds.includes(a.kind));
        if (inGroup.length === 0) return null;
        const total = inGroup.filter((a) => !a.archived).reduce((s, a) => s + (a.balanceCents ?? 0), 0);
        return (
          <section key={g.title} className="space-y-2">
            <div className="flex items-baseline justify-between px-1">
              <h2 className="text-sm font-semibold text-zinc-500">{g.title}</h2>
              <Money cents={total} className="text-sm font-medium text-zinc-500" />
            </div>
            <div className="grid grid-cols-2 gap-3">
              {inGroup.map((a) => (
                <AccountCard key={a.id} account={a} onOpen={() => setOpenId(a.id)} />
              ))}
            </div>
          </section>
        );
      })}

      {adding ? (
        <AccountForm
          onClose={() => setAdding(false)}
          onSaved={() => {
            setAdding(false);
            accounts.reload();
            onDataChanged?.();
          }}
        />
      ) : null}
    </div>
  );
}

function AccountCard({ account: a, onOpen }: { account: Account; onOpen: () => void }) {
  return (
    <button
      onClick={onOpen}
      className={`rounded-xl bg-white p-4 text-left shadow-sm ring-1 ring-zinc-200 transition hover:ring-brand-500 dark:bg-zinc-900 dark:ring-zinc-800 ${
        a.archived ? "opacity-60" : ""
      }`}
    >
      <div className="flex items-start justify-between gap-2">
        <div>
          <p className="font-medium">
            {a.name}
            {a.linked ? (
              <span className="ml-2 rounded bg-brand-50 px-1.5 py-0.5 align-middle text-[10px] font-medium text-brand-700 dark:bg-brand-900/40 dark:text-brand-500">
                SYNCED
              </span>
            ) : null}

          </p>
          <p className="text-xs text-zinc-500">
            {KIND_LABELS[a.kind]}
            {a.institution ? ` · ${a.institution}` : ""}
            {a.last4 ? ` · ••${a.last4}` : ""}
            {a.archived ? " · Archived" : ""}
          </p>
        </div>
        <Money cents={a.balanceCents} className="text-lg font-semibold" />
      </div>
      <p className="mt-2 text-xs text-zinc-400">
        {a.balanceAsOf ? `Balance as of ${formatDate(a.balanceAsOf)}` : "No balance entered"}
        {` · ${a.transactionCount} transactions`}
      </p>
    </button>
  );
}

function AccountDetail({ id, onBack, onChanged }: { id: number; onBack: () => void; onChanged: () => void }) {
  const account = useAsync(() => api.getAccount(id), [id]);
  const history = useAsync(() => api.balanceHistory(id), [id]);
  // Scoped to the active environment, so the account picker in the
  // transaction list never mixes test accounts into Production.
  const all = useAsync(() => api.listAccounts(true), [id]);
  const categories = useAsync(() => api.listCategories(), []);
  const holdings = useAsync(() => api.listHoldings(id), [id]);
  const [editing, setEditing] = useState(false);
  const [balanceOpen, setBalanceOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState<string>();
  const [confirm, confirmDialog] = useConfirm();

  const a = account.data;
  const refresh = () => {
    account.reload();
    history.reload();
    holdings.reload();
    all.reload();
    setRefreshKey((k) => k + 1);
    onChanged();
  };

  if (!a) {
    return (
      <div className="space-y-4">
        <Button variant="ghost" onClick={onBack}>
          ← Accounts
        </Button>
        {account.error ? <p className="text-sm text-red-600">{account.error}</p> : null}
      </div>
    );
  }

  const archive = async () => {
    try {
      await api.setAccountArchived(a.id, !a.archived);
      refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const remove = async () => {
    const ok = await confirm({
      title: `Delete ${a.name}?`,
      body: (
        <>
          This permanently deletes the account, its balance history and its {a.transactionCount} transactions. To keep
          history but hide the account, archive it instead.
        </>
      ),
      confirmLabel: "Delete account",
    });
    if (!ok) return;
    try {
      await api.deleteAccount(a.id);
      onChanged();
      onBack();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const removeSnapshot = async (snapshotId: number) => {
    try {
      await api.deleteBalance(snapshotId);
      refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="max-w-5xl space-y-6">
      <Button variant="ghost" onClick={onBack} className="-ml-3">
        ← Accounts
      </Button>
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">{a.name}</h1>
          <p className="text-sm text-zinc-500">
            {KIND_LABELS[a.kind]}
            {a.institution ? ` · ${a.institution}` : ""}
            {a.last4 ? ` · ending ${a.last4}` : ""}
            {a.archived ? " · Archived" : ""}
          </p>
          {a.linked ? (
            <p className="mt-1 text-xs text-brand-700 dark:text-brand-500">
              Synced through Plaid. Balances update each time you sync; manual edits to the balance are replaced on the next sync.
            </p>
          ) : null}
        </div>
        <div className="text-right">
          <Money cents={a.balanceCents} className="text-3xl font-semibold" />
          <p className="text-xs text-zinc-500">
            {a.isAsset ? "Balance" : "Owed"}
            {a.balanceAsOf ? ` as of ${formatDate(a.balanceAsOf)}` : ""}
          </p>
        </div>
      </div>

      <div className="flex flex-wrap gap-2">
        <Button onClick={() => setBalanceOpen(true)}>Update balance</Button>
        <Button variant="secondary" onClick={() => setImportOpen(true)} disabled={a.archived}>
          Import CSV
        </Button>
        <Button variant="secondary" onClick={() => setEditing(true)}>
          Edit
        </Button>
        <Button variant="secondary" onClick={archive}>
          {a.archived ? "Unarchive" : "Archive"}
        </Button>
        <Button variant="ghost" className="text-red-600" onClick={remove}>
          Delete
        </Button>
      </div>
      {error ? <p className="text-sm text-red-600">{error}</p> : null}

      {!a.isAsset ? <DebtCard account={a} onChanged={refresh} /> : null}

      {holdings.data && holdings.data.length > 0 ? <HoldingsTable holdings={holdings.data} /> : null}

      <div className="grid grid-cols-[1fr_18rem] gap-6">
        <div className="space-y-2">
          <h2 className="text-sm font-semibold text-zinc-500">Transactions</h2>
          {all.data && categories.data ? (
            <TransactionList
              accounts={all.data}
              categories={categories.data}
              fixedAccountId={a.id}
              refreshKey={refreshKey}
              onChanged={refresh}
            />
          ) : null}
        </div>
        <Card title="Balance history">
          {history.data && history.data.length > 0 ? (
            <ul className="divide-y divide-zinc-100 text-sm dark:divide-zinc-800">
              {history.data.map((h) => (
                <li key={h.id} className="group flex items-center justify-between py-1.5">
                  <span className="text-zinc-500">{formatDate(h.asOf)}</span>
                  <span className="flex items-center gap-2">
                    <Money cents={h.amountCents} />
                    <button
                      onClick={() => removeSnapshot(h.id)}
                      className="invisible text-xs text-zinc-400 group-hover:visible hover:text-red-600"
                      aria-label={`Remove balance from ${h.asOf}`}
                    >
                      ✕
                    </button>
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-sm text-zinc-500">No balances yet. Update the balance each month to track net worth over time.</p>
          )}
        </Card>
      </div>

      {editing ? (
        <AccountForm
          account={a}
          onClose={() => setEditing(false)}
          onSaved={() => {
            setEditing(false);
            refresh();
          }}
        />
      ) : null}
      {balanceOpen ? (
        <BalanceForm
          account={a}
          onClose={() => setBalanceOpen(false)}
          onSaved={() => {
            setBalanceOpen(false);
            refresh();
          }}
        />
      ) : null}
      {importOpen && all.data && categories.data ? (
        <ImportWizard
          accounts={all.data}
          categories={categories.data}
          defaultAccountId={a.id}
          onClose={() => setImportOpen(false)}
          onImported={refresh}
        />
      ) : null}
      {confirmDialog}
    </div>
  );
}

function HoldingsTable({ holdings }: { holdings: HoldingView[] }) {
  const total = holdings.reduce((s, h) => s + h.valueCents, 0);
  return (
    <Card title={`Holdings · as of ${formatDate(holdings[0].asOf)}`}>
      <table className="w-full text-sm">
        <thead className="text-left text-xs text-zinc-500">
          <tr>
            <th className="py-1 font-medium">Security</th>
            <th className="py-1 text-right font-medium">Shares</th>
            <th className="py-1 text-right font-medium">Value</th>
            <th className="py-1 text-right font-medium">Gain / loss</th>
            <th className="py-1 text-right font-medium">Weight</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
          {holdings.map((h, i) => (
            <tr key={i}>
              <td className="py-1.5">
                <span className="font-medium">{h.ticker ?? "—"}</span> <span className="text-zinc-500">{h.name}</span>
              </td>
              <td className="py-1.5 text-right tabular-nums">{h.quantity.toLocaleString("en-US", { maximumFractionDigits: 4 })}</td>
              <td className="py-1.5 text-right">
                <Money cents={h.valueCents} />
              </td>
              <td className="py-1.5 text-right">
                {h.costBasisCents !== null ? <Money cents={h.valueCents - h.costBasisCents} signed colored /> : "—"}
              </td>
              <td className="py-1.5 text-right tabular-nums text-zinc-500">
                {total > 0 ? `${((h.valueCents / total) * 100).toFixed(1)}%` : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

function months(n: number): string {
  const years = Math.floor(n / 12);
  const rest = n % 12;
  if (years === 0) return `${rest} month${rest === 1 ? "" : "s"}`;
  if (rest === 0) return `${years} year${years === 1 ? "" : "s"}`;
  return `${years} yr ${rest} mo`;
}

function ProjectionLines({ projection }: { projection: Projection }) {
  switch (projection.state) {
    case "clear":
      return <p className="text-sm text-brand-700 dark:text-brand-500">Nothing owed. Nicely done.</p>;
    case "incomplete":
      return (
        <p className="text-sm text-zinc-500">
          Add the interest rate and your monthly payment with <span className="font-medium">Edit</span> to see when this
          is paid off.
        </p>
      );
    case "neverPaidOff":
      return (
        <p className="text-sm text-amber-700 dark:text-amber-300">
          That payment only covers the interest ({formatCents(projection.monthlyInterestCents)} a month). Pay more than{" "}
          {formatCents(projection.minimumToMakeProgressCents)} to start reducing the balance.
        </p>
      );
    case "paid":
      return (
        <dl className="grid grid-cols-3 gap-4 text-sm">
          <div>
            <dt className="text-xs text-zinc-500">Paid off</dt>
            <dd className="font-medium">{formatDate(projection.payoffOn)}</dd>
          </div>
          <div>
            <dt className="text-xs text-zinc-500">Payments left</dt>
            <dd className="font-medium">
              {projection.months} ({months(projection.months)})
            </dd>
          </div>
          <div>
            <dt className="text-xs text-zinc-500">Interest from here</dt>
            <dd className="font-medium">
              <Money cents={projection.totalInterestCents} />
            </dd>
          </div>
        </dl>
      );
  }
}

/** Payoff projection and the payments you've linked to a debt. */
function DebtCard({ account, onChanged }: { account: Account; onChanged: () => void }) {
  const summary = useAsync(() => api.debtSummary(account.id), [account.id, account.balanceCents]);
  const [updating, setUpdating] = useState(false);
  const s = summary.data;
  if (!s) return null;

  const rate = s.aprBps !== null ? `${(s.aprBps / 100).toFixed(2)}%` : "not set";
  const payment = s.monthlyPaymentCents !== null ? formatCents(s.monthlyPaymentCents) : "not set";

  return (
    <Card title="Paying this off">
      <div className="space-y-4">
        <p className="text-sm text-zinc-500">
          Rate {rate} · Monthly payment {payment}
        </p>
        <ProjectionLines projection={s.projection} />

        {s.payments.count > 0 ? (
          <div className="flex flex-wrap items-center gap-3 rounded-lg bg-zinc-50 p-3 text-sm dark:bg-zinc-800/60">
            <div className="min-w-0 flex-1">
              <p>
                {s.payments.count} payment{s.payments.count === 1 ? "" : "s"} totalling{" "}
                <Money cents={s.payments.totalCents} /> linked
                {s.payments.since ? ` since your balance on ${formatDate(s.payments.since)}` : ""}.
              </p>
              {s.suggestedBalanceCents !== null ? (
                <p className="text-xs text-zinc-500">
                  That works out to about <Money cents={s.suggestedBalanceCents} /> owed, including an estimated{" "}
                  <Money cents={s.estimatedInterestCents} /> of interest. Your balance stays as it is until you apply it.
                </p>
              ) : null}
            </div>
            <Button variant="secondary" className="py-1" onClick={() => setUpdating(true)}>
              Update balance
            </Button>
          </div>
        ) : (
          <p className="text-xs text-zinc-500">
            Linked payments show up here. On the transaction, set "Paid to another account" to this one.
          </p>
        )}

        {s.recentPayments.length > 0 ? (
          <div>
            <p className="mb-1 text-xs font-medium text-zinc-500">Paid from</p>
            <ul className="divide-y divide-zinc-100 text-sm dark:divide-zinc-800">
              {s.recentPayments.map((p) => (
                <li key={p.id} className="flex items-baseline justify-between gap-3 py-1.5">
                  <span className="min-w-0">
                    <span className="text-zinc-500">{formatDate(p.postedOn)}</span>{" "}
                    <span className="font-medium">{p.fromAccountName}</span>
                    <span className="block truncate text-xs text-zinc-500">{p.payee}</span>
                  </span>
                  <span className="shrink-0 tabular-nums">
                    <Money cents={p.amountCents} />
                  </span>
                </li>
              ))}
            </ul>
            <p className="mt-1 text-xs text-zinc-500">
              The next payment with the same description from the same account gets tagged automatically.
            </p>
          </div>
        ) : null}
      </div>

      {updating ? (
        <BalanceForm
          account={account}
          suggestedCents={s.suggestedBalanceCents ?? undefined}
          note="Pre-filled from the payments you linked plus estimated interest. Change it to match your statement."
          onClose={() => setUpdating(false)}
          onSaved={() => {
            setUpdating(false);
            summary.reload();
            onChanged();
          }}
        />
      ) : null}
    </Card>
  );
}
