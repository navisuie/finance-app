import { useCallback, useRef, useState, type FormEvent, type ReactNode } from "react";
import { api, errorMessage, isApiError } from "../lib/api";
import { Modal } from "./Modal";
import { Button, PasswordField } from "./ui";

/**
 * Runs an action that may need the master password (anything that touches
 * live bank data). If the confirmation has expired, the password is asked for
 * right here and the action runs straight after, so you never have to leave
 * what you were doing.
 *
 *   const { run, dialog } = useProductionPassword();
 *   const start = () => run(() => api.plaidStartLink("banking")).then(...)
 *   ... {dialog}
 */
export function useProductionPassword() {
  const [asking, setAsking] = useState(false);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const retry = useRef<{ action: () => Promise<unknown>; resolve: (v: unknown) => void } | null>(null);

  const close = () => {
    retry.current?.resolve(undefined);
    retry.current = null;
    setAsking(false);
    setPassword("");
    setError(undefined);
  };

  /** Returns the action's result, or undefined if the person cancelled. */
  const run = useCallback(async <T,>(action: () => Promise<T>): Promise<T | undefined> => {
    try {
      return await action();
    } catch (e) {
      if (!isApiError(e) || e.code !== "password_required") throw e;
      return new Promise<T | undefined>((resolve) => {
        retry.current = { action: action as () => Promise<unknown>, resolve: resolve as (v: unknown) => void };
        setError(undefined);
        setPassword("");
        setAsking(true);
      });
    }
  }, []);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const pending = retry.current;
    if (!pending) return;
    setBusy(true);
    setError(undefined);
    try {
      // Confirming the environment is what re-authorizes live actions.
      await api.plaidSetEnvironment("production", password);
      const result = await pending.action();
      retry.current = null;
      setAsking(false);
      setPassword("");
      pending.resolve(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const dialog: ReactNode = asking ? (
    <Modal title="Confirm it's you" onClose={close}>
      <form onSubmit={submit} className="space-y-4">
        <p className="text-sm text-zinc-600 dark:text-zinc-400">
          Your confirmation has expired. Enter your master password to carry on with live bank data. It won't be asked
          again for another 10 minutes.
        </p>
        <PasswordField
          label="Master password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoFocus
          error={error}
        />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={close}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !password}>
            {busy ? "Checking…" : "Continue"}
          </Button>
        </div>
      </form>
    </Modal>
  ) : null;

  return { run, dialog };
}
