import { describe, expect, it } from "vitest";
import { estimateStrength, formatRecoveryInput } from "./password";

describe("estimateStrength", () => {
  it("flags short passwords", () => {
    expect(estimateStrength("abc").score).toBe(0);
  });
  it("rates a repeated character as weak", () => {
    expect(estimateStrength("aaaaaaaaaaaaaaaa").score).toBeLessThanOrEqual(1);
  });
  it("rates a long passphrase as strong", () => {
    expect(estimateStrength("maple river lantern 42 Copper").score).toBeGreaterThanOrEqual(3);
  });
});

describe("formatRecoveryInput", () => {
  it("groups into fours and strips junk", () => {
    expect(formatRecoveryInput("abcd efgh-ij")).toBe("ABCD-EFGH-IJ");
  });
  it("caps at 52 characters", () => {
    const out = formatRecoveryInput("A".repeat(80));
    expect(out.replace(/-/g, "")).toHaveLength(52);
  });
});
