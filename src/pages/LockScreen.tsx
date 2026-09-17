import { useEffect, useState, type FormEvent } from "react";
import { CenteredPanel, Button, Logo, PasswordField } from "../components/ui";
import { api, isApiError, errorMessage } from "../lib/api";

export function LockScreen({
  onUnlocked,
  onForgot,
  notice,
}: {
  onUnlocked: () => void;
  onForgot: () => void;
  notice?: string;
}) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [waitUntil, setWaitUntil] = useState<number>(0);
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    if (waitUntil <= Date.now()) return;
    const t = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(t);
  }, [waitUntil]);

  const secondsLeft = Math.max(0, Math.ceil((waitUntil - now) / 1000));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!password || secondsLeft > 0) return;
    setBusy(true);
    setError(undefined);
    try {
      await api.unlock(password);
      setPassword("");
      onUnlocked();
    } catch (err) {
      if (isApiError(err) && err.code === "too_many_attempts" && err.retry_after_secs) {
        setWaitUntil(Date.now() + err.retry_after_secs * 1000);
        setNow(Date.now());
      }
      setError(errorMessage(err));
      setPassword("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <CenteredPanel>
      <form onSubmit={submit} className="space-y-5">
        <div className="flex flex-col items-center gap-3 text-center">
          <Logo size={56} />
          <h1 className="text-xl font-semibold">Finance is locked</h1>
          {notice ? <p className="text-sm text-zinc-500">{notice}</p> : null}
        </div>
        <PasswordField
          label="Master password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoFocus
          error={secondsLeft > 0 ? `Too many attempts. Try again in ${secondsLeft} s.` : error}
        />
        <Button type="submit" className="w-full" disabled={busy || !password || secondsLeft > 0}>
          {busy ? "Unlocking…" : "Unlock"}
        </Button>
        <button type="button" onClick={onForgot} className="w-full text-center text-sm text-brand-700 hover:underline dark:text-brand-500">
          Forgot password? Use your recovery key
        </button>
      </form>
    </CenteredPanel>
  );
}
