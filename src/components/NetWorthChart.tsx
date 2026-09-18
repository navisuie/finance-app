import { useState } from "react";
import { formatCents } from "../lib/money";
import type { NetWorthPoint } from "../lib/types";

const WIDTH = 720;
const HEIGHT = 200;
const PAD = { top: 16, right: 64, bottom: 24, left: 8 };

function monthLabel(month: string, long = false): string {
  const [y, m] = month.split("-").map(Number);
  return new Date(y, m - 1, 1).toLocaleDateString("en-US", {
    month: "short",
    year: long || m === 1 ? "numeric" : undefined,
  });
}

/** Round a value up to a readable axis step. */
function niceCeil(value: number): number {
  if (value <= 0) return 0;
  const pow = 10 ** Math.floor(Math.log10(value));
  return Math.ceil(value / pow) * pow;
}

/**
 * Net worth month by month: one series, so no legend — the title says what it
 * is. Hover for the exact figure, or open the numbers underneath.
 */
export function NetWorthChart({ points, hidden }: { points: NetWorthPoint[]; hidden?: boolean }) {
  const [hover, setHover] = useState<number>();
  const [showTable, setShowTable] = useState(false);

  const data = points.filter((p) => p.hasData);
  if (data.length < 2) {
    return (
      <p className="text-sm text-zinc-500">
        Two months of balances draw the line. Update an account's balance each month and this fills in.
      </p>
    );
  }

  const values = data.map((p) => p.netCents);
  const max = niceCeil(Math.max(...values, 0));
  const min = Math.min(...values, 0) < 0 ? -niceCeil(Math.abs(Math.min(...values))) : 0;
  const span = max - min || 1;
  const plotW = WIDTH - PAD.left - PAD.right;
  const plotH = HEIGHT - PAD.top - PAD.bottom;
  const x = (i: number) => PAD.left + (data.length === 1 ? plotW / 2 : (i / (data.length - 1)) * plotW);
  const y = (cents: number) => PAD.top + plotH - ((cents - min) / span) * plotH;

  const line = data.map((p, i) => `${i === 0 ? "M" : "L"} ${x(i).toFixed(1)} ${y(p.netCents).toFixed(1)}`).join(" ");
  const area = `${line} L ${x(data.length - 1).toFixed(1)} ${y(min)} L ${x(0).toFixed(1)} ${y(min)} Z`;
  const ticks = [max, min + span / 2, min].filter((v, i, a) => a.indexOf(v) === i);
  const last = data[data.length - 1];
  const active = hover !== undefined ? data[hover] : undefined;
  // Label a handful of months, never all of them.
  const step = Math.max(1, Math.ceil(data.length / 6));

  return (
    <div className="space-y-2">
      <div
        className="relative"
        onMouseLeave={() => setHover(undefined)}
        onMouseMove={(e) => {
          const box = e.currentTarget.getBoundingClientRect();
          const rel = ((e.clientX - box.left) / box.width) * WIDTH;
          const i = Math.round(((rel - PAD.left) / plotW) * (data.length - 1));
          setHover(Math.min(data.length - 1, Math.max(0, i)));
        }}
      >
        <svg
          viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
          className="w-full"
          role="img"
          aria-label={`Net worth from ${monthLabel(data[0].month, true)} to ${monthLabel(last.month, true)}, latest ${formatCents(last.netCents)}`}
        >
          {ticks.map((t) => (
            <g key={t}>
              <line
                x1={PAD.left}
                x2={WIDTH - PAD.right}
                y1={y(t)}
                y2={y(t)}
                stroke="var(--viz-grid)"
                strokeWidth={1}
              />
              <text x={WIDTH - PAD.right + 8} y={y(t) + 4} fontSize="11" fill="var(--viz-axis)">
                {hidden ? "•••" : formatCents(t).replace(/\.00$/, "")}
              </text>
            </g>
          ))}
          {data.map((p, i) =>
            i % step === 0 || i === data.length - 1 ? (
              <text key={p.month} x={x(i)} y={HEIGHT - 6} fontSize="11" textAnchor="middle" fill="var(--viz-axis)">
                {monthLabel(p.month)}
              </text>
            ) : null,
          )}

          <path d={area} fill="var(--viz-line)" opacity={0.1} />
          <path d={line} fill="none" stroke="var(--viz-line)" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" />

          {active ? (
            <line
              x1={x(hover!)}
              x2={x(hover!)}
              y1={PAD.top}
              y2={HEIGHT - PAD.bottom}
              stroke="var(--viz-axis)"
              strokeWidth={1}
            />
          ) : null}
          {[{ point: last, i: data.length - 1 }, ...(active ? [{ point: active, i: hover! }] : [])].map(
            ({ point, i }) => (
              <circle
                key={i}
                cx={x(i)}
                cy={y(point.netCents)}
                r={5}
                fill="var(--viz-line)"
                stroke="var(--viz-surface)"
                strokeWidth={2}
              />
            ),
          )}
        </svg>
        {active ? (
          <div
            className="pointer-events-none absolute -translate-x-1/2 rounded-lg bg-zinc-900 px-2 py-1 text-xs text-white shadow-lg dark:bg-zinc-100 dark:text-zinc-900"
            style={{ left: `${(x(hover!) / WIDTH) * 100}%`, top: 0 }}
          >
            {monthLabel(active.month, true)} · {hidden ? "•••••" : formatCents(active.netCents)}
          </div>
        ) : null}
      </div>

      <button
        className="text-xs text-zinc-500 hover:underline"
        onClick={() => setShowTable((v) => !v)}
        aria-expanded={showTable}
      >
        {showTable ? "Hide the numbers" : "Show the numbers"}
      </button>
      {showTable ? (
        <table className="w-full text-sm">
          <thead className="text-left text-xs text-zinc-500">
            <tr>
              <th className="py-1 font-medium">Month</th>
              <th className="py-1 text-right font-medium">Assets</th>
              <th className="py-1 text-right font-medium">Debts</th>
              <th className="py-1 text-right font-medium">Net worth</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {[...data].reverse().map((p) => (
              <tr key={p.month}>
                <td className="py-1 text-zinc-500">{monthLabel(p.month, true)}</td>
                <td className="py-1 text-right tabular-nums">{hidden ? "•••••" : formatCents(p.assetsCents)}</td>
                <td className="py-1 text-right tabular-nums">{hidden ? "•••••" : formatCents(p.liabilitiesCents)}</td>
                <td className="py-1 text-right font-medium tabular-nums">
                  {hidden ? "•••••" : formatCents(p.netCents)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
    </div>
  );
}
