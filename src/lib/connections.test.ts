import { describe, expect, it } from "vitest";
import { STALE_AFTER_DAYS, connectionHealth, needsAttention } from "./connections";
import type { PlaidItem } from "./types";

const NOW = new Date("2026-10-14T12:00:00Z");

function item(patch: Partial<PlaidItem> = {}): PlaidItem {
  return {
    id: 1,
    institutionName: "BMO",
    status: "active",
    lastError: null,
    lastSyncedAt: "2026-10-14T08:00:00Z",
    products: ["transactions"],
    environment: "production",
    linkedAccountCount: 2,
    createdAt: "2026-08-01T00:00:00Z",
    ...patch,
  };
}

function daysAgo(days: number): string {
  return new Date(NOW.getTime() - days * 86_400_000).toISOString();
}

describe("connectionHealth", () => {
  it("is ok when it synced recently", () => {
    expect(connectionHealth(item(), NOW)).toEqual({ kind: "ok" });
    expect(needsAttention(item(), NOW)).toBe(false);
  });

  it("tolerates a quiet weekend", () => {
    expect(connectionHealth(item({ lastSyncedAt: daysAgo(STALE_AFTER_DAYS - 1) }), NOW).kind).toBe("ok");
  });

  it("flags a connection that has quietly stopped syncing", () => {
    // The BMO case: no error, status still "active", just nothing landing.
    const h = connectionHealth(item({ lastSyncedAt: daysAgo(9) }), NOW);
    expect(h).toEqual({ kind: "stale", days: 9 });
    expect(needsAttention(item({ lastSyncedAt: daysAgo(9) }), NOW)).toBe(true);
  });

  it("flags one that has never synced", () => {
    expect(connectionHealth(item({ lastSyncedAt: null }), NOW)).toEqual({ kind: "stale", days: null });
  });

  it("reports what Plaid said when sign-in is needed or a sync failed", () => {
    expect(connectionHealth(item({ status: "needs_reauth", lastError: "Sign in again" }), NOW)).toEqual({
      kind: "needs_reauth",
      message: "Sign in again",
    });
    expect(connectionHealth(item({ status: "error", lastError: "Bank unavailable" }), NOW)).toEqual({
      kind: "error",
      message: "Bank unavailable",
    });
  });

  it("an explicit status wins over a recent sync time", () => {
    // Plaid can mark an item for re-auth right after a good sync.
    expect(connectionHealth(item({ status: "needs_reauth" }), NOW).kind).toBe("needs_reauth");
  });

  it("doesn't nag about a connection with no accounts chosen yet", () => {
    expect(connectionHealth(item({ linkedAccountCount: 0, lastSyncedAt: null }), NOW).kind).toBe("ok");
  });
});
