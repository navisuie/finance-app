import { useState, type FormEvent } from "react";
import { api, errorMessage, isApiError } from "../lib/api";
import type { PlaidEnvironment } from "../lib/types";
import { Modal } from "./Modal";
import { Segmented } from "./Segmented";
import { Button, PasswordField } from "./ui";

/**
 * Switches between Sandbox and Production. Going live asks for the master
 * password (the backend remembers it for 10 minutes), so real bank data is
 * never one stray click away.
 */
export function EnvironmentSwitch({
  environment,
  onChanged,
  size = "normal",
}: {
  environment: PlaidEnvironment;
  onChanged: () => void;
  size?: "normal" | "small";
}) {
  const [asking, setAsking] = useState(false);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const apply = async (env: PlaidEnvironment, pw?: string) => {
    setBusy(true);
    setError(undefined);
    try {
      await api.plaidSetEnvironment(env, pw);
      setAsking(false);
      setPassword("");
      onChanged();
    } catch (e) {
      if (isApiError(e) && e.code === "password_required") {
        setAsking(true);
      } else {
        setError(errorMessage(e));
        if (!asking) window.setTimeout(() => setError(undefined), 6000);
      }
    } finally {
      setBusy(false);
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    apply("production", password);
  };

  return (
    <>
      <Segmented
        label="Plaid environment"
        value={environment}
        disabled={busy}
        onChange={(env) => apply(env)}
        options={[
          { value: "sandbox", label: size === "small" ? "Test" : "Sandbox", title: "Test banks with fake data" },
          { value: "production", label: size === "small" ? "Live" : "Production", title: "Your real banks" },
        ]}
      />
      {error && !asking ? <span className="text-xs text-red-600">{error}</span> : null}
      {asking ? (
        <Modal
          title="Switch to live bank data"
          onClose={() => {
            setAsking(false);
            setPassword("");
            setError(undefined);
          }}
        >
          <form onSubmit={submit} className="space-y-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              Production connects to your real banks. Anything you connect there counts against your Plaid Trial limit of
              10 institutions, and disconnecting doesn't give the slot back.
            </p>
            <p className="text-sm text-zinc-600 dark:text-zinc-400">
              Enter your master password to continue. It won't be asked again for 10 minutes.
            </p>
            <PasswordField
              label="Master password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoFocus
              error={error}
            />
            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="secondary"
                onClick={() => {
                  setAsking(false);
                  setPassword("");
                  setError(undefined);
                }}
              >
                Stay in Sandbox
              </Button>
              <Button type="submit" disabled={busy || !password}>
                {busy ? "Checking…" : "Switch to Production"}
              </Button>
            </div>
          </form>
        </Modal>
      ) : null}
    </>
  );
}
