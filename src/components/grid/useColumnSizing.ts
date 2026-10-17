import { useEffect, useMemo, useRef, useState } from "react";
import type { Cell, ColumnMeta } from "../../api";
import { selectNullLabel, useStore } from "../../state/store";
import { fitWidth } from "../gridHooks";

const MIN_WIDTH_REM = 3;
const MAX_FIT_REM = 40;

const remPx = () => parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;

interface Options {
  columns: ColumnMeta[];
  rows: Cell[][];
  /** Persisted widths by column name; missing columns are measured from the data. */
  saved?: Record<string, number>;
  onChange?: (widths: Record<string, number>) => void;
  /** The rows on screen, which auto-fit measures. */
  visibleRows: () => Cell[][];
}

/** Column widths: changed in this grid (drag / auto-fit), else saved, else measured from the first rows. */
export function useColumnSizing({ columns, rows, saved, onChange, visibleRows }: Options) {
  const nullLabel = useStore(selectNullLabel);
  const dataFontSize = useStore((s) => s.settings?.dataFontSize ?? 12);
  const [manual, setManual] = useState<Record<string, number>>({});
  const stopDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(), []);

  const measured = useMemo(() => columns.map((c, i) => fitWidth(c, i, rows.slice(0, 50), nullLabel, dataFontSize, 380)), [columns, rows, nullLabel, dataFontSize]);
  const widths = columns.map((c, i) => manual[c.name] ?? saved?.[c.name] ?? measured[i]);
  const totalWidth = widths.reduce((a, b) => a + b, 0);
  // Per-column CSS variables on the grid root; cells only reference them.
  const colVars = Object.fromEntries(widths.map((w, i) => [`--col-${i}`, `${w}px`])) as React.CSSProperties;

  const commit = (next: Record<string, number>) => {
    setManual(next);
    onChange?.(Object.fromEntries(columns.map((c, i) => [c.name, next[c.name] ?? widths[i]])));
  };

  const startResize = (e: React.MouseEvent, index: number) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startWidth = widths[index];
    const min = MIN_WIDTH_REM * remPx();
    const name = columns[index].name;
    let latest = startWidth;
    const move = (ev: MouseEvent) => {
      latest = Math.max(min, Math.round(startWidth + ev.clientX - startX));
      setManual((m) => ({ ...m, [name]: latest }));
    };
    const stop = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      stopDrag.current = null;
    };
    const up = () => {
      stop();
      commit({ ...manual, [name]: latest });
    };
    stopDrag.current = stop;
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const autoFit = (index: number) => {
    const visible = visibleRows();
    const sample = visible.length ? visible : rows.slice(0, 50);
    const width = fitWidth(columns[index], index, sample, nullLabel, dataFontSize, MAX_FIT_REM * remPx());
    commit({ ...manual, [columns[index].name]: Math.max(MIN_WIDTH_REM * remPx(), width) });
  };

  return { totalWidth, colVars, startResize, autoFit };
}
