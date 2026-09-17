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
import type { Account, HoldingView } from "../lib/types";

export function AccountsPage({ onDataChanged }: { onDataChanged?: () => void }) {
  const [showArchived, setShowArchived] = useState(false);
  const [adding, setAdding] = useState(false);
  const [openId, setOpenId] = useState<number>();
  const accounts = useAsync(() => api.listAccounts(true), []);
  const nw = useAsync(() => api.netWorth(), [accounts.data]);
  const plaid = useAsync(() => api.plaidStatus(), []);
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

      {plaid.data && items.data ? <Connections status={plaid.data} items={items.data} onChanged={reloadAll} /> : null}

      {accounts.error ? <p className="text-sm text-red-600">{accounts.error}</p> : null}

      {accounts.data && list.length === 0 ? (
        <Card>
          <p className="font-medium">No accounts yet</p>
          <p className="mt-1 text-sm text-zinc-500">
            Connect a bank above to add accounts automatically, or add your HYSA, checking, brokerage, 401(k), Roth IRA,
            cards and loans by hand.
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
