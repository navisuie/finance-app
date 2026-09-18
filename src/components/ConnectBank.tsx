import { useEffect, useRef, useState } from "react";
import { api, errorMessage, isApiError } from "../lib/api";
import type { LinkStart, PlaidEnvironment, SyncResult } from "../lib/types";
import { AccountMapper } from "./AccountMapper";
import { useProductionPassword } from "./ProductionPassword";
import { Modal } from "./Modal";
import { Button } from "./ui";

const POLL_MS = 3000;

export type ConnectMode =
  | { type: "new" }
  | { type: "reconnect"; itemId: number; name: string }
  | { type: "map"; itemId: number; name: string };

type Step =
  | { s: "choose" }
  | { s: "waiting"; start: LinkStart }
  | { s: "map"; itemId: number; name: string }
  | { s: "done"; sync: SyncResult }
  | { s: "reconnected"; name: string };

export function ConnectBank({
  mode,
  environment,
  onClose,
  onChanged,
}: {
  mode: ConnectMode;
  environment: PlaidEnvironment | null;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [step, setStep] = useState<Step>(
    mode.type === "map" ? { s: "map", itemId: mode.itemId, name: mode.name } : { s: "choose" },
  );
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const startedReconnect = useRef(false);
  const { run, dialog: passwordDialog } = useProductionPassword();
  const changedRef = useRef(onChanged);
  changedRef.current = onChanged;

  const begin = async (kind: "banking" | "investments") => {
    setBusy(true);
    setError(undefined);
    try {
      // Asks for the master password in place if the confirmation expired.
      const start = await run(() => api.plaidStartLink(kind));
      if (start) setStep({ s: "waiting", start });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const sandbox = async (kind: "banking" | "investments") => {
    setBusy(true);
    setError(undefined);
    try {
      const r = await api.plaidSandboxAddItem(kind);
      if (r.state === "connected") {
        onChanged();
        setStep({ s: "map", itemId: r.itemId, name: r.institutionName });
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  // Reconnect starts immediately.
  useEffect(() => {
    if (mode.type !== "reconnect" || startedReconnect.current) return;
    startedReconnect.current = true;
    run(() => api.plaidStartReconnect(mode.itemId))
      .then((start) => start && setStep({ s: "waiting", start }))
      .catch((e) => setError(errorMessage(e)));
  }, [mode, run]);

  // Poll while the user finishes in the browser.
  const token = step.s === "waiting" ? step.start.linkToken : undefined;
  useEffect(() => {
    if (!token) return;
    let stopped = false;
    const tick = async () => {
      try {
        const r = await api.plaidPollLink(token);
        if (stopped) return;
        if (r.state === "connected") {
          changedRef.current();
          setStep({ s: "map", itemId: r.itemId, name: r.institutionName });
        } else if (r.state === "reconnected") {
          changedRef.current();
          setStep({ s: "reconnected", name: mode.type === "reconnect" ? mode.name : "Your bank" });
        } else if (r.state === "exited") {
          setError(r.message);
          setStep({ s: "choose" });
        } else {
          timer = window.setTimeout(tick, POLL_MS);
        }
      } catch (e) {
        if (stopped) return;
        // Keep polling through brief network trouble; stop on anything else.
        if (isApiError(e) && e.code === "plaid" && e.plaid_code === "NETWORK_ERROR") {
          timer = window.setTimeout(tick, POLL_MS * 2);
        } else {
          setError(errorMessage(e));
          setStep({ s: "choose" });
        }
      }
    };
    let timer = window.setTimeout(tick, POLL_MS);
    return () => {
      stopped = true;
      window.clearTimeout(timer);
    };
  }, [token, mode]);

  const close = () => {
    if (step.s === "waiting") api.plaidCancelLink(step.start.linkToken).catch(() => {});
    onClose();
  };

  const title =
    mode.type === "reconnect" ? `Reconnect ${mode.name}` : step.s === "map" ? `Choose accounts: ${step.name}` : "Connect a bank";

  return (
    <Modal title={title} onClose={close} wide={step.s === "map"}>
      {step.s === "choose" && mode.type === "new" ? (
        <div className="space-y-4">
          <p className="text-sm text-zinc-600 dark:text-zinc-400">
            Plaid opens in your web browser. You sign in to your bank there; your bank password never passes through this
            app. When you're done, come back here.
          </p>
          {environment === "production" ? (
            <p className="rounded-lg bg-zinc-100 p-3 text-sm dark:bg-zinc-800">
              <span className="font-medium">This connects a real bank.</span> It uses one of the 10 institutions included
              in Plaid's free Trial plan, and disconnecting later doesn't give the slot back.
            </p>
          ) : null}
          <div className="grid grid-cols-2 gap-3">
            <ChoiceCard
              title="Bank or credit card"
              body="Checking, savings, HYSA, credit cards. Syncs balances and transactions."
              disabled={busy}
              onClick={() => begin("banking")}
            />
            <ChoiceCard
              title="Investments or retirement"
              body="Brokerage, 401(k), IRA. Syncs balances and holdings."
              disabled={busy}
              onClick={() => begin("investments")}
            />
          </div>
          {environment === "sandbox" ? (
            <div className="rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200">
              <p>
                <span className="font-medium">Sandbox mode.</span> In the browser, pick any bank and sign in with{" "}
                <code>user_good</code> / <code>pass_good</code>. Or add a test bank instantly:
              </p>
              <div className="mt-2 flex gap-2">
                <Button variant="secondary" className="py-1" disabled={busy} onClick={() => sandbox("banking")}>
                  Add test bank
                </Button>
                <Button variant="secondary" className="py-1" disabled={busy} onClick={() => sandbox("investments")}>
                  Add test brokerage
                </Button>
              </div>
            </div>
          ) : null}
          {error ? <p className="text-sm text-red-600">{error}</p> : null}
        </div>
      ) : null}

      {step.s === "choose" && mode.type === "reconnect" ? (
        <div className="space-y-3">
          {error ? <p className="text-sm text-red-600">{error}</p> : <p className="text-sm text-zinc-500">Opening Plaid…</p>}
          <div className="flex justify-end">
            <Button variant="secondary" onClick={close}>
              Close
            </Button>
          </div>
        </div>
      ) : null}

      {step.s === "waiting" ? (
        <div className="space-y-4 text-center">
          <div className="mx-auto h-10 w-10 animate-spin rounded-full border-4 border-zinc-200 border-t-brand-600" aria-hidden="true" />
          <p className="font-medium">Finish in your browser</p>
          <p className="text-sm text-zinc-500">
            This window updates automatically when you're done. The link is valid for 30 minutes.
          </p>
          <div className="flex justify-center gap-2">
            <Button variant="secondary" onClick={() => api.plaidReopenLink(step.start.linkToken).catch((e) => setError(errorMessage(e)))}>
              Open the browser again
            </Button>
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
          </div>
          {error ? <p className="text-sm text-red-600">{error}</p> : null}
        </div>
      ) : null}

      {passwordDialog}
      {step.s === "map" ? (
        <AccountMapper
          itemId={step.itemId}
          institutionName={step.name}
          onCancel={close}
          onDone={(sync) => {
            onChanged();
            setStep({ s: "done", sync });
          }}
        />
      ) : null}

      {step.s === "done" ? (
        <div className="space-y-4">
          <p className="text-lg font-medium">{step.sync.institutionName} is connected</p>
          <ul className="space-y-1 text-sm text-zinc-600 dark:text-zinc-400">
            <li>{step.sync.balancesUpdated} balance(s) updated</li>
            <li>{step.sync.transactions.added} transaction(s) imported</li>
            {step.sync.holdingsUpdated > 0 ? <li>{step.sync.holdingsUpdated} holding(s) updated</li> : null}
          </ul>
          {step.sync.historyPending ? (
            <p className="rounded-lg bg-zinc-100 p-3 text-sm dark:bg-zinc-800">
              The bank is still sending older history. Use Sync on the Accounts page in a few minutes to pull the rest.
            </p>
          ) : null}
          <div className="flex justify-end">
            <Button onClick={onClose}>Done</Button>
          </div>
        </div>
      ) : null}

      {step.s === "reconnected" ? (
        <div className="space-y-4">
          <p className="text-lg font-medium">{step.name} is reconnected</p>
          <p className="text-sm text-zinc-500">Syncing works again. Use Sync to pull the latest data.</p>
          <div className="flex justify-end">
            <Button onClick={onClose}>Done</Button>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}

function ChoiceCard({ title, body, onClick, disabled }: { title: string; body: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="rounded-xl p-4 text-left ring-1 ring-zinc-200 transition hover:ring-brand-500 disabled:opacity-50 dark:ring-zinc-700"
    >
      <p className="font-medium">{title}</p>
      <p className="mt-1 text-xs text-zinc-500">{body}</p>
    </button>
  );
}
