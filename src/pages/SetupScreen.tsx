import { useState, type FormEvent } from "react";
import { CenteredPanel, Button, Logo, PasswordField } from "../components/ui";
import { StrengthMeter } from "../components/StrengthMeter";
import { RecoveryKeyPanel } from "../components/RecoveryKeyPanel";
import { api, errorMessage } from "../lib/api";
import { MIN_LENGTH } from "../lib/password";

export function SetupScreen({ onReady }: { onReady: () => void }) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [recoveryKey, setRecoveryKey] = useState<string>();

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    if (password !== confirm) {
      setError("Passwords don't match.");
      return;
    }
    setBusy(true);
    try {
      const { recoveryKey } = await api.createVault(password);
      setPassword("");
      setConfirm("");
      setRecoveryKey(recoveryKey);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  if (recoveryKey) {
    return (
      <CenteredPanel>
        <RecoveryKeyPanel
          recoveryKey={recoveryKey}
          onDone={() => {
            setRecoveryKey(undefined);
            onReady();
          }}
        />
      </CenteredPanel>
    );
  }

  return (
    <CenteredPanel>
      <form onSubmit={submit} className="space-y-5">
        <div className="flex items-center gap-3">
          <Logo />
          <div>
            <h1 className="text-xl font-semibold">Create your vault</h1>
            <p className="text-sm text-zinc-500">Your data is encrypted on this Mac with this password.</p>
          </div>
        </div>
        <PasswordField
          label="Master password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoFocus
          hint={`At least ${MIN_LENGTH} characters. A phrase of 3–4 unrelated words is strong and memorable.`}
        />
        <StrengthMeter password={password} />
        <PasswordField label="Confirm password" value={confirm} onChange={(e) => setConfirm(e.target.value)} />
        {error ? <p className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
        <Button type="submit" className="w-full" disabled={busy || password.length < MIN_LENGTH}>
          {busy ? "Securing your vault…" : "Create vault"}
        </Button>
        <p className="text-xs text-zinc-500">
          There is no password reset by email. If you forget this password, only your recovery key can unlock the
          vault.
        </p>
      </form>
    </CenteredPanel>
  );
}
