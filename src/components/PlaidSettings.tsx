import { useState, type FormEvent } from "react";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import type { PlaidEnvironment } from "../lib/types";
import { useConfirm } from "./Confirm";
import { Segmented } from "./Segmented";
import { Button, Card, Field, PasswordField } from "./ui";

const LABELS: Record<PlaidEnvironment, string> = {
  sandbox: "Sandbox (test banks)",
  production: "Production (your real banks)",
};

export function PlaidSettings() {
  const status = useAsync(() => api.plaidStatus(), []);
  const [editing, setEditing] = useState<PlaidEnvironment>();
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string }>();
  const [confirm, dialog] = useConfirm();

  const s = status.data;

  const startEdit = (env: PlaidEnvironment) => {
    setEditing(env);
    setClientId("");
    setSecret("");
    setMsg(undefined);
  };

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!editing) return;
    setBusy(true);
    setMsg(undefined);
    try {
      await api.plaidSaveCredentials(editing, clientId, secret);
      setEditing(undefined);
      setClientId("");
      setSecret("");
      setMsg({ ok: true, text: `${LABELS[editing]} keys verified with Plaid and saved. This environment is now active.` });
      status.reload();
    } catch (err) {
      setMsg({ ok: false, text: errorMessage(err) });
    } finally {
      setBusy(false);
    }
  };

  const remove = async (env: PlaidEnvironment) => {
    const ok = await confirm({
      title: `Remove ${env} keys?`,
      body: "Syncing in that environment stops until you add keys again. Your connections and data stay in the vault.",
      confirmLabel: "Remove keys",
    });
    if (!ok) return;
    try {
      await api.plaidClearCredentials(env);
      status.reload();
    } catch (err) {
      setMsg({ ok: false, text: errorMessage(err) });
    }
  };

  const switchTo = async (env: PlaidEnvironment) => {
    try {
      await api.plaidSetEnvironment(env);
      status.reload();
    } catch (err) {
      setMsg({ ok: false, text: errorMessage(err) });
    }
  };

  return (
    <Card title="Bank connections (Plaid)">
      {s ? (
        <div className="space-y-4">
          <div className="flex flex-wrap items-center gap-3">
            <span className="text-sm text-zinc-600 dark:text-zinc-400">Using</span>
            <Segmented
              label="Plaid environment"
              value={s.environment}
              onChange={switchTo}
              options={[
                { value: "sandbox", label: "Sandbox" },
                { value: "production", label: "Production" },
              ]}
            />
            <span className="text-xs text-zinc-500">You can also switch this from the Accounts page.</span>
          </div>

          <ul className="divide-y divide-zinc-100 text-sm dark:divide-zinc-800">
            {s.environments.map((env) => (
              <li key={env.environment} className="flex flex-wrap items-center gap-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="font-medium">
                    {LABELS[env.environment]}
                    {env.environment === s.environment ? (
                      <span className="ml-2 rounded bg-brand-50 px-1.5 py-0.5 text-[10px] font-medium text-brand-700 dark:bg-brand-900/40 dark:text-brand-500">
                        ACTIVE
                      </span>
                    ) : null}
                  </p>
                  <p className="text-xs text-zinc-500">
                    {env.configured ? `Keys saved · client ID …${env.clientIdHint}` : "No keys yet"}
                  </p>
                </div>
                <Button variant="secondary" className="py-1" onClick={() => startEdit(env.environment)}>
                  {env.configured ? "Replace keys" : "Add keys"}
                </Button>
                {env.configured ? (
                  <Button variant="ghost" className="py-1 text-red-600" onClick={() => remove(env.environment)}>
                    Remove
                  </Button>
                ) : null}
              </li>
            ))}
          </ul>

          {!s.configured ? (
            <p className="rounded-lg bg-amber-50 p-3 text-xs text-amber-900 dark:bg-amber-950 dark:text-amber-200">
              {s.environment === "sandbox" ? "Sandbox" : "Production"} has no keys yet, so connecting a bank is turned off
              while it's active.
            </p>
          ) : null}
        </div>
      ) : null}

      {editing ? (
        <form onSubmit={save} className="mt-5 space-y-4 border-t border-zinc-200 pt-5 dark:border-zinc-800">
          <p className="text-sm font-medium">{LABELS[editing]} keys</p>
          <ol className="list-decimal space-y-1 pl-5 text-sm text-zinc-600 dark:text-zinc-400">
            <li>Sign in at dashboard.plaid.com (free).</li>
            <li>
              Open <span className="font-medium">Developers → Keys</span> and copy your client ID and the{" "}
              <span className="font-medium">{editing}</span> secret.
            </li>
          </ol>
          <Field
            label="Client ID"
            value={clientId}
            onChange={(e) => setClientId(e.target.value.trim())}
            autoComplete="off"
            spellCheck={false}
            required
          />
          <PasswordField
            label={`${editing === "production" ? "Production" : "Sandbox"} secret`}
            value={secret}
            onChange={(e) => setSecret(e.target.value.trim())}
            required
          />
          <p className="text-xs text-zinc-500">
            Keys are stored inside your encrypted vault and only ever sent to Plaid. Sandbox and Production keys are kept
            separately, so switching between them doesn't need re-typing. On Plaid's free Trial plan you can connect up to
            10 institutions in Production, and removing one doesn't free a slot.
          </p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setEditing(undefined)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !clientId || !secret}>
              {busy ? "Checking with Plaid…" : "Verify and save"}
            </Button>
          </div>
        </form>
      ) : null}

      {msg ? <p className={`mt-3 text-sm ${msg.ok ? "text-brand-700 dark:text-brand-500" : "text-red-600"}`}>{msg.text}</p> : null}
      {status.error ? <p className="mt-3 text-sm text-red-600">{status.error}</p> : null}
      {dialog}
    </Card>
  );
}
