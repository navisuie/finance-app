import { useState, type FormEvent } from "react";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import type { PlaidEnvironment } from "../lib/types";
import { useConfirm } from "./Confirm";
import { Select } from "./Select";
import { Button, Card, Field, PasswordField } from "./ui";

export function PlaidSettings() {
  const status = useAsync(() => api.plaidStatus(), []);
  const [editing, setEditing] = useState(false);
  const [environment, setEnvironment] = useState<PlaidEnvironment>("sandbox");
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string }>();
  const [confirm, dialog] = useConfirm();

  const s = status.data;
  const showForm = editing || (s && !s.configured);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setMsg(undefined);
    try {
      await api.plaidSaveCredentials(environment, clientId, secret);
      setSecret("");
      setClientId("");
      setEditing(false);
      setMsg({ ok: true, text: "Keys verified with Plaid and saved in your vault." });
      status.reload();
    } catch (err) {
      setMsg({ ok: false, text: errorMessage(err) });
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    const ok = await confirm({
      title: "Remove Plaid keys?",
      body: "Syncing stops until you add keys again. Your connections and data stay in the vault.",
      confirmLabel: "Remove keys",
    });
    if (!ok) return;
    try {
      await api.plaidClearCredentials();
      status.reload();
    } catch (err) {
      setMsg({ ok: false, text: errorMessage(err) });
    }
  };

  return (
    <Card title="Bank connections (Plaid)">
      {s?.configured && !editing ? (
        <div className="flex items-center justify-between gap-3">
          <div className="text-sm">
            <p>
              Connected to Plaid <span className="font-medium">{s.environment === "production" ? "Production" : "Sandbox"}</span>{" "}
              <span className="text-zinc-500">· client ID …{s.clientIdHint}</span>
            </p>
            {s.environment === "sandbox" ? (
              <p className="text-xs text-zinc-500">Sandbox uses test banks only. Switch to Production to connect your real accounts.</p>
            ) : null}
          </div>
          <div className="flex gap-2">
            <Button
              variant="secondary"
              onClick={() => {
                setEnvironment(s.environment ?? "sandbox");
                setEditing(true);
              }}
            >
              Change keys
            </Button>
            <Button variant="ghost" className="text-red-600" onClick={remove}>
              Remove
            </Button>
          </div>
        </div>
      ) : null}

      {showForm ? (
        <form onSubmit={save} className="space-y-4">
          <ol className="list-decimal space-y-1 pl-5 text-sm text-zinc-600 dark:text-zinc-400">
            <li>Sign up for free at dashboard.plaid.com.</li>
            <li>
              Open <span className="font-medium">Developers → Keys</span> and copy your client ID and the secret for the
              environment you want.
            </li>
            <li>Start with Sandbox to try it with test banks. Use Production for your real accounts.</li>
          </ol>
          <div className="grid grid-cols-2 gap-3">
            <Select
              label="Environment"
              value={environment}
              onChange={(e) => setEnvironment(e.target.value as PlaidEnvironment)}
              options={[
                { value: "sandbox", label: "Sandbox (test banks)" },
                { value: "production", label: "Production (real banks)" },
              ]}
            />
            <Field
              label="Client ID"
              value={clientId}
              onChange={(e) => setClientId(e.target.value.trim())}
              autoComplete="off"
              spellCheck={false}
              required
            />
          </div>
          <PasswordField
            label={`${environment === "production" ? "Production" : "Sandbox"} secret`}
            value={secret}
            onChange={(e) => setSecret(e.target.value.trim())}
            required
          />
          <p className="text-xs text-zinc-500">
            Keys are stored inside your encrypted vault, never in plain files, and are only sent to Plaid. On Plaid's free
            Trial plan you can connect up to 10 institutions in Production. Removing one does not free up a slot.
          </p>
          <div className="flex justify-end gap-2">
            {editing ? (
              <Button type="button" variant="secondary" onClick={() => setEditing(false)}>
                Cancel
              </Button>
            ) : null}
            <Button type="submit" disabled={busy || !clientId || !secret}>
              {busy ? "Checking with Plaid…" : "Verify and save"}
            </Button>
          </div>
        </form>
      ) : null}

      {msg ? <p className={`mt-3 text-sm ${msg.ok ? "text-brand-700 dark:text-brand-500" : "text-red-600"}`}>{msg.text}</p> : null}
      {status.error ? <p className="text-sm text-red-600">{status.error}</p> : null}
      {dialog}
    </Card>
  );
}
