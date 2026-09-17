import { useState, type FormEvent } from "react";
import { CenteredPanel, Button, Field, PasswordField } from "../components/ui";
import { StrengthMeter } from "../components/StrengthMeter";
import { RecoveryKeyPanel } from "../components/RecoveryKeyPanel";
import { api, errorMessage } from "../lib/api";
import { formatRecoveryInput, MIN_LENGTH } from "../lib/password";

export function RecoverScreen({ onDone, onCancel }: { onDone: () => void; onCancel: () => void }) {
  const [key, setKey] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [newKey, setNewKey] = useState<string>();

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    if (password !== confirm) {
      setError("Passwords don't match.");
      return;
    }
    setBusy(true);
    try {
      const res = await api.recover(key, password);
      setKey("");
      setPassword("");
      setConfirm("");
      setNewKey(res.recoveryKey);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  if (newKey) {
    return (
      <CenteredPanel>
        <p className="mb-4 rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200">
          Your old recovery key no longer works. Here is your new one.
        </p>
        <RecoveryKeyPanel recoveryKey={newKey} onDone={onDone} />
      </CenteredPanel>
    );
  }

  return (
    <CenteredPanel>
      <form onSubmit={submit} className="space-y-5">
        <div>
          <h1 className="text-xl font-semibold">Reset with recovery key</h1>
          <p className="mt-1 text-sm text-zinc-500">Enter the key you saved when you created the vault, then choose a new password.</p>
        </div>
        <Field
          label="Recovery key"
          value={key}
          onChange={(e) => setKey(formatRecoveryInput(e.target.value))}
          placeholder="XXXX-XXXX-XXXX-…"
          className="block w-full rounded-lg border-0 bg-white px-3 py-2 font-mono text-sm ring-1 ring-zinc-300 outline-none focus:ring-2 focus:ring-brand-500 dark:bg-zinc-900 dark:ring-zinc-700"
          autoComplete="off"
          spellCheck={false}
          autoFocus
        />
        <PasswordField label="New master password" value={password} onChange={(e) => setPassword(e.target.value)} />
        <StrengthMeter password={password} />
        <PasswordField label="Confirm new password" value={confirm} onChange={(e) => setConfirm(e.target.value)} />
        {error ? <p className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
        <div className="flex gap-2">
          <Button type="button" variant="secondary" onClick={onCancel} className="flex-1">
            Back
          </Button>
          <Button type="submit" className="flex-1" disabled={busy || key.length < 64 || password.length < MIN_LENGTH}>
            {busy ? "Resetting…" : "Reset password"}
          </Button>
        </div>
      </form>
    </CenteredPanel>
  );
}
