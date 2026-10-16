import { useState } from "react";
import { ChevronDown, ChevronRight, Database, Lock, Table2 } from "lucide-react";
import type { Address, ChipTarget, DocField, EntityDoc, RelatedChip, TimelineEvent } from "../../../api";
import { cellText } from "../../../lib/format";
import { whereEquals } from "../../../lib/sql";
import { flagEmoji } from "../../../lib/wpFormat";
import { selectActiveSession, useStore } from "../../../state/store";
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

export function RelationStrip({ items, onTarget }: { items: RelatedChip[]; onTarget?: (target: ChipTarget) => void }) {
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
        <div>
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

function TimelineItem({ e }: { e: TimelineEvent }) {
  const openEntity = useStore((s) => s.openEntity);
  const session = useStore(selectActiveSession);
  const [open, setOpen] = useState(false);
  const expandable = !!e.raw && e.raw.trim() !== e.title.trim();
  const openRow = () => {
    if (!e.row || !session?.database) return;
    const s = useStore.getState();
    s.setMode("db");
    s.openTableTab(session.database, e.row.table, "data", `\`${e.row.idColumn}\` = ${e.row.id}`);
  };
  return (
    <li className={`tl-item tl-${e.level}`}>
      <span className="tl-icon" aria-hidden>{KIND_ICON[e.kind] ?? "•"}</span>
      <div className="tl-body">
        <div className="tl-title">
          {e.link ? (
            <button className="link-btn plain" onClick={() => openEntity(e.link!.entity, e.link!.id)}>{e.title}</button>
          ) : expandable ? (
            <button className="link-btn plain tl-expand" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
              {open ? <ChevronDown size="0.769rem" /> : <ChevronRight size="0.769rem" />} {e.title}
            </button>
          ) : (
            e.title
          )}
        </div>
        <div className="tl-meta">
          <span className="tl-kind">{e.kind.replace("_", " ")}</span>
          {e.detail && <span> · {e.detail}</span>}
          <span> · <Ago unix={e.at} /></span>
          {e.row && (
            <>
              {" · "}
              <button className="link-btn plain small" onClick={openRow} title={`${e.row.table} where ${e.row.idColumn} = ${e.row.id}`}>
                <Table2 size="0.692rem" /> Source row
              </button>
            </>
          )}
        </div>
        {open && e.raw && <pre className="tl-raw">{e.raw}</pre>}
      </div>
    </li>
  );
}

export function Timeline({ events }: { events: TimelineEvent[] }) {
  if (events.length === 0) return <div className="muted">No events.</div>;
  return (
    <ol className="timeline">
      {events.map((e, i) => (
        <TimelineItem key={`${e.at}-${i}`} e={e} />
      ))}
    </ol>
  );
}

/** The complete generic document: fields, grouped meta, relations as navigation. */
/** The complete stored document: the row's columns and every meta key, as stored. */
export function AllFields({ doc, changed }: { doc: EntityDoc; changed?: Set<string> }) {
  const openInDatabase = useStore((s) => s.openInDatabase);
  const meta = doc.meta.reduce((n, g) => n + g.entries.length, 0);
  const row = whereEquals([[doc.idColumn, doc.id]]);
  const ms = doc.metaSource;
  const metaRow = ms ? (f: DocField) => ({ table: ms.table, where: whereEquals([[ms.fk, doc.id], [ms.key, f.name]]) }) : undefined;
  return (
    <details className="all-fields" open>
      <summary>
        <ChevronRight size="0.923rem" /> Stored fields & meta <span className="badge">{doc.fields.length + meta}</span>
        <span className="muted mono"> · {doc.table}</span>
      </summary>
      <Card
        title={<span className="mono">{doc.table}</span>}
        aside={
          <button className="btn tiny" onClick={() => openInDatabase(doc.table, row)} title={`Open WHERE ${row}`}>
            <Database size="0.846rem" /> Open row
          </button>
        }
      >
        <FieldList fields={doc.fields} changed={changed} target={{ kind: "row", table: doc.table, keyColumn: doc.idColumn, key: doc.id }} source={() => ({ table: doc.table, where: row })} />
      </Card>
      {ms && meta > 0 && (
        <div className="meta-head">
          <span className="mono">{ms.table}</span>
          <span className="muted"> · {meta} keys</span>
          <div className="spacer" />
          <button className="btn tiny" onClick={() => openInDatabase(ms.table, whereEquals([[ms.fk, doc.id]]))}>
            <Database size="0.846rem" /> Open all meta rows
          </button>
        </div>
      )}
      {doc.meta.map((g) => (
        <details key={`${g.private}-${g.name}`} className="meta-group" open>
          <summary>
            <ChevronRight size="0.846rem" />
            {g.private && <Lock size="0.769rem" />}
            <span className="mono">{g.name}</span>
            <span className="badge">{g.entries.length}</span>
          </summary>
          <FieldList fields={g.entries} changed={changed} prefix="meta." target={{ kind: "meta", entity: doc.entity, id: doc.id }} source={metaRow} />
        </details>
      ))}
    </details>
  );
}
