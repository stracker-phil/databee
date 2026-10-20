import { X } from "lucide-react";
import type { Address, Document } from "../../api";
import { useBindingLabel } from "../../commands";
import { formatAddress } from "../../lib/address";
import { objectTitle, type ObjectView } from "../../lib/document";
import { wholeTable } from "../../state/slices/nav";
import { useStore } from "../../state/store";
import { useContextMenu, type MenuAnchor } from "../ContextMenu";
import { KeyCap } from "../Field";
import { TableIcon } from "../TableIcon";
import type { ColumnPlace } from "../value/open";
import { useOpenValue } from "../value/useOpenValue";
import { SummaryLine } from "../value/SummaryLine";
import { actionMenu } from "./actions";
import { PaneMaximize } from "./PaneMaximize";

interface Props {
  document: Document;
  owner: Address;
  view: { current: ObjectView; onChange: (view: ObjectView) => void };
  /** The column of an object tab the header is in: its table name opens the whole table in that tab, a drilled column closes. */
  place?: ColumnPlace;
}

function ViewToggle({ current, onChange }: Props["view"]) {
  const documentKeys = useBindingLabel("object.viewDocument");
  const rowsKeys = useBindingLabel("object.viewRows");
  return (
    <div className="segmented" role="group" aria-label="View" data-stage="view-toggle" data-stage-label="Document / Rows">
      <button data-stage="view-toggle:document" aria-pressed={current === "document"} onClick={() => onChange("document")}>
        Document
        <span aria-hidden="true">
          <KeyCap keys={documentKeys} />
        </span>
      </button>
      <button data-stage="view-toggle:rows" aria-pressed={current === "rows"} onClick={() => onChange("rows")}>
        Rows
        <span aria-hidden="true">
          <KeyCap keys={rowsKeys} />
        </span>
      </button>
    </div>
  );
}

export function ObjectHeader({ document, owner, view, place }: Props) {
  const menu = useContextMenu();
  const { openValue, targetMenu } = useOpenValue(owner);
  const target = document.address.target;
  const table = target.kind === "row" ? target.table : "";
  const key = target.kind === "row" ? target.key : "";
  const openActions = (at: MenuAnchor) => menu.open(at, actionMenu(owner, document.actions));
  return (
    <section className="object-head" aria-label="Object header" data-stage="object-header" data-stage-label={`${table} #${key}`} onContextMenu={openActions}>
      <div className="object-title">
        {place ? (
          <button
            className="tn"
            aria-label={`Open table ${table}`}
            title={`Open table ${table}`}
            data-stage="object-header:table"
            data-stage-label={`Open table ${table}`}
            onClick={() => useStore.getState().navigateTab(place.tabId, wholeTable(document.address))}
          >
            <TableIcon database={document.address.database} table={table} effect="go" />
            {table}
          </button>
        ) : (
          <span className="tn">
            <TableIcon database={document.address.database} table={table} />
            {table}
          </span>
        )}
        <span className="key">#{key}</span>
        <span className="spacer" />
        <ViewToggle {...view} />
        {document.actions.length > 0 && (
          <button
            className="btn"
            title="Actions for this row"
            data-stage="object-actions"
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              openActions({ x: r.left, y: r.bottom });
            }}
          >
            Actions
          </button>
        )}
        {place && <PaneMaximize />}
        {place && place.index > 0 && (
          <button className="icon-btn" aria-label="Close column" title="Close column" data-stage={`column-close:${place.index}`} onClick={() => useStore.getState().popToColumn(place.tabId, place.index - 1)}>
            <X size="1rem" />
          </button>
        )}
      </div>
      {document.header.values.length > 0 && (
        <div className="object-summary">
          <SummaryLine summary={document.header} onOpen={openValue} />
        </div>
      )}
      {document.alternatives.length > 0 && (
        <div className="object-alternatives">
          Also matched:{" "}
          {document.alternatives.map((alt) => (
            <button key={formatAddress(alt)} className="also-table" data-stage={`alternative:${formatAddress(alt)}`} data-stage-label={objectTitle(alt)} onClick={() => useStore.getState().openObject(alt)}>
              <TableIcon database={alt.database} table={"table" in alt.target ? alt.target.table : null} effect="go" />
              {objectTitle(alt)}
            </button>
          ))}
        </div>
      )}
      {menu.element}
      {targetMenu}
    </section>
  );
}
