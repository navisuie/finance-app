import { useState, type FormEvent } from "react";
import { api, errorMessage } from "../lib/api";
import { centsToInput, parseMoney, todayISO } from "../lib/money";
import type { Account, Category, Transaction } from "../lib/types";
import { CategorySelect } from "./CategorySelect";
import { Modal } from "./Modal";
import { Select, selectClass } from "./Select";
import { Button, Field } from "./ui";

export function TransactionForm({
  transaction,
  accounts,
  categories,
  defaultAccountId,
  onClose,
  onSaved,
}: {
  transaction?: Transaction;
  accounts: Account[];
  categories: Category[];
  defaultAccountId?: number;
  onClose: () => void;
  onSaved: () => void;
}) {
  const editing = !!transaction;
  const [accountId, setAccountId] = useState<number>(
    transaction?.accountId ?? defaultAccountId ?? accounts[0]?.id ?? 0,
  );
  const [postedOn, setPostedOn] = useState(transaction?.postedOn ?? todayISO());
  const [direction, setDirection] = useState<"out" | "in">(
    transaction && transaction.amountCents > 0 ? "in" : "out",
  );
  const [amount, setAmount] = useState(transaction ? centsToInput(Math.abs(transaction.amountCents)) : "");
  const [payee, setPayee] = useState(transaction?.payee ?? "");
  const [categoryId, setCategoryId] = useState<number | null>(transaction?.categoryId ?? null);
  const [categoryTouched, setCategoryTouched] = useState(editing);
  const [notes, setNotes] = useState(transaction?.notes ?? "");
  const [transferTo, setTransferTo] = useState<number | null>(transaction?.transferAccountId ?? null);
  const [transferTouched, setTransferTouched] = useState(editing);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  const suggest = async () => {
    if (!payee.trim()) return;
    try {
      if (!categoryTouched) {
        const id = await api.suggestCategory(payee);
        if (id !== null) setCategoryId(id);
      }
      // A recurring payment you've already tagged (a car loan, a card) gets
      // offered the same account again.
      if (transferTo === null && !transferTouched) {
        const target = await api.suggestTransferAccount(accountId, payee);
        if (target !== null && target !== accountId) setTransferTo(target);
      }
    } catch {
      /* suggestions are optional */
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    const cents = parseMoney(amount);
    if (cents === null || cents === 0) {
      setError("Enter an amount, like 42.50.");
      return;
    }
    const signed = direction === "out" ? -Math.abs(cents) : Math.abs(cents);
    setBusy(true);
    try {
      const input = {
        accountId,
        postedOn,
        amountCents: signed,
        payee,
        categoryId,
        notes: notes || null,
        transferAccountId: transferTo,
      };
      if (editing) await api.updateTransaction(transaction.id, input);
      else await api.createTransaction(input);
      onSaved();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  if (accounts.length === 0) {
    return (
      <Modal title="Add transaction" onClose={onClose}>
        <p className="text-sm text-zinc-600 dark:text-zinc-400">Add an account first, then you can record transactions in it.</p>
      </Modal>
    );
  }

  return (
    <Modal title={editing ? "Edit transaction" : "Add transaction"} onClose={onClose}>
      <form onSubmit={submit} className="space-y-4">
        <div className="grid grid-cols-2 gap-3">
          <Select
            label="Account"
            value={accountId}
            onChange={(e) => setAccountId(Number(e.target.value))}
            options={accounts.map((a) => ({ value: a.id, label: a.name }))}
          />
          <Field label="Date" type="date" value={postedOn} onChange={(e) => setPostedOn(e.target.value)} required />
        </div>
        <div className="grid grid-cols-[auto_1fr] items-end gap-3">
          <div className="inline-flex rounded-lg bg-zinc-100 p-1 dark:bg-zinc-800" role="radiogroup" aria-label="Direction">
            {(["out", "in"] as const).map((d) => (
              <button
                key={d}
                type="button"
                role="radio"
                aria-checked={direction === d}
                onClick={() => setDirection(d)}
                className={`rounded-md px-3 py-1.5 text-sm ${direction === d ? "bg-white shadow-sm dark:bg-zinc-700" : "text-zinc-500"}`}
              >
                {d === "out" ? "Money out" : "Money in"}
              </button>
            ))}
          </div>
          <Field label="Amount" value={amount} onChange={(e) => setAmount(e.target.value)} inputMode="decimal" placeholder="0.00" required />
        </div>
        <Field
          label="Payee / description"
          value={payee}
          onChange={(e) => setPayee(e.target.value)}
          onBlur={suggest}
          maxLength={200}
          required
        />
        <div className="space-y-1.5">
          <label className="block text-sm font-medium text-zinc-700 dark:text-zinc-300" htmlFor="tx-category">
            Category
          </label>
          <CategorySelect
            id="tx-category"
            categories={categories}
            value={categoryId}
            onChange={(id) => {
              setCategoryId(id);
              setCategoryTouched(true);
            }}
          />
        </div>
        <div className="space-y-1.5">
          <label className="block text-sm font-medium text-zinc-700 dark:text-zinc-300" htmlFor="tx-transfer">
            Paid to another account (optional)
          </label>
          <select
            id="tx-transfer"
            className={selectClass}
            value={transferTo ?? ""}
            onChange={(e) => {
              setTransferTouched(true);
              setTransferTo(e.target.value ? Number(e.target.value) : null);
            }}
          >
            <option value="">Not a transfer</option>
            {accounts
              .filter((a) => a.id !== accountId)
              .map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
          </select>
          <p className="text-xs text-zinc-500">
            {transferTo
              ? "Counted as moving money between your accounts, so it isn't spending. On a debt, it also counts toward the balance you owe."
              : "Use this for a loan or credit card payment, or money moved to savings."}
          </p>
        </div>
        <Field label="Notes (optional)" value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={1000} />
        {error ? <p className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !payee.trim() || !amount.trim()}>
            {editing ? "Save" : "Add transaction"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
