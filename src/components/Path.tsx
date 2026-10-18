/**
 * The toolbar path: the active object tab's table, then its column stack as crumbs (`wp_wc_orders › order #4711 ›
 * user #12`); a table tab's table as the current location.
 */
import { Fragment } from "react";
import { columnName, type StackColumn } from "../lib/columns";
import { documentText } from "../lib/document";
import { wholeTable } from "../state/slices/nav";
import { projectKey } from "../state/slices/project";
import { selectActiveTab, selectDocument, useStore, type ObjectTab, type TableTab } from "../state/store";

/** The entity the rules name the row by (its document's, else its table's only one); chrome, so the table name is the fallback. */
function useCrumbName(sessionId: string, column: StackColumn): string {
  const { name } = columnName(column);
  return useStore((s) => {
    if (column.kind !== "object") return name;
    const entity = selectDocument(s, sessionId, documentText(column.address))?.document?.entity;
    if (entity) return entity;
    const entities = s.rulesInfo[projectKey(sessionId, column.address.database)]?.tableEntities[name];
    return entities?.length === 1 ? entities[0] : name;
  });
}

function Crumb({ tab, column, index }: { tab: ObjectTab; column: StackColumn; index: number }) {
  const name = useCrumbName(tab.sessionId, column);
  const { key } = columnName(column);
  const active = index === tab.active;
  const label = key === null ? name : `${name} #${key}`;
  const content = (
    <>
      <span className="o2">{name}</span>
      {key !== null && <span className={active ? "key" : undefined}> #{key}</span>}
    </>
  );
  if (index === tab.columns.length - 1) {
    return (
      <span className="crumb" aria-current={active ? "location" : undefined}>
        {content}
      </span>
    );
  }
  return (
    <button
      className="crumb"
      aria-current={active ? "location" : undefined}
      data-stage={`path:${index}`}
      aria-label={label}
      data-stage-label={label}
      data-stage-list="path"
      title="Back to this column"
      onClick={() => useStore.getState().popToColumn(tab.id, index)}
    >
      {content}
    </button>
  );
}

/** The table of the tab's root row; a click shows the whole table in the same tab. */
function TableCrumb({ tab }: { tab: ObjectTab }) {
  const root = tab.columns[0];
  if (root?.kind !== "object" || root.address.target.kind !== "row") return null;
  const { table } = root.address.target;
  return (
    <>
      <button
        className="crumb"
        data-stage="path:table"
        aria-label={`Open table ${table}`}
        data-stage-label={`Open table ${table}`}
        title="Open the whole table in this tab"
        onClick={() => useStore.getState().navigateTab(tab.id, wholeTable(root.address))}
      >
        <span className="o2">{table}</span>
      </button>
      <span className="o3">›</span>
    </>
  );
}

function TablePath({ tab }: { tab: TableTab }) {
  const filtered = !!tab.filter || tab.where.length > 0;
  return (
    <nav className="path mono" aria-label="Path" data-stage="toolbar:path">
      <span className="crumb" aria-current="location">
        <span className="o2">{tab.table}</span>
        {filtered && <span className="o3"> (filtered)</span>}
      </span>
    </nav>
  );
}

export function Path() {
  const tab = useStore((s) => {
    const active = selectActiveTab(s);
    return active?.kind === "object" || active?.kind === "table" ? active : null;
  });
  if (!tab) return null;
  if (tab.kind === "table") return <TablePath tab={tab} />;
  return (
    <nav className="path mono" aria-label="Path" data-stage="toolbar:path">
      <TableCrumb tab={tab} />
      {tab.columns.map((column, index) => (
        <Fragment key={index}>
          {index > 0 && <span className="o3">›</span>}
          <Crumb tab={tab} column={column} index={index} />
        </Fragment>
      ))}
    </nav>
  );
}
