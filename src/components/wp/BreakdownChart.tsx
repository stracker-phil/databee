import { useMemo, useState, type KeyboardEvent } from "react";
import { ChevronRight } from "lucide-react";

export interface BreakdownItem {
  id: string;
  /** Human name, e.g. "Rewrite rules · core". */
  label: string;
  value: number;
  sublabel?: string;
  onOpen?: () => void;
}

export interface BreakdownChartProps {
  /** Full list; the chart shows the top rows and folds the rest into "Other". */
  items: BreakdownItem[];
  format: (v: number) => string;
  /** Word after the total in the donut center, e.g. "autoloaded". */
  totalLabel: string;
  modes?: { id: string; label: string }[];
  mode?: string;
  onModeChange?: (id: string) => void;
  /** Named rows before "Other" (4–7). */
  maxRows?: number;
}

const OTHER = "__other";
const COLORS = ["var(--category-1)", "var(--category-2)", "var(--category-3)", "var(--category-4)", "var(--category-5)", "var(--category-6)", "var(--category-7)"];

interface Row extends BreakdownItem {
  color: string;
  children?: BreakdownItem[];
}

/** Top N rows plus one "Other (n)" row holding the remainder. */
export function splitRows(items: BreakdownItem[], maxRows: number): Row[] {
  const sorted = [...items].filter((i) => i.value > 0).sort((a, b) => b.value - a.value);
  const n = Math.max(4, Math.min(7, maxRows));
  if (sorted.length <= n) return sorted.map((i, k) => ({ ...i, color: COLORS[k % COLORS.length] }));
  const named = sorted.slice(0, n);
  const rest = sorted.slice(n);
  return [
    ...named.map((i, k) => ({ ...i, color: COLORS[k % COLORS.length] })),
    { id: OTHER, label: `Other (${rest.length})`, value: rest.reduce((s, i) => s + i.value, 0), color: "var(--category-other)", children: rest },
  ];
}

function arcPath(cx: number, cy: number, rOuter: number, rInner: number, start: number, end: number): string {
  const a0 = start * 2 * Math.PI - Math.PI / 2;
  const a1 = end * 2 * Math.PI - Math.PI / 2;
  const large = end - start > 0.5 ? 1 : 0;
  const p = (r: number, a: number) => `${(cx + r * Math.cos(a)).toFixed(3)} ${(cy + r * Math.sin(a)).toFixed(3)}`;
  return `M ${p(rOuter, a0)} A ${rOuter} ${rOuter} 0 ${large} 1 ${p(rOuter, a1)} L ${p(rInner, a1)} A ${rInner} ${rInner} 0 ${large} 0 ${p(rInner, a0)} Z`;
}

export function BreakdownChart({ items, format, totalLabel, modes, mode, onModeChange, maxRows = 7 }: BreakdownChartProps) {
  const [drilled, setDrilled] = useState(false);
  const [hover, setHover] = useState<string | null>(null);

  const top = useMemo(() => splitRows(items, maxRows), [items, maxRows]);
  const other = top.find((r) => r.id === OTHER);
  const rows = drilled && other?.children ? splitRows(other.children, maxRows) : top;
  const total = rows.reduce((s, r) => s + r.value, 0);
  const max = rows[0]?.value ?? 1;
  const hovered = rows.find((r) => r.id === hover) ?? null;

  const activate = (row: Row) => {
    if (row.id === OTHER) setDrilled(true);
    else row.onOpen?.();
  };
  const onKey = (e: KeyboardEvent, row: Row) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      activate(row);
    }
  };

  let acc = 0;
  const slices = rows.map((r) => {
    const start = acc / (total || 1);
    acc += r.value;
    return { row: r, start, end: acc / (total || 1) };
  });

  return (
    <div className="breakdown">
      {(modes || drilled) && (
        <div className="breakdown-tools">
          {drilled ? (
            <nav className="crumbs" aria-label="Breakdown level">
              <span className="crumb"><button onClick={() => (setDrilled(false), setHover(null))}>All</button></span>
              <span className="crumb current"><ChevronRight size="0.846rem" /><span>Other</span></span>
            </nav>
          ) : (
            <span />
          )}
          {modes && (
            <div className="segmented" role="group" aria-label="Breakdown mode">
              {modes.map((m) => (
                <button key={m.id} aria-pressed={mode === m.id} onClick={() => (onModeChange?.(m.id), setDrilled(false))}>
                  {m.label}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
      <div className="breakdown-body">
        <svg className="donut" viewBox="0 0 100 100" role="img" aria-label={`${format(total)} ${totalLabel}`}>
          {rows.length === 1 ? (
            <circle cx={50} cy={50} r={37} fill="none" stroke={rows[0].color} strokeWidth={18} data-id={rows[0].id} />
          ) : (
            slices.map(({ row, start, end }) => (
              <path
                key={row.id}
                d={arcPath(50, 50, 46, 28, start, Math.max(end, start + 0.0005))}
                fill={row.color}
                data-id={row.id}
                className={`donut-slice${hover && hover !== row.id ? " dim" : ""}${hover === row.id ? " hot" : ""}`}
                onMouseEnter={() => setHover(row.id)}
                onMouseLeave={() => setHover(null)}
                onClick={() => activate(row)}
              >
                <title>{`${row.label} · ${format(row.value)}`}</title>
              </path>
            ))
          )}
          <text x={50} y={hovered ? 47 : 48} textAnchor="middle" className="donut-value">
            {format(hovered ? hovered.value : total)}
          </text>
          <text x={50} y={hovered ? 57 : 58} textAnchor="middle" className="donut-label">
            {hovered ? hovered.label.slice(0, 18) : totalLabel}
          </text>
        </svg>
        <ul className="breakdown-list" role="list">
          {rows.map((r) => (
            <li key={r.id}>
              <button
                className={`breakdown-row${hover === r.id ? " hot" : ""}`}
                data-id={r.id}
                aria-label={`${r.label}: ${format(r.value)}`}
                onMouseEnter={() => setHover(r.id)}
                onMouseLeave={() => setHover(null)}
                onFocus={() => setHover(r.id)}
                onBlur={() => setHover(null)}
                onClick={() => activate(r)}
                onKeyDown={(e) => onKey(e, r)}
              >
                <i className="swatch" style={{ background: r.color }} />
                <span className="breakdown-name">
                  <span className="breakdown-text">
                    <span className="grow-text">{r.label}</span>
                    {r.sublabel && <span className="muted small breakdown-sub">{r.sublabel}</span>}
                  </span>
                  <span className="breakdown-bar"><span style={{ width: `${(r.value / (max || 1)) * 100}%`, background: r.color }} /></span>
                </span>
                <span className="breakdown-value">{format(r.value)}</span>
                <span className="breakdown-pct muted">{total ? `${Math.round((r.value / total) * 100)}%` : ""}</span>
              </button>
            </li>
          ))}
          {rows.length === 0 && <li className="muted">Nothing to show.</li>}
        </ul>
      </div>
    </div>
  );
}
