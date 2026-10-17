import { useState, type MouseEvent } from "react";
import { MoreHorizontal } from "lucide-react";
import type { Address, Document } from "../../api";
import { useBindingLabel } from "../../commands";
import { formatAddress } from "../../lib/address";
import { objectTitle, type ObjectView } from "../../lib/document";
import { useStore } from "../../state/store";
import { ContextMenu, type MenuEntry } from "../ContextMenu";
import { KeyCap } from "../Field";
import { useOpenValue } from "../value/useOpenValue";
import { Value } from "../value/Value";
import { actionMenu } from "./actions";

interface Props {
  document: Document;
  owner: Address;
  /** The Document/Rows control. */
  view: { current: ObjectView; onChange(view: ObjectView): void };
}

function ViewToggle({ current, onChange }: Props["view"]) {
  const keys = useBindingLabel("object.toggleView");
  return (
    <>
      <div className="segmented" role="group" aria-label="View" data-stage="view-toggle" data-stage-label="Document / Rows">
        <button data-stage="view-toggle:document" aria-pressed={current === "document"} onClick={() => onChange("document")}>
          Document
        </button>
        <button data-stage="view-toggle:rows" aria-pressed={current === "rows"} onClick={() => onChange("rows")}>
          Rows
        </button>
      </div>
      <KeyCap keys={keys} />
    </>
  );
}

/** The object header block (docs/ux.md): table name, `#key`, the view control, the summary; actions on right-click. */
export function ObjectHeader({ document, owner, view }: Props) {
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const { openValue, targetMenu } = useOpenValue(owner);
  const target = document.address.target;
  const table = target.kind === "row" ? target.table : "";
  const key = target.kind === "row" ? target.key : "";
  const openActions = (x: number, y: number) => setMenu({ x, y, items: actionMenu(owner, document.actions) });
  const onContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    openActions(e.clientX, e.clientY);
  };
  return (
    <section className="object-head" aria-label="Object header" data-stage="object-header" data-stage-label={`${table} #${key}`} onContextMenu={onContextMenu}>
      <div className="object-title">
        <span className="tn">{table}</span>
        <span className="key">#{key}</span>
        <span className="spacer" />
        <ViewToggle {...view} />
        <button
          className="icon-btn"
          aria-label="Actions"
          title={document.actions.length > 0 ? "Actions for this row" : "No actions apply to this row"}
          data-stage="object-actions"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            openActions(r.left, r.bottom);
          }}
        >
          <MoreHorizontal size="1rem" />
        </button>
      </div>
      {document.header.length > 0 && (
        <div className="object-summary">
          {document.header.map(([name, value]) => (
            <span key={name}>
              <span className="o3">{name} </span>
              <Value value={value} onOpen={openValue} />
            </span>
          ))}
        </div>
      )}
      {document.alternatives.length > 0 && (
        <div className="object-alternatives">
          Also matched:{" "}
          {document.alternatives.map((alt) => (
            <button key={formatAddress(alt)} className="also-table" data-stage={`alternative:${formatAddress(alt)}`} data-stage-label={objectTitle(alt)} onClick={() => useStore.getState().openObject(alt)}>
              {objectTitle(alt)}
            </button>
          ))}
        </div>
      )}
      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
      {targetMenu}
    </section>
  );
}
