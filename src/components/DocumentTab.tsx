import { useEffect, useState } from "react";
import { ArrowUpRight, ChevronDown, ChevronRight, Lock, RefreshCw, Table2 } from "lucide-react";
import { api, toAppError, type AppError, type DocField, type EntityDoc } from "../api";
import { cellDisplay } from "../lib/format";
import { useStore, type DocumentTab as DocumentTabState } from "../state/store";
import { ErrorPanel } from "./QueryTab";
import { ValueTree } from "./ValueTree";

function FieldValue({ field }: { field: DocField }) {
  const nullLabel = useStore((s) => s.settings?.nullLabel ?? "NULL");
  const openDocument = useStore((s) => s.openDocument);
  const [raw, setRaw] = useState(false);
  const decoded = field.decoded;
  return (
    <>
      {decoded && !raw ? (
        <ValueTree value={decoded} />
      ) : (
        <span className={`t-${field.value.t}`} style={field.value.t === "null" ? { color: "var(--type-null)", fontStyle: "italic" } : undefined}>
          {cellDisplay(field.value, nullLabel, 100_000)}
        </span>
      )}
      {decoded && (
        <button className="link-btn" style={{ background: "var(--bg-active)", color: "var(--text-secondary)" }} onClick={() => setRaw(!raw)}>
          {raw ? decoded.format.toUpperCase() : "raw"}
        </button>
      )}
      {field.link && (
        <button
          className="link-btn"
          aria-label={`Open ${field.link.entity} ${field.link.id}`}
          onClick={() => openDocument(field.link!.entity, field.link!.id)}
        >
          → {field.link.entity} {field.link.id}
        </button>
      )}
    </>
  );
}

function Fields({ fields }: { fields: DocField[] }) {
  return (
    <div className="kv">
      {fields.map((f) => (
        <div key={f.name} style={{ display: "contents" }}>
          <div className="k" title={f.name}>{f.name}</div>
          <div className="v"><FieldValue field={f} /></div>
        </div>
      ))}
    </div>
  );
}

function Relation({ doc, relation, database, sessionId }: { doc: EntityDoc; relation: EntityDoc["relations"][number]; database: string; sessionId: string }) {
  const openDocument = useStore((s) => s.openDocument);
  const [items, setItems] = useState<EntityDoc[] | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const load = () => {
    if (items || relation.count === 0) return;
    api.wpExpand(sessionId, database, doc.entity, doc.id, relation.name).then(setItems, (e) => setError(toAppError(e)));
  };

  return (
    <details className="relation" onToggle={(e) => (e.currentTarget as HTMLDetailsElement).open && load()}>
      <summary>
        <ChevronRight size={12} />
        <strong>{relation.label}</strong>
        <span className="badge">{relation.count}</span>
      </summary>
      {error && <ErrorPanel message={error.message} code={error.code} />}
      {relation.count > 0 && !items && !error && <div className="relation-item muted">Loading…</div>}
      {items?.map((item) => (
        <div key={item.id}>
          <div className="relation-item">
            <button className="icon-btn" style={{ width: 18, height: 18 }} aria-label={`Expand ${item.label} ${item.id}`} onClick={() => setExpandedId(expandedId === item.id ? null : item.id)}>
              {expandedId === item.id ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
            </button>
            <span className="muted mono">#{item.id}</span>
            <span className="t">{item.title ?? <span className="muted">untitled</span>}</span>
            {item.subtitle && <span className="badge">{item.subtitle}</span>}
            <button className="link-btn" onClick={() => openDocument(item.entity, item.id, item.label)}>
              open <ArrowUpRight size={10} style={{ verticalAlign: -1 }} />
            </button>
          </div>
          {expandedId === item.id && (
            <div style={{ padding: "6px 12px 10px 36px" }}>
              <Fields fields={item.fields} />
              {item.meta.flatMap((g) => g.entries).length > 0 && (
                <div style={{ marginTop: 8 }}>
                  <Fields fields={item.meta.flatMap((g) => g.entries)} />
                </div>
              )}
            </div>
          )}
        </div>
      ))}
    </details>
  );
}

export function DocumentView({ doc, database, sessionId }: { doc: EntityDoc; database: string; sessionId: string }) {
  const openTableTab = useStore((s) => s.openTableTab);
  const publicMeta = doc.meta.filter((g) => !g.private);
  const privateMeta = doc.meta.filter((g) => g.private);
  const metaCount = doc.meta.reduce((n, g) => n + g.entries.length, 0);

  const MetaGroups = ({ groups }: { groups: EntityDoc["meta"] }) => (
    <>
      {groups.map((g) => (
        <details key={`${g.private}-${g.name}`} className="meta-group" open={groups.length <= 4 || g.name !== "other"}>
          <summary>
            <ChevronRight size={11} />
            {g.private ? `_${g.name}` : g.name}
            <span className="badge">{g.entries.length}</span>
          </summary>
          <Fields fields={g.entries} />
        </details>
      ))}
    </>
  );

  return (
    <div className="doc">
      <div className="doc-head">
        <span className="badge">{doc.label}</span>
        <h2>#{doc.id} {doc.title && <span style={{ fontWeight: 400 }}>{doc.title}</span>}</h2>
        {doc.subtitle && <span className="badge">{doc.subtitle}</span>}
        <div className="spacer" />
        <button className="btn" onClick={() => openTableTab(database, doc.table, "data", `\`${doc.idColumn}\` = ${doc.id}`)}>
          <Table2 size={13} /> Source row
        </button>
      </div>

      <section className="doc-section">
        <h3>Fields</h3>
        <Fields fields={doc.fields} />
      </section>

      {metaCount > 0 && (
        <section className="doc-section">
          <h3>Meta <span className="badge">{metaCount}</span></h3>
          <MetaGroups groups={publicMeta} />
          {privateMeta.length > 0 && (
            <>
              <h3 style={{ marginTop: 12 }}><Lock size={11} /> Private meta</h3>
              <MetaGroups groups={privateMeta} />
            </>
          )}
        </section>
      )}

      {doc.relations.length > 0 && (
        <section className="doc-section">
          <h3>Related</h3>
          {doc.relations.map((r) => (
            <Relation key={r.name} doc={doc} relation={r} database={database} sessionId={sessionId} />
          ))}
        </section>
      )}
    </div>
  );
}

export function DocumentTab({ tab }: { tab: DocumentTabState }) {
  const [doc, setDoc] = useState<EntityDoc | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let alive = true;
    api.wpLoad(tab.sessionId, tab.database, tab.entity, [tab.entityId]).then(
      (docs) => {
        if (!alive) return;
        if (docs.length === 0) setError({ kind: "invalid_input", message: `${tab.entity} #${tab.entityId} was not found`, code: null });
        else {
          setDoc(docs[0]);
          setError(null);
        }
      },
      (e) => alive && setError(toAppError(e)),
    );
    return () => {
      alive = false;
    };
  }, [tab.sessionId, tab.database, tab.entity, tab.entityId, version]);

  return (
    <>
      <div className="table-head">
        <span className="muted">{tab.database}</span>
        <div className="spacer" />
        <button className="icon-btn" aria-label="Reload document" onClick={() => setVersion((v) => v + 1)}>
          <RefreshCw size={13} />
        </button>
      </div>
      {error ? (
        <ErrorPanel message={error.message} code={error.code} />
      ) : doc ? (
        <DocumentView doc={doc} database={tab.database} sessionId={tab.sessionId} />
      ) : (
        <div className="empty-state">Loading…</div>
      )}
    </>
  );
}
