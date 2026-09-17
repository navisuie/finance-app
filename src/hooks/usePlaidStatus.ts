import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import type { PlaidStatus } from "../lib/types";

const EVENT = "finance:env-changed";

/** Tell every screen the Plaid environment changed. */
export function announceEnvironmentChange() {
  window.dispatchEvent(new Event(EVENT));
}

/** Plaid status, kept in step across screens when the environment changes. */
export function usePlaidStatus() {
  const [status, setStatus] = useState<PlaidStatus>();
  const load = useCallback(() => {
    api
      .plaidStatus()
      .then(setStatus)
      .catch(() => setStatus(undefined));
  }, []);

  useEffect(() => {
    load();
    window.addEventListener(EVENT, load);
    return () => window.removeEventListener(EVENT, load);
  }, [load]);

  return { status, reload: load };
}
