import { useState } from "react";
import { formatCents } from "../../lib/money";

const WIDTH = 640;
const HEIGHT = 200;
const PAD = { top: 16, right: 8, bottom: 24, left: 56 };

function niceCeil(value: number): number {
  if (value <= 0) return 0;
  const pow = 10 ** Math.floor(Math.log10(value));
  return Math.ceil(value / pow) * pow;
}

export interface ChartSeries {
  label: string;
  values: number[];
  /** Semantic color: the app only has "good" (green) and "over/bad" (red) viz tokens. */
  color: "line" | "over";
  dashed?: boolean;
}

/**
 * A line chart for one or two series over a shared x-axis (years), with an
 * optional shaded band (e.g. a Monte Carlo 10th-90th percentile range) drawn
 * behind them. Values are cents; hover shows the exact figures.
 */
export function LineChart({
  xLabels,
  series,
  band,
  hidden,
}: {
  xLabels: string[];
  series: ChartSeries[];
  band?: { low: number[]; high: number[] };
  hidden?: boolean;
}) {
  const [hover, setHover] = useState<number>();
  const n = xLabels.length;
  if (n < 2) return null;

  const all = series.flatMap((s) => s.values).concat(band ? [...band.low, ...band.high] : []);
  const max = niceCeil(Math.max(...all, 0));
  const min = Math.min(...all, 0) < 0 ? -niceCeil(Math.abs(Math.min(...all))) : 0;
  const span = max - min || 1;
  const plotW = WIDTH - PAD.left - PAD.right;
  const plotH = HEIGHT - PAD.top - PAD.bottom;
  const x = (i: number) => PAD.left + (i / (n - 1)) * plotW;
  const y = (v: number) => PAD.top + plotH - ((v - min) / span) * plotH;

  const path = (values: number[]) => values.map((v, i) => `${i === 0 ? "M" : "L"} ${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join(" ");
  const ticks = [max, min + span / 2, min].filter((v, i, a) => a.indexOf(v) === i);
  const step = Math.max(1, Math.ceil(n / 6));
  const colorVar = (c: "line" | "over") => (c === "line" ? "var(--viz-line)" : "var(--viz-over)");

  return (
    <div
      className="relative"
      onMouseLeave={() => setHover(undefined)}
      onMouseMove={(e) => {
        const box = e.currentTarget.getBoundingClientRect();
        const rel = ((e.clientX - box.left) / box.width) * WIDTH;
        const i = Math.round(((rel - PAD.left) / plotW) * (n - 1));
        setHover(Math.min(n - 1, Math.max(0, i)));
      }}
    >
      <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} className="w-full" role="img" aria-label={series.map((s) => s.label).join(" vs ")}>
        {ticks.map((t) => (
          <g key={t}>
            <line x1={PAD.left} x2={WIDTH - PAD.right} y1={y(t)} y2={y(t)} stroke="var(--viz-grid)" strokeWidth={1} />
            <text x={0} y={y(t) + 4} fontSize="11" fill="var(--viz-axis)">
              {hidden ? "•••" : formatCents(t).replace(/\.00$/, "")}
            </text>
          </g>
        ))}
        {xLabels.map((label, i) =>
          i % step === 0 || i === n - 1 ? (
            <text key={label} x={x(i)} y={HEIGHT - 6} fontSize="11" textAnchor="middle" fill="var(--viz-axis)">
              {label}
            </text>
          ) : null,
        )}

        {band ? (
          <path
            d={`${path(band.high)} ${band.low
              .map((v, i) => `L ${x(i).toFixed(1)} ${y(v).toFixed(1)}`)
              .reverse()
              .join(" ")} Z`}
            fill="var(--viz-line)"
            opacity={0.08}
          />
        ) : null}

        {series.map((s) => (
          <path
            key={s.label}
            d={path(s.values)}
            fill="none"
            stroke={colorVar(s.color)}
            strokeWidth={2}
            strokeDasharray={s.dashed ? "5 4" : undefined}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        ))}

        {hover !== undefined ? (
          <line x1={x(hover)} x2={x(hover)} y1={PAD.top} y2={HEIGHT - PAD.bottom} stroke="var(--viz-axis)" strokeWidth={1} />
        ) : null}
        {hover !== undefined
          ? series.map((s) => (
              <circle key={s.label} cx={x(hover)} cy={y(s.values[hover])} r={4} fill={colorVar(s.color)} stroke="var(--viz-surface)" strokeWidth={2} />
            ))
          : null}
      </svg>
      {hover !== undefined ? (
        <div
          className="pointer-events-none absolute -translate-x-1/2 space-y-0.5 rounded-lg bg-zinc-900 px-2 py-1 text-xs text-white shadow-lg dark:bg-zinc-100 dark:text-zinc-900"
          style={{ left: `${(x(hover) / WIDTH) * 100}%`, top: 0 }}
        >
          <div className="font-medium">{xLabels[hover]}</div>
          {series.map((s) => (
            <div key={s.label}>
              {s.label}: {hidden ? "•••••" : formatCents(s.values[hover])}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
