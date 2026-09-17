import { useEffect, useState, type FormEvent } from "react";
import { Button, Card, PasswordField } from "../components/ui";
import { StrengthMeter } from "../components/StrengthMeter";
import { CategoryManager } from "../components/CategoryManager";
import { PlaidSettings } from "../components/PlaidSettings";
import { api, errorMessage, type AppSettings } from "../lib/api";
import type { ImportProfile } from "../lib/types";
import { MIN_LENGTH } from "../lib/password";

const LOCK_OPTIONS = [1, 2, 5, 10, 15, 30, 60];

export function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings>();
  const [lockMsg, setLockMsg] = useState<string>();

  useEffect(() => {
    api.getSettings().then(setSettings).catch((e) => setLockMsg(errorMessage(e)));
  }, []);

  const changeLock = async (minutes: number) => {
    try {
      setSettings(await api.setAutoLock(minutes));
      setLockMsg("Saved.");
    } catch (e) {
      setLockMsg(errorMessage(e));
    }
  };

  return (
    <div className="max-w-2xl space-y-6">
      <h1 className="text-2xl font-semibold">Settings</h1>

      <Card title="Auto-lock">
        <div className="flex items-center gap-3">
          <label htmlFor="autolock" className="text-sm">
            Lock after
          </label>
          <select
            id="autolock"
            value={settings?.autoLockMinutes ?? 5}
            onChange={(e) => changeLock(Number(e.target.value))}
            className="rounded-lg bg-white px-2 py-1.5 text-sm ring-1 ring-zinc-300 dark:bg-zinc-900 dark:ring-zinc-700"
          >
            {LOCK_OPTIONS.map((m) => (
              <option key={m} value={m}>
                {m} {m === 1 ? "minute" : "minutes"} idle
              </option>
            ))}
          </select>
          {lockMsg ? <span className="text-xs text-zinc-500">{lockMsg}</span> : null}
        </div>
      </Card>

      <PlaidSettings />

      <CategoryManager />

      <ImportProfiles />

      <ChangePassword />

      <Card title="Coming later">
        <ul className="list-disc space-y-1 pl-5 text-sm text-zinc-500">
          <li>Touch ID unlock and encrypted backups (phase 5)</li>
        </ul>
      </Card>
    </div>
  );
}

function ImportProfiles() {
  const [profiles, setProfiles] = useState<ImportProfile[]>();
  const load = () => api.listImportProfiles().then(setProfiles).catch(() => setProfiles([]));
  useEffect(() => {
    load();
  }, []);
  if (!profiles || profiles.length === 0) return null;
  return (
    <Card title="Saved CSV import settings">
      <ul className="divide-y divide-zinc-100 text-sm dark:divide-zinc-800">
        {profiles.map((p) => (
          <li key={p.id} className="flex items-center justify-between py-1.5">
            <span>{p.name}</span>
            <button
              className="text-xs text-zinc-400 hover:text-red-600"
              onClick={() => api.deleteImportProfile(p.id).then(load)}
              aria-label={`Delete ${p.name}`}
            >
              Remove
            </button>
          </li>
        ))}
      </ul>
    </Card>
  );
}

function ChangePassword() {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string }>();
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setMsg(undefined);
    if (next !== confirm) {
      setMsg({ ok: false, text: "New passwords don't match." });
      return;
    }
    setBusy(true);
    try {
      await api.changePassword(current, next);
      setCurrent("");
      setNext("");
      setConfirm("");
      setMsg({ ok: true, text: "Password changed. Your recovery key still works." });
    } catch (err) {
      setMsg({ ok: false, text: errorMessage(err) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title="Change master password">
      <form onSubmit={submit} className="space-y-4">
        <PasswordField label="Current password" value={current} onChange={(e) => setCurrent(e.target.value)} />
        <PasswordField label="New password" value={next} onChange={(e) => setNext(e.target.value)} />
        <StrengthMeter password={next} />
        <PasswordField label="Confirm new password" value={confirm} onChange={(e) => setConfirm(e.target.value)} />
        {msg ? (
          <p className={`text-sm ${msg.ok ? "text-brand-700 dark:text-brand-500" : "text-red-600 dark:text-red-400"}`}>{msg.text}</p>
        ) : null}
        <Button type="submit" disabled={busy || !current || next.length < MIN_LENGTH}>
          {busy ? "Updating…" : "Change password"}
        </Button>
      </form>
    </Card>
  );
}
