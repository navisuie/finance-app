import { useState } from "react";
import { api, errorMessage } from "../lib/api";
import { announceEnvironmentChange } from "../hooks/usePlaidStatus";
import type { PlaidItem, PlaidStatus } from "../lib/types";
import { useConfirm } from "./Confirm";
import { ConnectBank, type ConnectMode } from "./ConnectBank";
import { EnvironmentSwitch } from "./EnvironmentSwitch";
import { Button, Card } from "./ui";

function ago(iso: string | null): string {
  if (!iso) return "never";
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

export function Connections({
  status,
  items,
  onChanged,
  onEnvironmentChanged,
  onOpenSettings,
}: {
  status: PlaidStatus;
  items: PlaidItem[];
  onChanged: () => void;
  onEnvironmentChanged: () => void;
  onOpenSettings?: () => void;
}) {
  const [connect, setConnect] = useState<ConnectMode>();
  const [syncing, setSyncing] = useState<Set<number>>(new Set());
  const [messages, setMessages] = useState<Record<number, string>>({});
  const [confirm, dialog] = useConfirm();

  const sync = async (item: PlaidItem) => {
    setSyncing((s) => new Set(s).add(item.id));
    setMessages((m) => ({ ...m, [item.id]: "" }));
    try {
      const r = await api.plaidSyncItem(item.id);
      const t = r.transactions;
      setMessages((m) => ({
        ...m,
        [item.id]: `${t.added} new, ${t.modified} updated, ${t.removed} removed${r.historyPending ? " · more history still loading" : ""}`,
      }));
    } catch (e) {
      setMessages((m) => ({ ...m, [item.id]: errorMessage(e) }));
    } finally {
      setSyncing((s) => {
        const n = new Set(s);
        n.delete(item.id);
        return n;
      });
      onChanged();
    }
  };

  const syncAll = () =>
    items
      .filter((i) => i.environment === status.environment && i.linkedAccountCount > 0 && i.status !== "needs_reauth")
      .forEach(sync);

  const disconnect = async (item: PlaidItem) => {
    const ok = await confirm({
      title: `Disconnect ${item.institutionName}?`,
      body: "Plaid's access to this bank is revoked. The accounts and their history stay here as manual accounts.",
      confirmLabel: "Disconnect",
    });
    if (!ok) return;
    try {
      await api.plaidRemoveItem(item.id, false);
      onChanged();
    } catch (e) {
      const force = await confirm({
        title: "Couldn't reach Plaid",
        body: `${errorMessage(e)} Remove the connection from this app anyway? You can revoke access later in Plaid Portal (my.plaid.com).`,
        confirmLabel: "Remove anyway",
      });
      if (force) {
        await api.plaidRemoveItem(item.id, true).catch((err) => setMessages((m) => ({ ...m, [item.id]: errorMessage(err) })));
        onChanged();
      }
    }
  };

  const anyKeys = status.environments.some((e) => e.configured);
  // Connections from the other environment aren't shown here: test banks
  // belong to Sandbox, real banks to Production.
  const visible = items.filter((i) => i.environment === status.environment);

  if (!anyKeys && items.length === 0) {
    return (
      <Card>
        <p className="font-medium">Connect your banks</p>
        <p className="mt-1 text-sm text-zinc-500">
          Add your Plaid keys in Settings to sync balances and transactions automatically. You can keep Sandbox and
          Production keys side by side and switch between them here.
        </p>
        {onOpenSettings ? (
          <Button className="mt-4" onClick={onOpenSettings}>
            Open Settings
          </Button>
        ) : null}
      </Card>
    );
  }

  return (
    <section className="space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-2 px-1">
        <div className="flex items-center gap-3">
          <h2 className="text-sm font-semibold text-zinc-500">Bank connections</h2>
          <EnvironmentSwitch
            environment={status.environment}
            onChanged={() => {
              announceEnvironmentChange();
              onEnvironmentChanged();
            }}
          />
          {status.environment === "sandbox" ? (
            <span className="text-xs text-amber-700 dark:text-amber-300">Showing test data only</span>
          ) : null}
          {!status.configured ? (
            <button className="text-xs text-brand-700 hover:underline dark:text-brand-500" onClick={onOpenSettings}>
              Add {status.environment} keys
            </button>
          ) : null}
        </div>
        <div className="flex gap-2">
          {visible.length > 0 ? (
            <Button variant="secondary" className="py-1" onClick={syncAll} disabled={syncing.size > 0 || !status.configured}>
              {syncing.size > 0 ? "Syncing…" : "Sync all"}
            </Button>
          ) : null}
          <Button className="py-1" onClick={() => setConnect({ type: "new" })} disabled={!status.configured}>
            Connect a bank
          </Button>
        </div>
      </div>
      {messages[0] ? <p className="px-1 text-sm text-red-600">{messages[0]}</p> : null}
      {visible.length === 0 ? (
        <p className="px-1 text-sm text-zinc-500">
          {status.environment === "sandbox" ? "No test banks connected yet." : "No banks connected yet."}
        </p>
      ) : (
        <div className="divide-y divide-zinc-100 rounded-xl bg-white ring-1 ring-zinc-200 dark:divide-zinc-800 dark:bg-zinc-900 dark:ring-zinc-800">
          {visible.map((item) => {
            return (
              <div key={item.id} className="flex flex-wrap items-center gap-3 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <p className="font-medium">
                    {item.institutionName}
                    {item.environment === "sandbox" ? <span className="ml-2 rounded bg-amber-100 px-1.5 text-xs text-amber-800 dark:bg-amber-900 dark:text-amber-200">test</span> : null}
                  </p>
                  <p className="text-xs text-zinc-500">
                    {item.linkedAccountCount} account{item.linkedAccountCount === 1 ? "" : "s"} · synced {ago(item.lastSyncedAt)}
                    {item.products.includes("investments") ? " · investments" : ""}
                  </p>
                  {item.status === "needs_reauth" ? (
                    <p className="text-xs text-amber-700 dark:text-amber-300">Sign-in needed: {item.lastError}</p>
                  ) : item.status === "error" && item.lastError ? (
                    <p className="text-xs text-red-600">{item.lastError}</p>
                  ) : null}
                  {messages[item.id] ? <p className="text-xs text-zinc-500">{messages[item.id]}</p> : null}
                </div>
                {item.linkedAccountCount === 0 ? (
                  <Button className="py-1" onClick={() => setConnect({ type: "map", itemId: item.id, name: item.institutionName })}>
                    Choose accounts
                  </Button>
                ) : null}
                {item.status === "needs_reauth" ? (
                  <Button className="py-1" onClick={() => setConnect({ type: "reconnect", itemId: item.id, name: item.institutionName })}>
                    Reconnect
                  </Button>
                ) : null}
                {item.linkedAccountCount > 0 ? (
                  <>
                    {item.status !== "needs_reauth" ? (
                      <Button variant="secondary" className="py-1" disabled={syncing.has(item.id)} onClick={() => sync(item)}>
                        {syncing.has(item.id) ? "Syncing…" : "Sync"}
                      </Button>
                    ) : null}
                    <Button
                      variant="ghost"
                      className="py-1"
                      onClick={() => setConnect({ type: "map", itemId: item.id, name: item.institutionName })}
                    >
                      Accounts
                    </Button>
                  </>
                ) : null}
                <Button variant="ghost" className="py-1 text-red-600" onClick={() => disconnect(item)}>
                  Disconnect
                </Button>
              </div>
            );
          })}
        </div>
      )}
      {connect ? (
        <ConnectBank mode={connect} environment={status.environment} onClose={() => setConnect(undefined)} onChanged={onChanged} />
      ) : null}
      {dialog}
    </section>
  );
}
