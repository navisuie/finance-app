const fmt = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });

export function formatCents(cents: number, opts: { signed?: boolean } = {}): string {
  const s = fmt.format(Math.abs(cents) / 100);
  if (cents < 0) return `-${s}`;
  if (opts.signed && cents > 0) return `+${s}`;
  return s;
}

/**
 * Parse what a person types into a money field ("1,234.56", "$12", "-4.5",
 * "(3.00)") into integer cents. Returns null when it isn't a valid amount.
 * Mirrors money::parse_cents in Rust; the backend re-validates anyway.
 */
export function parseMoney(input: string): number | null {
  let s = input.trim();
  if (!s) return null;
  let negative = false;
  if (s.startsWith("(") && s.endsWith(")")) {
    negative = true;
    s = s.slice(1, -1);
  }
  s = s.replace(/[,$\s]/g, "");
  if (s.startsWith("-")) {
    negative = !negative;
    s = s.slice(1);
  } else if (s.startsWith("+")) {
    s = s.slice(1);
  }
  const m = /^(\d*)(?:\.(\d{0,2}))?$/.exec(s);
  if (!m || (m[1] === "" && (m[2] === undefined || m[2] === ""))) return null;
  const whole = m[1] === "" ? 0 : Number(m[1]);
  const frac = m[2] ? Number(m[2].padEnd(2, "0")) : 0;
  const cents = whole * 100 + frac;
  if (!Number.isSafeInteger(cents) || cents > 1_000_000_000_000) return null;
  return negative ? -cents : cents;
}

/** Cents to an editable string without currency symbol, e.g. "1234.50". */
export function centsToInput(cents: number): string {
  const sign = cents < 0 ? "-" : "";
  const abs = Math.abs(cents);
  return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
}

function iso(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function todayISO(now = new Date()): string {
  return iso(now);
}

/** First and last day of the month containing `now`. */
export function monthRange(now = new Date()): { from: string; to: string } {
  const first = new Date(now.getFullYear(), now.getMonth(), 1);
  const last = new Date(now.getFullYear(), now.getMonth() + 1, 0);
  return { from: iso(first), to: iso(last) };
}

export function formatDate(isoDate: string): string {
  const [y, m, d] = isoDate.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}
