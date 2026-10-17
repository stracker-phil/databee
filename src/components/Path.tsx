/** The toolbar path: the active object tab's column stack as crumbs, `order #4711 › user #12 › payment_token #7`. */
import { Fragment } from "react";
import { columnName, type StackColumn } from "../lib/columns";
import { documentText } from "../lib/document";
import { projectKey } from "../state/slices/project";
import { selectActiveTab, selectDocument, useStore, type ObjectTab } from "../state/store";

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

export function Path() {
  const tab = useStore((s) => {
    const active = selectActiveTab(s);
    return active?.kind === "object" ? active : null;
  });
  if (!tab) return null;
  return (
    <nav className="path mono" aria-label="Path" data-stage="toolbar:path">
      {tab.columns.map((column, index) => (
        <Fragment key={index}>
          {index > 0 && <span className="o3">›</span>}
          <Crumb tab={tab} column={column} index={index} />
        </Fragment>
      ))}
    </nav>
  );
}
