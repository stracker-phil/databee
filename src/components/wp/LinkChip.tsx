import type { EntityLink } from "../../api";
import { useStore } from "../../state/store";
import { Peekable } from "./Peek";

/**
 * An entity link drawn as a reference value: the stored id in the accent with a dotted underline, the
 * target's summary after it in secondary text. WordPress screens only, until U7 deletes them.
 */
export function LinkChip({ link, label }: { link: EntityLink; label?: string }) {
  const openEntity = useStore((s) => s.openEntity);
  const summary = label ?? link.label;
  return (
    <span className="val val-reference">
      <Peekable entity={link.entity} id={link.id}>
        <button className="val-ref" aria-label={`Open ${link.entity} ${link.id}`} onClick={() => openEntity(link.entity, link.id)}>
          {link.id}
        </button>
      </Peekable>
      {summary && <span className="val-sec">{summary}</span>}
    </span>
  );
}
