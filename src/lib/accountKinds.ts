import type { AccountKind } from "./types";

export const KIND_LABELS: Record<AccountKind, string> = {
  checking: "Checking",
  savings: "Savings",
  hysa: "High-yield savings",
  brokerage: "Brokerage",
  "401k": "401(k)",
  roth_ira: "Roth IRA",
  traditional_ira: "Traditional IRA",
  hsa: "HSA",
  credit_card: "Credit card",
  loan: "Loan",
  mortgage: "Mortgage",
  other: "Other",
};

export const GROUPS: { title: string; kinds: AccountKind[] }[] = [
  { title: "Cash", kinds: ["checking", "savings", "hysa"] },
  { title: "Investments", kinds: ["brokerage"] },
  { title: "Retirement", kinds: ["401k", "roth_ira", "traditional_ira", "hsa"] },
  { title: "Debt", kinds: ["credit_card", "loan", "mortgage"] },
  { title: "Other", kinds: ["other"] },
];

export const KIND_OPTIONS = (Object.keys(KIND_LABELS) as AccountKind[]).map((k) => ({
  value: k,
  label: KIND_LABELS[k],
}));

export function defaultIsAsset(kind: AccountKind): boolean {
  return !["credit_card", "loan", "mortgage"].includes(kind);
}
