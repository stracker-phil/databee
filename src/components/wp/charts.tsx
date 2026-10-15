import { useState } from "react";
import type { DayBucket } from "../../api";

const SERIES: { key: keyof Omit<DayBucket, "date">; label: string; color: string }[] = [
  { key: "completed", label: "completed", color: "var(--chart-green)" },
  { key: "processing", label: "processing", color: "var(--chart-blue)" },
  { key: "other", label: "other", color: "var(--chart-gray)" },
  { key: "failed", label: "failed", color: "var(--chart-red)" },
];

export function StackedBars({ days, labels }: { days: DayBucket[]; labels?: Partial<Record<string, string>> }) {
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(1, ...days.map((d) => d.completed + d.processing + d.failed + d.other));
  const width = 100 / Math.max(days.length, 1);
  const active = hover != null ? days[hover] : null;
  return (
    <div className="bars">
      <svg viewBox="0 0 100 50" preserveAspectRatio="none" className="bars-svg" role="img" aria-label="Per-day counts">
        {days.map((d, i) => {
          let y = 50;
          return (
            <g key={d.date} onMouseEnter={() => setHover(i)} onMouseLeave={() => setHover(null)}>
              <rect x={i * width} y={0} width={width} height={50} fill="transparent" />
              {SERIES.map((s) => {
                const h = (d[s.key] / max) * 46;
                y -= h;
                return h > 0 ? (
                  <rect key={s.key} x={i * width + width * 0.15} y={y} width={width * 0.7} height={h} fill={s.color} rx={0.6} opacity={hover == null || hover === i ? 1 : 0.55} />
                ) : null;
              })}
            </g>
          );
        })}
      </svg>
      <div className="bars-legend">
        {active ? (
          <span>
            <strong>{active.date}</strong>{" "}
            {SERIES.filter((s) => active[s.key] > 0).map((s) => `${active[s.key]} ${labels?.[s.key] ?? s.label}`).join(" · ") || "nothing"}
          </span>
        ) : (
          SERIES.map((s) => (
            <span key={s.key} className="legend-item">
              <i style={{ background: s.color }} /> {labels?.[s.key] ?? s.label}
            </span>
          ))
        )}
      </div>
    </div>
  );
}

export function Segments({ parts }: { parts: { label: string; value: number; color: string }[] }) {
  const total = parts.reduce((s, p) => s + p.value, 0) || 1;
  return (
    <div className="segments" role="img" aria-label={parts.map((p) => `${p.value} ${p.label}`).join(", ")}>
      {parts
        .filter((p) => p.value > 0)
        .map((p) => (
          <span key={p.label} style={{ width: `${(p.value / total) * 100}%`, background: p.color }} title={`${p.value} ${p.label}`} />
        ))}
    </div>
  );
}
