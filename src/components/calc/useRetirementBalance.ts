import { useMemo } from "react";
import { useAsync } from "../../hooks/useAsync";
import { api } from "../../lib/api";
import { KIND_LABELS } from "../../lib/accountKinds";
import type { AccountKind } from "../../lib/types";

/** What the retirement calculators mean by "current balance": 401(k) + Roth
 * IRA + brokerage + HYSA. Traditional IRA and HSA are deliberately left out —
 * they're not what this input is meant to sum. */
const KINDS: AccountKind[] = ["401k", "roth_ira", "brokerage", "hysa"];

export function useRetirementBalance() {
  const accounts = useAsync(() => api.listAccounts(false), []);
  return useMemo(() => {
    const matched = (accounts.data ?? []).filter((a) => !a.archived && KINDS.includes(a.kind));
    const totalCents = matched.reduce((sum, a) => sum + (a.balanceCents ?? 0), 0);
    return {
      totalCents,
      count: matched.length,
      loading: accounts.loading,
      label: KINDS.map((k) => KIND_LABELS[k]).join(", "),
    };
  }, [accounts.data, accounts.loading]);
}
