import { Fragment, useEffect, useMemo, useRef, type RefObject } from "react";
import { columnLabel, DIVIDER_WIDTH, maximizable, MIN_PANE_WIDTH, paneShares, shownColumns, type StackColumn } from "../../lib/columns";
import { documentText } from "../../lib/document";
import { selectMaxPanes, useStore, type ObjectTab } from "../../state/store";
import { Splitter } from "../Splitter";
import { ColumnContext } from "../value/open";
import { ValueColumnView } from "../value/ValueColumnView";
import { ObjectColumnView } from "./ObjectColumnView";
import { QuietPaneContext } from "./ObjectHeader";
import { PaneContext } from "./PaneMaximize";
import { Trail } from "./Trail";
import { useWidth } from "./useWidth";

/** The active column takes the keyboard focus, also after the panes shown changed, unless something shown outside the panel holds it. */
function useFocusActive(ref: RefObject<HTMLElement | null>, active: number, count: number, shown: boolean, start: number, end: number) {
  useEffect(() => {
    const panel = ref.current;
    if (!shown || !panel) return;
    const focused = document.activeElement;
    const free = !focused || focused === document.body || panel.contains(focused) || !!focused.closest("[hidden]");
    const column = panel.querySelector<HTMLElement>(`[data-column="${active}"]`);
    if (!free || !column || column.contains(focused)) return;
    column.focus({ preventScroll: true });
    column.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [ref, active, count, shown, start, end]);
}

const columnKey = (column: StackColumn, index: number) =>
  column.kind === "object" ? `${index}:${documentText(column.address)}` : `${index}:value:${column.value.source?.column ?? ""}`;

interface ColumnProps {
  tab: ObjectTab;
  column: StackColumn;
  index: number;
  shown: boolean;
  /** Its share of the panes' width. */
  share: number;
  /** Several panes show, so this one is marked when active and keeps a minimum width. */
  beside: boolean;
  canMaximize: boolean;
}

function Column({ tab, column, index, shown, share, beside, canMaximize }: ColumnProps) {
  const place = useMemo(() => ({ tabId: tab.id, index }), [tab.id, index]);
  const maximized = !!tab.maximized;
  const pane = useMemo(() => (canMaximize ? { tabId: tab.id, index, maximized } : null), [canMaximize, tab.id, index, maximized]);
  const active = index === tab.active;
  const quiet = beside && !active;
  const classes = ["column", active && beside ? "active" : "", quiet ? "quiet" : "", column.kind === "value" ? "value-column" : ""].filter(Boolean).join(" ");
  const label = columnLabel(column);
  return (
    <ColumnContext.Provider value={place}>
      <PaneContext.Provider value={pane}>
        <QuietPaneContext.Provider value={quiet}>
          <section
            className={classes}
            style={{ flexGrow: share, minWidth: beside ? MIN_PANE_WIDTH : undefined }}
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
              <ValueColumnView column={column} place={place} onClose={() => useStore.getState().popToColumn(tab.id, index - 1)} />
            )}
          </section>
        </QuietPaneContext.Provider>
      </PaneContext.Provider>
    </ColumnContext.Provider>
  );
}

interface DividerProps {
  tab: ObjectTab;
  shares: number[];
  /** The pane after the divider, counted from the first shown one. */
  at: number;
  /** The column index of the first shown pane. */
  start: number;
  /** The panes' width in px, without the dividers. */
  space: number;
  stack: RefObject<HTMLElement | null>;
}

/** Resizes the two panes next to it; while dragging only their flex grows change, the store takes the result on release. */
function PaneDivider({ tab, shares, at, start, space, stack }: DividerProps) {
  const pair = shares[at - 1] + shares[at];
  const sharesAt = (px: number) => shares.map((share, i) => (i === at - 1 ? px / space : i === at ? pair - px / space : share));
  const onResize = (px: number) => {
    const next = sharesAt(px);
    for (const i of [at - 1, at]) {
      const pane = stack.current?.querySelector<HTMLElement>(`[data-column="${start + i}"]`);
      if (pane) pane.style.flexGrow = String(next[i]);
    }
  };
  const left = columnLabel(tab.columns[start + at - 1]);
  const right = columnLabel(tab.columns[start + at]);
  const { setPaneWidths } = useStore.getState();
  return (
    <Splitter
      axis="x"
      paneAfter={false}
      size={Math.round(shares[at - 1] * space)}
      min={MIN_PANE_WIDTH}
      max={Math.round(pair * space) - MIN_PANE_WIDTH}
      onResize={onResize}
      onCommit={(px) => setPaneWidths(tab.id, sharesAt(px))}
      onReset={() => setPaneWidths(tab.id, null)}
      label={`Resize ${left} and ${right}`}
      stage={`pane-divider:${start + at - 1}`}
    />
  );
}

/** Pane shares belong to one number of panes; when that changes they start equal again. */
function useResetShares(tab: ObjectTab, panes: number, maximized: boolean) {
  const stale = !maximized && !!tab.paneWidths && tab.paneWidths.length !== panes;
  useEffect(() => {
    if (stale) useStore.getState().setPaneWidths(tab.id, null);
  }, [stale, tab.id]);
}

export function ObjectPanel({ tab }: { tab: ObjectTab }) {
  const shown = useStore((s) => s.activeTabId === tab.id);
  const maxPanes = useStore(selectMaxPanes);
  const panel = useRef<HTMLDivElement>(null);
  const stack = useRef<HTMLDivElement>(null);
  const width = useWidth(stack);
  const count = tab.columns.length;
  const maximized = !!tab.maximized && count > 1;
  const { start, end } = maximized ? { start: tab.active, end: tab.active + 1 } : shownColumns(count, tab.active, width, maxPanes);
  const panes = end - start;
  const shares = paneShares(maximized ? undefined : tab.paneWidths, panes);
  const space = width - (panes - 1) * DIVIDER_WIDTH;
  const canMaximize = count > 1 && maximizable(tab, maxPanes);
  useResetShares(tab, panes, maximized);
  useFocusActive(panel, tab.active, count, shown, start, end);
  return (
    <div className="object-panel" ref={panel} data-stage="object" data-stage-label={tab.title}>
      {count > 0 && <Trail tab={tab} start={start} end={end} />}
      <div className="stack" ref={stack}>
        {tab.columns.slice(start, end).map((column, i) => {
          const index = start + i;
          return (
            <Fragment key={columnKey(column, index)}>
              {i > 0 && Number.isFinite(space) && <PaneDivider tab={tab} shares={shares} at={i} start={start} space={space} stack={stack} />}
              <Column tab={tab} column={column} index={index} shown={shown} share={shares[i]} beside={panes > 1} canMaximize={canMaximize} />
            </Fragment>
          );
        })}
      </div>
    </div>
  );
}
