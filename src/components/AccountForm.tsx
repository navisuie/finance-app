import { useState, type FormEvent } from "react";
import { api, errorMessage } from "../lib/api";
import { defaultIsAsset, KIND_OPTIONS } from "../lib/accountKinds";
import { centsToInput, parseMoney, todayISO } from "../lib/money";
import type { Account, AccountKind } from "../lib/types";
import { Modal } from "./Modal";
import { Select } from "./Select";
import { Button, Field } from "./ui";

export function AccountForm({
  account,
  onClose,
  onSaved,
}: {
  account?: Account;
  onClose: () => void;
  onSaved: (a: Account) => void;
}) {
  const editing = !!account;
  const [name, setName] = useState(account?.name ?? "");
  const [kind, setKind] = useState<AccountKind>(account?.kind ?? "checking");
  const [institution, setInstitution] = useState(account?.institution ?? "");
  const [last4, setLast4] = useState(account?.last4 ?? "");
  const [balance, setBalance] = useState(account?.balanceCents != null ? centsToInput(account.balanceCents) : "");
  const [asOf, setAsOf] = useState(todayISO());
  const [apr, setApr] = useState(account?.aprBps != null ? (account.aprBps / 100).toString() : "");
  const [payment, setPayment] = useState(
    account?.monthlyPaymentCents != null ? centsToInput(account.monthlyPaymentCents) : "",
  );
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  const liability = !defaultIsAsset(kind);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    let balanceCents: number | null = null;
    if (!editing && balance.trim()) {
      balanceCents = parseMoney(balance);
      if (balanceCents === null) {
        setError("Enter the balance as a number, like 1250.00.");
        return;
      }
    }
    let aprBps: number | null = null;
    if (liability && apr.trim()) {
      const pct = Number(apr);
      if (!Number.isFinite(pct) || pct < 0 || pct > 500) {
        setError("Enter the rate as a percentage, like 6.49.");
        return;
      }
      aprBps = Math.round(pct * 100);
    }
    let monthlyPaymentCents: number | null = null;
    if (liability && payment.trim()) {
      monthlyPaymentCents = parseMoney(payment);
      if (monthlyPaymentCents === null || monthlyPaymentCents < 0) {
        setError("Enter the payment as a number, like 450.00.");
        return;
      }
    }
    setBusy(true);
    try {
      const input = {
        name,
        kind,
        institution: institution || null,
        last4: last4 || null,
        isAsset: defaultIsAsset(kind),
        balanceCents,
        balanceAsOf: balanceCents !== null ? asOf : null,
        aprBps,
        monthlyPaymentCents,
      };
      const saved = editing ? await api.updateAccount(account.id, input) : await api.createAccount(input);
      onSaved(saved);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={editing ? "Edit account" : "Add account"} onClose={onClose}>
      <form onSubmit={submit} className="space-y-4">
        <Field label="Name" value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Ally Savings" maxLength={100} required />
        <Select label="Type" value={kind} onChange={(e) => setKind(e.target.value as AccountKind)} options={KIND_OPTIONS} />
        <div className="grid grid-cols-2 gap-3">
          <Field label="Institution (optional)" value={institution} onChange={(e) => setInstitution(e.target.value)} maxLength={100} />
          <Field
            label="Last 4 digits (optional)"
            value={last4}
            onChange={(e) => setLast4(e.target.value.replace(/\D/g, "").slice(0, 4))}
            inputMode="numeric"
            placeholder="1234"
            hint="Never enter the full account number."
          />
        </div>
        {!editing ? (
          <div className="grid grid-cols-2 gap-3">
            <Field
              label={liability ? "Amount owed (optional)" : "Current balance (optional)"}
              value={balance}
              onChange={(e) => setBalance(e.target.value)}
              inputMode="decimal"
              placeholder="0.00"
            />
            <Field label="As of" type="date" value={asOf} onChange={(e) => setAsOf(e.target.value)} />
          </div>
        ) : (
          <p className="text-xs text-zinc-500">Update the balance from the account's page.</p>
        )}
        {liability ? (
          <>
            <div className="grid grid-cols-2 gap-3">
              <Field
                label="Interest rate (optional)"
                value={apr}
                onChange={(e) => setApr(e.target.value)}
                inputMode="decimal"
                placeholder="6.49"
                hint="Annual percentage rate"
              />
              <Field
                label="Monthly payment (optional)"
                value={payment}
                onChange={(e) => setPayment(e.target.value)}
                inputMode="decimal"
                placeholder="450.00"
                hint="Used to work out your payoff date"
              />
            </div>
            <p className="text-xs text-zinc-500">
              Debts count against your net worth. Enter what you owe as a positive number. Link the payments you make
              from a transaction so they don't count as spending.
            </p>
          </>
        ) : null}
        {error ? <p className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !name.trim()}>
            {editing ? "Save" : "Add account"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

export function BalanceForm({
  account,
  suggestedCents,
  note,
  onClose,
  onSaved,
}: {
  account: Account;
  /** Pre-filled amount, e.g. the balance after linked payments. */
  suggestedCents?: number;
  note?: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [amount, setAmount] = useState(
    centsToInput(suggestedCents ?? account.balanceCents ?? 0),
  );
  const [asOf, setAsOf] = useState(todayISO());
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const cents = parseMoney(amount);
    if (cents === null) {
      setError("Enter the balance as a number, like 1250.00.");
      return;
    }
    setBusy(true);
    try {
      await api.setBalance(account.id, asOf, cents);
      onSaved();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={`Update balance: ${account.name}`} onClose={onClose}>
      <form onSubmit={submit} className="space-y-4">
        <div className="grid grid-cols-2 gap-3">
          <Field
            label={account.isAsset ? "Balance" : "Amount owed"}
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            inputMode="decimal"
            autoFocus
          />
          <Field label="As of" type="date" value={asOf} onChange={(e) => setAsOf(e.target.value)} />
        </div>
        {note ? <p className="text-xs text-zinc-500">{note}</p> : null}
        <p className="text-xs text-zinc-500">Saving a balance for a date that already has one replaces it.</p>
        {error ? <p className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy}>
            Save balance
          </Button>
        </div>
      </form>
    </Modal>
  );
}
