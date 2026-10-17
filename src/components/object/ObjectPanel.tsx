import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import { columnLabel, columnName, shownColumns, type StackColumn } from "../../lib/columns";
import { documentText } from "../../lib/document";
import { useStore, type ObjectTab } from "../../state/store";
import { ColumnContext } from "../value/open";
import { ValueColumnView } from "../value/ValueColumnView";
import { ObjectColumnView } from "./ObjectColumnView";

/** The stack's width; a hidden tab measures 0, which keeps the last width so its columns stay mounted. */
function useWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(Infinity);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => el.offsetWidth > 0 && setWidth(el.offsetWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}

/** The active column takes the keyboard focus unless something shown outside the stack holds it. */
function useFocusActive(ref: RefObject<HTMLElement | null>, active: number, count: number, shown: boolean) {
  useEffect(() => {
    const stack = ref.current;
    if (!shown || !stack) return;
    const focused = document.activeElement;
    const free = !focused || focused === document.body || stack.contains(focused) || !!focused.closest("[hidden]");
    const column = stack.querySelector<HTMLElement>(`[data-column="${active}"]`);
    if (!free || !column || column.contains(focused)) return;
    column.focus({ preventScroll: true });
    column.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [ref, active, count, shown]);
}

const columnKey = (column: StackColumn, index: number) =>
  column.kind === "object" ? `${index}:${documentText(column.address)}` : `${index}:value:${column.value.source?.column ?? ""}`;

interface ColumnProps {
  tab: ObjectTab;
  column: StackColumn;
  index: number;
  shown: boolean;
}

function Strip({ tab, column, index }: ColumnProps) {
  const label = columnLabel(column);
  const { name, key } = columnName(column);
  const before = index < tab.active;
  const onClick = () => (before ? useStore.getState().popToColumn(tab.id, index) : useStore.getState().setActiveColumn(tab.id, index));
  return (
    <button className="strip" aria-label={`${before ? "Back to" : "Show"} ${label}`} data-stage={`strip:${index}`} data-stage-label={label} data-stage-list="strips" onClick={onClick}>
      <span className="strip-label">
        {name}
        {key !== null && <span className="key"> #{key}</span>}
      </span>
    </button>
  );
}

function Column({ tab, column, index, shown }: ColumnProps) {
  const place = useMemo(() => ({ tabId: tab.id, index }), [tab.id, index]);
  const active = index === tab.active;
  const classes = ["column", active && tab.columns.length > 1 ? "active" : "", column.kind === "value" ? "value-column" : ""].filter(Boolean).join(" ");
  const label = columnLabel(column);
  return (
    <ColumnContext.Provider value={place}>
      <section
        className={classes}
        aria-label={label}
        aria-current={active ? "true" : undefined}
        tabIndex={-1}
        data-column={index}
        data-stage={`column:${index}`}
        data-stage-label={label}
        data-stage-list="columns"
        onMouseDownCapture={() => useStore.getState().setActiveColumn(tab.id, index)}
      >
        {column.kind === "object" ? (
          <ObjectColumnView tab={tab} column={column} index={index} editable={shown && active} />
        ) : (
          <ValueColumnView column={column} onClose={() => useStore.getState().popToColumn(tab.id, index - 1)} onEdited={(text) => useStore.getState().updateValueColumn(tab.id, index, text)} />
        )}
      </section>
    </ColumnContext.Provider>
  );
}

export function ObjectPanel({ tab }: { tab: ObjectTab }) {
  const shown = useStore((s) => s.activeTabId === tab.id);
  const ref = useRef<HTMLDivElement>(null);
  const width = useWidth(ref);
  const count = tab.columns.length;
  const { start, end } = shownColumns(count, tab.active, width);
  useFocusActive(ref, tab.active, count, shown);
  return (
    <div className="stack" ref={ref} data-stage="object" data-stage-label={tab.title}>
      {tab.columns.map((column, index) => {
        const props = { tab, column, index, shown };
        const key = columnKey(column, index);
        return index < start || index >= end ? <Strip key={`strip:${key}`} {...props} /> : <Column key={key} {...props} />;
      })}
    </div>
  );
}
