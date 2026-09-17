import { useEffect, useMemo, useState } from "react";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import { formatDate } from "../lib/money";
import type { Account, Category, Transaction, TransactionFilter } from "../lib/types";
import { CategorySelect } from "./CategorySelect";
import { useConfirm } from "./Confirm";
import { Money } from "./privacy";
import { controlClass } from "./Select";
import { TransactionForm } from "./TransactionForm";
import { Button } from "./ui";

const PAGE_SIZE = 100;
const inputClass =
  "rounded-lg border-0 bg-white px-2.5 py-1.5 text-sm ring-1 ring-zinc-300 outline-none focus:ring-2 focus:ring-brand-500 dark:bg-zinc-900 dark:ring-zinc-700";

export function TransactionList({
  accounts,
  categories,
  fixedAccountId,
  refreshKey = 0,
  onChanged,
}: {
  accounts: Account[];
  categories: Category[];
  fixedAccountId?: number;
  refreshKey?: number;
  onChanged?: () => void;
}) {
  const [accountId, setAccountId] = useState<number | undefined>(fixedAccountId);
  const [categoryId, setCategoryId] = useState<number | undefined>();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [editing, setEditing] = useState<Transaction | "new">();
  const [message, setMessage] = useState<string>();
  const [confirm, confirmDialog] = useConfirm();

  useEffect(() => {
    const t = setTimeout(() => setDebounced(search), 250);
    return () => clearTimeout(t);
  }, [search]);

  const filter: TransactionFilter = useMemo(
    () => ({
      accountId: fixedAccountId ?? accountId,
      categoryId,
      from: from || undefined,
      to: to || undefined,
      search: debounced || undefined,
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
    }),
    [fixedAccountId, accountId, categoryId, from, to, debounced, page],
  );

  useEffect(() => {
    setPage(0);
    setSelected(new Set());
  }, [accountId, categoryId, from, to, debounced]);

  const { data, error, loading, reload } = useAsync(() => api.queryTransactions(filter), [filter, refreshKey]);
  const items = data?.items ?? [];
  const pages = data ? Math.max(1, Math.ceil(data.total / PAGE_SIZE)) : 1;
  const allSelected = items.length > 0 && items.every((t) => selected.has(t.id));

  const changed = () => {
    setSelected(new Set());
    reload();
    onChanged?.();
  };

  const toggle = (id: number) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const bulkCategory = async (cid: number | null) => {
    try {
      const n = await api.setTransactionsCategory([...selected], cid);
      setMessage(`Updated ${n} transaction${n === 1 ? "" : "s"}.`);
      changed();
    } catch (e) {
      setMessage(errorMessage(e));
    }
  };

  const bulkDelete = async () => {
    const ok = await confirm({
      title: `Delete ${selected.size} transaction${selected.size === 1 ? "" : "s"}?`,
      body: "This can't be undone. Re-importing the same CSV would bring imported rows back.",
    });
    if (!ok) return;
    try {
      const n = await api.deleteTransactions([...selected]);
      setMessage(`Deleted ${n} transaction${n === 1 ? "" : "s"}.`);
      changed();
    } catch (e) {
      setMessage(errorMessage(e));
    }
  };

  const setOneCategory = async (t: Transaction, cid: number | null) => {
    try {
      await api.setTransactionsCategory([t.id], cid);
      changed();
    } catch (e) {
      setMessage(errorMessage(e));
    }
  };

  const openAccounts = accounts.filter((a) => !a.archived);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="search"
          placeholder="Search payee or notes"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className={`${inputClass} w-56`}
          aria-label="Search"
        />
        {fixedAccountId === undefined ? (
          <select
            className={`${controlClass} w-auto`}
            value={accountId ?? ""}
            onChange={(e) => setAccountId(e.target.value ? Number(e.target.value) : undefined)}
            aria-label="Account"
          >
            <option value="">All accounts</option>
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
                {a.archived ? " (archived)" : ""}
              </option>
            ))}
          </select>
        ) : null}
        <select
          className={`${controlClass} w-auto`}
          value={categoryId ?? ""}
          onChange={(e) => setCategoryId(e.target.value === "" ? undefined : Number(e.target.value))}
          aria-label="Category"
        >
          <option value="">All categories</option>
          <option value="0">Uncategorized</option>
          {categories.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
        <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} className={inputClass} aria-label="From date" />
        <span className="text-sm text-zinc-400">to</span>
        <input type="date" value={to} onChange={(e) => setTo(e.target.value)} className={inputClass} aria-label="To date" />
        <div className="flex-1" />
        <Button onClick={() => setEditing("new")} disabled={openAccounts.length === 0}>
          Add transaction
        </Button>
      </div>

      {data ? (
        <div className="flex gap-6 text-sm text-zinc-500">
          <span>{data.total.toLocaleString()} transactions</span>
          <span>
            In <Money cents={data.inflowCents} className="text-zinc-900 dark:text-zinc-100" />
          </span>
          <span>
            Out <Money cents={data.outflowCents} className="text-zinc-900 dark:text-zinc-100" />
          </span>
        </div>
      ) : null}

      {selected.size > 0 ? (
        <div className="flex items-center gap-3 rounded-lg bg-brand-50 px-3 py-2 text-sm dark:bg-brand-900/30">
          <span className="font-medium">{selected.size} selected</span>
          <span className="text-zinc-500">Set category:</span>
          <CategorySelect
            categories={categories}
            value={null}
            onChange={bulkCategory}
            noneLabel="Choose… (or clear)"
            className={`${controlClass} w-auto`}
            aria-label="Set category for selected"
          />
          <Button variant="danger" className="ml-auto py-1" onClick={bulkDelete}>
            Delete
          </Button>
        </div>
      ) : null}
      {message ? <p className="text-sm text-zinc-500">{message}</p> : null}
      {error ? <p className="text-sm text-red-600">{error}</p> : null}

      <div className="overflow-hidden rounded-xl bg-white ring-1 ring-zinc-200 dark:bg-zinc-900 dark:ring-zinc-800">
        <table className="w-full text-sm">
          <thead className="bg-zinc-50 text-left text-xs text-zinc-500 dark:bg-zinc-800/50">
            <tr>
              <th className="w-8 px-3 py-2">
                <input
                  type="checkbox"
                  checked={allSelected}
                  onChange={() => setSelected(allSelected ? new Set() : new Set(items.map((t) => t.id)))}
                  aria-label="Select all on this page"
                />
              </th>
              <th className="px-3 py-2 font-medium">Date</th>
              <th className="px-3 py-2 font-medium">Payee</th>
              {fixedAccountId === undefined ? <th className="px-3 py-2 font-medium">Account</th> : null}
              <th className="px-3 py-2 font-medium">Category</th>
              <th className="px-3 py-2 text-right font-medium">Amount</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {items.map((t) => (
              <tr key={t.id} className="hover:bg-zinc-50 dark:hover:bg-zinc-800/40">
                <td className="px-3 py-2">
                  <input type="checkbox" checked={selected.has(t.id)} onChange={() => toggle(t.id)} aria-label={`Select ${t.payee}`} />
                </td>
                <td className="px-3 py-2 whitespace-nowrap text-zinc-500">{formatDate(t.postedOn)}</td>
                <td className="px-3 py-2">
                  <button className="selectable text-left hover:underline" onClick={() => setEditing(t)}>
                    {t.payee}
                  </button>
                  {t.notes ? <div className="text-xs text-zinc-400">{t.notes}</div> : null}
                </td>
                {fixedAccountId === undefined ? <td className="px-3 py-2 text-zinc-500">{t.accountName}</td> : null}
                <td className="px-3 py-1.5">
                  <CategorySelect
                    categories={categories}
                    value={t.categoryId}
                    onChange={(cid) => setOneCategory(t, cid)}
                    className={`w-40 rounded-md border-0 bg-transparent px-1.5 py-1 text-sm hover:ring-1 hover:ring-zinc-300 focus:ring-2 focus:ring-brand-500 ${
                      t.categoryId === null ? "text-amber-600" : ""
                    }`}
                    aria-label={`Category for ${t.payee}`}
                  />
                </td>
                <td className="px-3 py-2 text-right whitespace-nowrap">
                  <Money cents={t.amountCents} colored />
                </td>
              </tr>
            ))}
            {!loading && items.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-3 py-10 text-center text-zinc-500">
                  No transactions match. Add one or import a CSV.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>

      {pages > 1 ? (
        <div className="flex items-center justify-end gap-2 text-sm">
          <Button variant="secondary" className="py-1" disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
            Previous
          </Button>
          <span className="text-zinc-500">
            Page {page + 1} of {pages}
          </span>
          <Button variant="secondary" className="py-1" disabled={page + 1 >= pages} onClick={() => setPage((p) => p + 1)}>
            Next
          </Button>
        </div>
      ) : null}

      {confirmDialog}
      {editing ? (
        <TransactionForm
          transaction={editing === "new" ? undefined : editing}
          accounts={editing === "new" ? openAccounts : accounts}
          categories={categories}
          defaultAccountId={fixedAccountId ?? accountId}
          onClose={() => setEditing(undefined)}
          onSaved={() => {
            setEditing(undefined);
            changed();
          }}
        />
      ) : null}
    </div>
  );
}
