import type { PlaidItem } from "./types";

/**
 * A connection that hasn't synced successfully in this long is flagged.
 *
 * Plaid refreshes a bank's data on its own schedule, roughly daily, and
 * banks often don't post over weekends and holidays — so a day or two of
 * quiet is normal. Three days catches a connection that has genuinely
 * stopped without crying wolf every Monday.
 */
export const STALE_AFTER_DAYS = 3;

export type ConnectionHealth =
  /** Syncing normally. */
  | { kind: "ok" }
  /** Plaid says the bank wants you to sign in again. */
  | { kind: "needs_reauth"; message: string | null }
  /** The last sync failed for some other reason. */
  | { kind: "error"; message: string | null }
  /** No error, but nothing has synced successfully in a while — often a
   *  connection that has gone stale on Plaid's side. Signing in again is
   *  the usual fix. `days` is null when it has never synced. */
  | { kind: "stale"; days: number | null };

/** Whole days since `iso`, or null if it has never happened. */
function daysSince(iso: string | null, now: Date): number | null {
  if (!iso) return null;
  return Math.floor((now.getTime() - new Date(iso).getTime()) / 86_400_000);
}

export function connectionHealth(item: PlaidItem, now = new Date()): ConnectionHealth {
  if (item.status === "needs_reauth") return { kind: "needs_reauth", message: item.lastError };
  if (item.status === "error") return { kind: "error", message: item.lastError };
  // A connection with no accounts chosen yet isn't expected to sync.
  if (item.linkedAccountCount === 0) return { kind: "ok" };
  const days = daysSince(item.lastSyncedAt, now);
  if (days === null || days >= STALE_AFTER_DAYS) return { kind: "stale", days };
  return { kind: "ok" };
}

export function needsAttention(item: PlaidItem, now = new Date()): boolean {
  return connectionHealth(item, now).kind !== "ok";
}
