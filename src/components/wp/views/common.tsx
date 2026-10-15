import { ChevronRight, Lock } from "lucide-react";
import type { Address, EntityDoc, RelatedChip, TimelineEvent } from "../../../api";
import { cellText } from "../../../lib/format";
import { flagEmoji } from "../../../lib/wpFormat";
import { useStore } from "../../../state/store";
import { FieldList } from "../FieldValue";
import { LinkChip } from "../LinkChip";
import { Ago, Card } from "../ui";

/** Every field and meta entry, flattened to `name → text`; used for change highlighting. */
export function flattenDoc(doc: EntityDoc): Record<string, string> {
  const out: Record<string, string> = {};
  for (const f of doc.fields) out[f.name] = cellText(f.value);
  for (const g of doc.meta) for (const f of g.entries) out[`meta.${f.name}`] = cellText(f.value);
  return out;
}

export function RelationStrip({ items, onTarget }: { items: RelatedChip[]; onTarget?: (target: string) => void }) {
  if (items.length === 0) return null;
  return (
    <div className="relation-strip" aria-label="Related">
      {items.map((r, i) =>
        r.link ? (
          <LinkChip key={i} link={r.link} label={r.label} />
        ) : (
          <button key={i} className="chip" onClick={() => r.target && onTarget?.(r.target)}>
            <span className="chip-label">{r.label}</span>
            {r.count != null && <span className="chip-id">{r.count}</span>}
          </button>
        ),
      )}
    </div>
  );
}

export function AddressCard({ title, address, warn }: { title: string; address: Address | null; warn?: boolean }) {
  return (
    <div className={`address-card${warn ? " warn" : ""}`}>
      <div className="address-title">{title}</div>
      {address ? (
        <div className="pii">
          {address.name && <div className="address-name">{address.name}</div>}
          {address.company && <div>{address.company}</div>}
          {address.lines.map((l) => (
            <div key={l}>{l}</div>
          ))}
          {address.country && (
            <div>
              {flagEmoji(address.country)} {address.country}
            </div>
          )}
          {address.email && <div className="muted">{address.email}</div>}
          {address.phone && <div className="muted">{address.phone}</div>}
        </div>
      ) : (
        <div className="muted">No address</div>
      )}
    </div>
  );
}

const KIND_ICON: Record<string, string> = {
  session: "🧭",
  created: "🛒",
  paid: "💳",
  completed: "✅",
  note: "📝",
  customer_note: "💬",
  action: "⏱",
  log: "📄",
  refund: "↩︎",
};

export function Timeline({ events }: { events: TimelineEvent[] }) {
  const openEntity = useStore((s) => s.openEntity);
  if (events.length === 0) return <div className="muted">No events.</div>;
  return (
    <ol className="timeline">
      {events.map((e, i) => (
        <li key={i} className={`tl-item tl-${e.level}`}>
          <span className="tl-icon" aria-hidden>{KIND_ICON[e.kind] ?? "•"}</span>
          <div className="tl-body">
            <div className="tl-title">
              {e.link ? (
                <button className="link-btn plain" onClick={() => openEntity(e.link!.entity, e.link!.id)}>{e.title}</button>
              ) : (
                e.title
              )}
            </div>
            <div className="tl-meta">
              <span className="tl-kind">{e.kind.replace("_", " ")}</span>
              {e.detail && <span> · {e.detail}</span>}
              <span> · <Ago unix={e.at} /></span>
            </div>
          </div>
        </li>
      ))}
    </ol>
  );
}

/** The complete generic document: fields, grouped meta, relations as navigation. */
export function AllFields({ doc, changed, open = false }: { doc: EntityDoc; changed?: Set<string>; open?: boolean }) {
  const meta = doc.meta.reduce((n, g) => n + g.entries.length, 0);
  return (
    <details className="all-fields" open={open}>
      <summary>
        <ChevronRight size="0.923rem" /> All fields & meta <span className="badge">{doc.fields.length + meta}</span>
        <span className="muted"> · {doc.table}</span>
      </summary>
      <Card title="Fields">
        <FieldList fields={doc.fields} changed={changed} target={{ kind: "row", table: doc.table, keyColumn: doc.idColumn, key: doc.id }} />
      </Card>
      {doc.meta.map((g) => (
        <details key={`${g.known}-${g.private}-${g.name}`} className="meta-group" open={g.known || doc.meta.length <= 4}>
          <summary>
            <ChevronRight size="0.846rem" />
            {!g.known && g.private && <Lock size="0.769rem" />}
            {g.name}
            <span className="badge">{g.entries.length}</span>
          </summary>
          <FieldList fields={g.entries} changed={changed} prefix="meta." target={{ kind: "meta", entity: doc.entity, id: doc.id }} />
        </details>
      ))}
    </details>
  );
}
