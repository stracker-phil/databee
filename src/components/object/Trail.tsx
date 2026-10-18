import { useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { useShallow } from "zustand/react/shallow";
import { columnName, type StackColumn } from "../../lib/columns";
import { documentText } from "../../lib/document";
import { foldSteps, trailUnits, type TrailMore, type TrailUnit } from "../../lib/trail";
import { wholeTable } from "../../state/slices/nav";
import { projectKey } from "../../state/slices/project";
import { selectDocument, useStore, type ObjectTab, type Store } from "../../state/store";
import { useContextMenu, type MenuEntry } from "../ContextMenu";
import { useWidth } from "./useWidth";

/** The entity the rules name the row by (its document's, else its table's only one); chrome, so the table name is the fallback. */
function crumbName(s: Store, sessionId: string, column: StackColumn): string {
  const { name } = columnName(column);
  if (column.kind !== "object") return name;
  const entity = selectDocument(s, sessionId, documentText(column.address))?.document?.entity;
  if (entity) return entity;
  const entities = s.rulesInfo[projectKey(sessionId, column.address.database)]?.tableEntities[name];
  return entities?.length === 1 ? entities[0] : name;
}

interface Crumb {
  name: string;
  key: string | null;
  label: string;
}

function useCrumbs(tab: ObjectTab): Crumb[] {
  const names = useStore(useShallow((s) => tab.columns.map((column) => crumbName(s, tab.sessionId, column))));
  return tab.columns.map((column, index) => {
    const { key } = columnName(column);
    const name = names[index];
    return { name, key, label: key === null ? name : `${name} #${key}` };
  });
}

/** Folds hidden crumbs, one step per layout pass, until the trail fits; a new `layout` (what it shows) starts over unfolded. */
function useFolds(ref: RefObject<HTMLElement | null>, layout: string, max: number): number {
  const [fit, setFit] = useState({ layout: "", steps: 0 });
  const steps = fit.layout === layout ? fit.steps : 0;
  useLayoutEffect(() => {
    const nav = ref.current;
    if (!nav || steps >= max || nav.scrollWidth <= nav.clientWidth) return;
    setFit({ layout, steps: steps + 1 });
  }, [ref, layout, steps, max]);
  return steps;
}

const SEPARATOR = "›";

const separated = (nodes: ReactNode[], key: string): ReactNode[] =>
  nodes.flatMap((node, i) => (i === 0 ? [node] : [<span key={`${key}-sep-${i}`} className="trail-sep" aria-hidden="true">{SEPARATOR}</span>, node]));

function TableCrumb({ table, onOpen }: { table: string; onOpen: () => void }) {
  const label = `Open table ${table}`;
  return (
    <button className="trail-crumb" aria-label={label} title="Open the whole table in this tab" data-stage="trail:table" data-stage-label={label} data-stage-list="trail" onClick={onOpen}>
      {table}
    </button>
  );
}

function MoreCrumb({ side, labels, onOpen }: { side: TrailMore["side"]; labels: string[]; onOpen: (button: HTMLElement) => void }) {
  const title = labels.join(` ${SEPARATOR} `);
  return (
    <button
      className="trail-crumb"
      aria-label="More columns"
      aria-haspopup="menu"
      title={title}
      data-stage={`trail:more-${side}`}
      data-stage-label={title}
      data-stage-list="trail"
      onClick={(e) => onOpen(e.currentTarget)}
    >
      …
    </button>
  );
}

function ColumnCrumb({ crumb, index, active, hidden, onShow }: { crumb: Crumb; index: number; active: boolean; hidden: boolean; onShow: () => void }) {
  const { name, key, label } = crumb;
  return (
    <button
      className="trail-crumb"
      aria-label={label}
      aria-current={active ? "location" : undefined}
      title={hidden ? `Show ${label}` : undefined}
      data-stage={`trail:${index}`}
      data-stage-label={label}
      data-stage-list="trail"
      onClick={onShow}
    >
      <span className="trail-name">{name}</span>
      {key !== null && <span className={active ? "trail-key active" : "trail-key"}> #{key}</span>}
    </button>
  );
}

interface Props {
  tab: ObjectTab;
  /** The columns shown as panes, `[start, end)`. */
  start: number;
  end: number;
}

/** The root row's table, then a crumb per column; crumbs of hidden columns fold into "…" menus when they don't fit. */
export function Trail({ tab, start, end }: Props) {
  const ref = useRef<HTMLElement>(null);
  const menu = useContextMenu();
  const crumbs = useCrumbs(tab);
  const width = useWidth(ref);
  const root = tab.columns[0];
  const table = root?.kind === "object" && root.address.target.kind === "row" ? root.address.target.table : null;
  const count = tab.columns.length;
  const max = foldSteps(count, start, end, !!table);
  const steps = useFolds(ref, [width, start, end, tab.active, table, ...crumbs.map((c) => c.label)].join("\n"), max);
  const units = trailUnits(count, start, end, !!table, steps);

  const { navigateTab, setActiveColumn } = useStore.getState();
  const openTable = () => {
    if (root?.kind === "object") navigateTab(tab.id, wholeTable(root.address));
  };
  const openMore = (more: TrailMore, button: HTMLElement) => {
    const items: MenuEntry[] = [
      ...(more.table ? [{ label: `Open table ${table}`, action: openTable }] : []),
      ...more.columns.map((index) => ({ label: crumbs[index].label, action: () => setActiveColumn(tab.id, index) })),
    ];
    const r = button.getBoundingClientRect();
    menu.open({ x: r.left, y: r.bottom }, items);
  };

  const render = (unit: TrailUnit): ReactNode => {
    if (unit.kind === "table") return table && <TableCrumb key="table" table={table} onOpen={openTable} />;
    if (unit.kind === "more") {
      const labels = [...(unit.table && table ? [table] : []), ...unit.columns.map((i) => crumbs[i].label)];
      return <MoreCrumb key={`more-${unit.side}`} side={unit.side} labels={labels} onOpen={(button) => openMore(unit, button)} />;
    }
    const { index } = unit;
    const hidden = index < start || index >= end;
    return <ColumnCrumb key={`column-${index}`} crumb={crumbs[index]} index={index} active={index === tab.active} hidden={hidden} onShow={() => setActiveColumn(tab.id, index)} />;
  };

  const shown = (unit: TrailUnit) => unit.kind === "column" && unit.index >= start && unit.index < end;
  const first = units.findIndex(shown);
  const after = first + units.filter(shown).length;
  const panes = (
    <span key="panes" className="trail-panes">
      {separated(units.slice(first, after).map(render), "panes")}
    </span>
  );
  const items = [...units.slice(0, first).map(render), panes, ...units.slice(after).map(render)];
  return (
    <nav ref={ref} className={steps >= max ? "trail tight" : "trail"} aria-label="Trail" data-stage="trail" data-stage-label="Trail">
      {separated(items, "trail")}
      {menu.element}
    </nav>
  );
}
