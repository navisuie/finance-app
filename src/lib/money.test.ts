import { describe, expect, it } from "vitest";
import { centsToInput, formatCents, monthRange, parseMoney, todayISO } from "./money";

describe("parseMoney", () => {
  it.each([
    ["1,234.56", 123456],
    ["$12", 1200],
    ["-4.5", -450],
    ["(3.00)", -300],
    [".99", 99],
    ["+7", 700],
  ])("parses %s", (input, cents) => {
    expect(parseMoney(input)).toBe(cents);
  });

  it.each(["", "abc", "1.234", "1.2.3", "$", "--5", "."])("rejects %s", (input) => {
    expect(parseMoney(input)).toBeNull();
  });
});

describe("formatting", () => {
  it("formats cents", () => {
    expect(formatCents(123456)).toBe("$1,234.56");
    expect(formatCents(-5)).toBe("-$0.05");
    expect(formatCents(500, { signed: true })).toBe("+$5.00");
  });
  it("round-trips input", () => {
    expect(centsToInput(-123405)).toBe("-1234.05");
    expect(parseMoney(centsToInput(-123405))).toBe(-123405);
  });
  it("computes month ranges", () => {
    expect(monthRange(new Date(2026, 1, 10))).toEqual({ from: "2026-02-01", to: "2026-02-28" });
    expect(todayISO(new Date(2026, 8, 5))).toBe("2026-09-05");
  });
});
