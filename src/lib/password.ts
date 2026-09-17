/**
 * Client-side strength hint for the setup form. The Rust core enforces the
 * real policy; this only gives quick feedback while typing.
 */
export const MIN_LENGTH = 12;

export type Strength = { score: 0 | 1 | 2 | 3 | 4; label: string };

export function estimateStrength(pw: string): Strength {
  if (pw.length === 0) return { score: 0, label: "" };
  if (pw.length < MIN_LENGTH) return { score: 0, label: `At least ${MIN_LENGTH} characters` };

  let pool = 0;
  if (/[a-z]/.test(pw)) pool += 26;
  if (/[A-Z]/.test(pw)) pool += 26;
  if (/[0-9]/.test(pw)) pool += 10;
  if (/[^A-Za-z0-9]/.test(pw)) pool += 33;
  const unique = new Set(pw).size;
  // Rough entropy estimate, penalising repeated characters.
  const bits = Math.log2(Math.max(pool, 2)) * Math.min(pw.length, unique * 2);

  if (bits < 50) return { score: 1, label: "Weak" };
  if (bits < 70) return { score: 2, label: "Fair" };
  if (bits < 90) return { score: 3, label: "Strong" };
  return { score: 4, label: "Very strong" };
}

/** Normalise a typed recovery key for display while typing. */
export function formatRecoveryInput(raw: string): string {
  const clean = raw.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 52);
  return clean.match(/.{1,4}/g)?.join("-") ?? "";
}
