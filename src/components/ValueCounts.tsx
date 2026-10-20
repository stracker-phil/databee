/** "Value counts" of a grid column: each stored value of the column with how often it occurs; a click filters by it. */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, type Cell, type ColumnEquals } from "../api";
import { formatCount } from "../lib/format";
import { useApiResult } from "../lib/useApiResult";
import { Value } from "./value/Value";

/** What Rust counts (`value_counts`), capped there too. */
const VALUE_COUNTS_LIMIT = 50;

export interface ValueCountsTarget {
  sessionId: string;
  database: string;
  table: string;
  column: string;
  /** The structured filter the counts stay inside (a table tab's `where`). */
  equals: ColumnEquals[];
  x: number;
  y: number;
}

interface Props {
  target: ValueCountsTarget;
  /** `null` filters on IS NULL. */
  onPick: (value: string | null) => void;
  /** Why a click cannot filter, shown on every row; picking is off while set. */
  pickRefusal?: string | null;
  onClose: () => void;
}

const EDGE = 4;

/** The stored text a filter compares with; previews (binary, long text) cannot filter. */
function filterText(cell: Cell): { value: string | null } | null {
  if (cell.t === "null") return { value: null };
  if (cell.t === "bytes" || cell.t === "longText") return null;
  return { value: cell.v };
}

function useCounts({ sessionId, database, table, column, equals }: ValueCountsTarget) {
  const key = JSON.stringify([sessionId, database, table, column, equals]);
  const { data: counts, error } = useApiResult(key, () => api.valueCounts(sessionId, database, table, column, equals, VALUE_COUNTS_LIMIT));
  return { counts, error };
}

export function ValueCounts({ target, onPick, pickRefusal, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const [pos, setPos] = useState({ left: target.x, top: target.y });
  const { counts, error } = useCounts(target);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPos({
      left: Math.max(EDGE, Math.min(target.x, window.innerWidth - width - EDGE)),
      top: Math.max(EDGE, Math.min(target.y, window.innerHeight - height - EDGE)),
    });
  }, [target.x, target.y, counts]);

  useEffect(() => {
    const closeOutside = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onCloseRef.current();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onCloseRef.current();
    window.addEventListener("mousedown", closeOutside);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", closeOutside);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const label = `Value counts of ${target.table}.${target.column}`;
  return (
    <div className="value-counts" role="dialog" aria-label={label} ref={ref} style={pos} data-stage="grid:value-counts" data-stage-label={label}>
      <div className="value-counts-head mono">
        <span className="tn">{target.table}</span>.{target.column}
        {counts && counts.length >= VALUE_COUNTS_LIMIT && <span className="o3"> · top {VALUE_COUNTS_LIMIT}</span>}
      </div>
      {error && <div className="form-error">{error.message}</div>}
      {!counts && !error && <div className="muted text-small">Counting…</div>}
      {counts?.length === 0 && <div className="muted text-small">No rows.</div>}
      {counts && counts.length > 0 && (
        <ul className="value-counts-list" role="listbox" aria-label="Values">
          {counts.map((c, i) => {
            const text = filterText(c.value.raw);
            const reason = pickRefusal ?? (text ? null : "Binary and long values cannot filter");
            const pick = () => {
              if (!text || reason) return;
              onClose();
              onPick(text.value);
            };
            return (
              <li
                key={i}
                role="option"
                aria-selected={false}
                aria-disabled={!!reason}
                tabIndex={reason ? -1 : 0}
                title={reason ?? "Filter by this value"}
                data-stage={`grid:value-count:${i}`}
                data-stage-list="grid:value-counts"
                onClick={(e) => {
                  // A secret's reveal or a tree's toggle inside the value is not a pick.
                  if (!(e.target as Element).closest("button")) pick();
                }}
                onKeyDown={(e) => e.key === "Enter" && pick()}
              >
                <span className="grow-text mono">
                  <Value value={c.value} density="grid" />
                </span>
                <span className="count">{formatCount(c.count)}</span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
