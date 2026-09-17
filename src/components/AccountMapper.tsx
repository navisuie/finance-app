import { useEffect, useState } from "react";
import { useAsync } from "../hooks/useAsync";
import { KIND_LABELS, KIND_OPTIONS } from "../lib/accountKinds";
import { api, errorMessage } from "../lib/api";
import type { AccountChoice, AccountKind, PlaidAccountView, SyncResult } from "../lib/types";
import { Money } from "./privacy";
import { controlClass } from "./Select";
import { Button } from "./ui";

type Row = {
  view: PlaidAccountView;
  action: "create" | "link" | "skip";
  kind: AccountKind;
  accountId: number | null;
  syncFrom: string;
};

function initialRow(v: PlaidAccountView): Row {
  const target = v.linkedAccountId ?? v.suggestedMatchId;
  return {
    view: v,
    action: target ? "link" : "create",
    kind: v.suggestedKind,
    accountId: target,
    syncFrom: v.suggestedSyncFrom ?? "",
  };
}

/** Choose what to do with each account Plaid returned for a connection. */
export function AccountMapper({
  itemId,
  institutionName,
  onDone,
  onCancel,
}: {
  itemId: number;
  institutionName: string;
  onDone: (sync: SyncResult) => void;
  onCancel: () => void;
}) {
  const views = useAsync(() => api.plaidItemAccounts(itemId), [itemId]);
  const locals = useAsync(() => api.listAccounts(false), []);
  const [rows, setRows] = useState<Row[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (views.data) setRows(views.data.map(initialRow));
  }, [views.data]);

  const update = (i: number, patch: Partial<Row>) => setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...patch } : r)));

  // Local accounts that can be linked: not linked elsewhere, or linked to this row already.
  const linkable = (row: Row) =>
    (locals.data ?? []).filter((a) => !a.linked || a.id === row.view.linkedAccountId);

  const save = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const choices: AccountChoice[] = rows.map((r) => ({
        plaidAccountId: r.view.plaidAccountId,
        action: r.action,
        accountId: r.action === "link" ? r.accountId : null,
        kind: r.action === "create" ? r.kind : null,
        syncFrom: r.action === "link" && r.syncFrom ? r.syncFrom : null,
      }));
      const out = await api.plaidLinkAccounts(itemId, choices);
      onDone(out.sync);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (views.error) {
    return (
      <div className="space-y-3">
        <p className="text-sm text-red-600">{views.error}</p>
        <Button variant="secondary" onClick={onCancel}>
          Close
        </Button>
      </div>
    );
  }
  if (!views.data || !locals.data) return <p className="text-sm text-zinc-500">Loading accounts from {institutionName}…</p>;

  const tracked = rows.filter((r) => r.action !== "skip").length;
  const invalid = rows.some((r) => r.action === "link" && !r.accountId);

  return (
    <div className="space-y-4">
      <p className="text-sm text-zinc-600 dark:text-zinc-400">
        {institutionName} shared {rows.length} account{rows.length === 1 ? "" : "s"}. Add each one as a new account, link it
        to an account you already track (history you entered stays, and Plaid fills in after it), or skip it.
      </p>
      <div className="divide-y divide-zinc-100 rounded-xl ring-1 ring-zinc-200 dark:divide-zinc-800 dark:ring-zinc-800">
        {rows.map((r, i) => (
          <div key={r.view.plaidAccountId} className="grid grid-cols-[1fr_auto] gap-3 p-3">
            <div>
              <p className="font-medium">
                {r.view.name}
                {r.view.mask ? <span className="text-zinc-400"> ••{r.view.mask}</span> : null}
              </p>
              <p className="text-xs text-zinc-500">
                {r.view.subtype ?? r.view.accountType} · <Money cents={r.view.balanceCents} />
              </p>
            </div>
            <div className="flex flex-wrap items-center justify-end gap-2">
              <select
                className={controlClass}
                value={r.action}
                onChange={(e) => update(i, { action: e.target.value as Row["action"] })}
                aria-label={`What to do with ${r.view.name}`}
              >
                <option value="create">Add as new account</option>
                <option value="link" disabled={linkable(r).length === 0}>
                  Link to existing account
                </option>
                <option value="skip">Don't track</option>
              </select>
              {r.action === "create" ? (
                <select
                  className={controlClass}
                  value={r.kind}
                  onChange={(e) => update(i, { kind: e.target.value as AccountKind })}
                  aria-label={`Type for ${r.view.name}`}
                >
                  {KIND_OPTIONS.map((o) => (
                    <option key={o.value} value={o.value}>
                      {o.label}
                    </option>
                  ))}
                </select>
              ) : null}
              {r.action === "link" ? (
                <>
                  <select
                    className={controlClass}
                    value={r.accountId ?? ""}
                    onChange={(e) => update(i, { accountId: e.target.value ? Number(e.target.value) : null })}
                    aria-label={`Existing account for ${r.view.name}`}
                  >
                    <option value="">Choose account…</option>
                    {linkable(r).map((a) => (
                      <option key={a.id} value={a.id}>
                        {a.name} ({KIND_LABELS[a.kind]})
                      </option>
                    ))}
                  </select>
                  <label className="flex items-center gap-1 text-xs text-zinc-500">
                    Plaid history from
                    <input
                      type="date"
                      className={`${controlClass} py-1`}
                      value={r.syncFrom}
                      onChange={(e) => update(i, { syncFrom: e.target.value })}
                      aria-label={`Import Plaid history from for ${r.view.name}`}
                    />
                  </label>
                </>
              ) : null}
            </div>
          </div>
        ))}
      </div>
      {rows.some((r) => r.action === "link" && r.syncFrom) ? (
        <p className="text-xs text-zinc-500">
          The start date is the day after the latest transaction you already have in that account, so nothing is
          duplicated. Clear it to pull all available history (up to 2 years).
        </p>
      ) : null}
      {error ? <p className="text-sm text-red-600">{error}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="secondary" onClick={onCancel}>
          Decide later
        </Button>
        <Button onClick={save} disabled={busy || invalid}>
          {busy ? "Syncing…" : `Save and sync ${tracked} account${tracked === 1 ? "" : "s"}`}
        </Button>
      </div>
    </div>
  );
}
