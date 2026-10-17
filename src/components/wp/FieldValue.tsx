import { useState } from "react";
import { Copy, Eye, Maximize2, Minimize2, MoreHorizontal, Pencil } from "lucide-react";
import { api, type ColumnEquals, type DocField, type EntityLink, type NewValue, type Provenance, type Value as StoredValue, type ValueKind } from "../../api";
import { copyText } from "../../lib/clipboard";
import { cellText, relativeTime } from "../../lib/format";
import { absoluteFromUnix, shorten } from "../../lib/wpFormat";
import { selectActiveSession, useStore, type WpList } from "../../state/store";
import { ContextMenu, type MenuEntry } from "../ContextMenu";
import { Value } from "../value/Value";
import { ValueTree } from "../ValueTree";
import { sourceText } from "../../lib/provenance";

const COLLAPSE_AT = 120;

/** Where edits of a field list go: a table row; `key: null` uses the field name as key (options), `column` defaults to the field name. */
export type EditTarget = { kind: "row"; table: string; keyColumn: string; key: string | null; column?: string };

function formatBytesShort(n: number) {
  return n < 1024 ? `${n} B` : `${(n / 1024).toFixed(1)} KB`;
}

function Secret({ text }: { text: string }) {
  const [shown, setShown] = useState(false);
  if (shown) return <span className="mono">{text}</span>;
  return (
    <button className="masked" aria-label="Reveal secret" onClick={() => setShown(true)}>
      <Eye size="0.846rem" /> ••••••••
    </button>
  );
}

function LongText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <span className="long">
      <span className={open ? "long-full" : "mono"}>{open ? text : shorten(text, 32, 6)}</span>
      {!open && <span className="hint"> ({formatBytesShort(new TextEncoder().encode(text).length)})</span>}
      <button className="icon-btn tiny" aria-label={open ? "Collapse value" : "Expand value"} onClick={() => setOpen(!open)}>
        {open ? <Minimize2 size="0.846rem" /> : <Maximize2 size="0.846rem" />}
      </button>
      <button
        className="icon-btn tiny"
        aria-label="Copy value"
        onClick={() => void copyText(text, "value")}
      >
        <Copy size="0.846rem" />
      </button>
    </span>
  );
}

/** Queues a new value for the field at `target`. */
function useFieldEditor(field: DocField, target?: EditTarget) {
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  if (!target || !session?.database) return null;
  const database = session.database;
  return (value: string | null) =>
    queueChange({
      type: "setCell",
      database,
      table: target.table,
      keyColumn: target.keyColumn,
      key: target.key ?? field.name,
      column: target.column ?? field.name,
      value,
    });
}

/** An entity link drawn as a reference: the stored id in the accent; a click opens the entity as an object. */
function EntityRef({ link }: { link: EntityLink }) {
  return (
    <button className="val-ref" aria-label={`Open ${link.entity} ${link.id}`} onClick={() => void useStore.getState().openEntity(link.entity, link.id)}>
      {link.id}
    </button>
  );
}

function TextEditor({ initial, onSave, onCancel, nullable }: { initial: string; onSave: (v: string | null) => void; onCancel: () => void; nullable: boolean }) {
  const [text, setText] = useState(initial);
  const multiline = initial.length > 80 || initial.includes("\n");
  const keys = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onCancel();
    if (e.key === "Enter" && (!multiline || e.metaKey)) {
      e.preventDefault();
      onSave(text);
    }
  };
  return (
    <span className="field-editor">
      {multiline ? (
        <textarea className="input mono" autoFocus rows={4} aria-label="New value" value={text} onChange={(e) => setText(e.target.value)} onKeyDown={keys} />
      ) : (
        <input className="input" autoFocus aria-label="New value" value={text} onChange={(e) => setText(e.target.value)} onKeyDown={keys} />
      )}
      <button className="btn tiny btn-primary" onClick={() => onSave(text)}>Queue</button>
      {nullable && <button className="btn tiny" onClick={() => onSave(null)} title="Set NULL">{"∅"}</button>}
      <button className="btn tiny" onClick={onCancel}>Cancel</button>
    </span>
  );
}

/** Fields whose value filters a list: the list and which query parameter carries the value. */
const PIVOTS: Record<string, { list: WpList; param: "search" | "kind" | "status"; label: string }> = {
  billing_email: { list: "orders", param: "search", label: "orders" },
  _billing_email: { list: "orders", param: "search", label: "orders" },
  user_email: { list: "orders", param: "search", label: "orders" },
  payment_method: { list: "orders", param: "kind", label: "orders" },
  _payment_method: { list: "orders", param: "kind", label: "orders" },
  status: { list: "orders", param: "status", label: "orders" },
  post_status: { list: "content", param: "status", label: "content" },
  hook: { list: "actions", param: "kind", label: "actions" },
  transaction_id: { list: "orders", param: "search", label: "orders" },
  _transaction_id: { list: "orders", param: "search", label: "orders" },
};

/** Context-menu entries for a value: copy, filter the matching list, find it everywhere. */
export function valueMenu(
  name: string,
  text: string,
  actions: { copy: () => void; navigate: (route: Parameters<ReturnType<typeof useStore.getState>["navigate"]>[0]) => void; openRow?: () => void },
): MenuEntry[] {
  const items: MenuEntry[] = [{ label: "Copy value", action: actions.copy }];
  if (actions.openRow) items.push({ label: "Open row in Database view", action: actions.openRow });
  const pivot = PIVOTS[name];
  if (pivot && text && (pivot.param !== "status" || text.startsWith("wc-") || pivot.list === "content")) {
    items.push({ label: `Filter ${pivot.label} by this value`, action: () => actions.navigate({ screen: "list", list: pivot.list, params: { [pivot.param]: text } }) });
  }
  if (text.trim().length >= 2) {
    items.push("separator", { label: "Find everywhere", action: () => useStore.getState().setPaletteOpen(true, `=${text.trim()}`) });
  }
  return items;
}

/** The table row a field is stored in, as column matches for the Database view; `column` when the field is a column of it. */
export type RowSource = { table: string; where: ColumnEquals[]; column?: string };

/** Where a column field is stored; meta and option fields have no single column here. */
function provenanceOf(source?: RowSource): Provenance | null {
  if (!source?.column) return null;
  const row = source.where.flatMap((c) => (c.value === null ? [] : [{ column: c.column, value: c.value }]));
  return { table: source.table, column: source.column, row };
}

/**
 * The field as a `Value`, for the kinds `Value` draws the same way (until the options screen goes in U12):
 * null, empty, secrets, money and time hints, short text.
 */
function fieldValue(field: DocField, source: Provenance): StoredValue | null {
  const { value: raw, hint } = field;
  const text = cellText(raw);
  let kind: ValueKind = { kind: "text" };
  if (raw.t === "null") kind = { kind: "null" };
  else if (field.sensitive === "secret") kind = { kind: "secret" };
  else if (text === "") kind = { kind: "empty" };
  else if (field.link || field.decoded || text.length > COLLAPSE_AT) return null;
  else if (hint?.type === "time") kind = { kind: "timestamp", utc: hint.unix, zone: null };
  const secondary = hint?.type === "money" ? { text: hint.text, source: null } : null;
  return { raw, kind, source, style: null, secondary };
}

/** Renders one field according to its render hint; raw text is always one toggle away. */
export function FieldValue({ field, target, source }: { field: DocField; target?: EditTarget; source?: RowSource }) {
  const nullLabel = useStore((s) => s.settings?.nullLabel ?? "NULL");
  const toast = useStore((s) => s.toast);
  const [raw, setRaw] = useState(false);
  const [editing, setEditing] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const save = useFieldEditor(field, target);
  const { value, hint, decoded, link, sensitive } = field;
  const text = cellText(value);
  const provenance = provenanceOf(source);
  const bridged = provenance && fieldValue(field, provenance);

  const editTree = save
    ? async (path: number[], v: NewValue) => {
        try {
          await save(await api.editSerialized(text, path, v));
        } catch (e) {
          toast("error", String((e as { message?: string }).message ?? e));
        }
      }
    : undefined;

  if (editing && save) {
    return (
      <TextEditor
        initial={text}
        nullable
        onCancel={() => setEditing(false)}
        onSave={(v) => {
          setEditing(false);
          void save(v);
        }}
      />
    );
  }

  // Option fields have no single column, and `Value` has no form for entity links, editable trees or
  // collapsed long text yet.
  let body: React.ReactNode;
  if (bridged) {
    body = <Value value={bridged} />;
  } else if (value.t === "null") {
    body = <span className="null">{nullLabel}</span>;
  } else if (sensitive === "secret") {
    body = <Secret text={text} />;
  } else if (link) {
    body = <EntityRef link={link} />;
  } else if (decoded && !raw) {
    body = <ValueTree value={decoded} onEdit={editTree} />;
  } else if (hint) {
    // The stored value always shows; the hint only annotates it.
    const plain = text.length > COLLAPSE_AT ? <LongText text={text} /> : <span>{text}</span>;
    const note = (n: React.ReactNode, title?: string) => (
      <>
        {plain} <span className="hint-note" title={title}>{n}</span>
      </>
    );
    body = hint.type === "money" ? note(hint.text) : note(relativeTime(hint.unix), absoluteFromUnix(hint.unix));
  } else if (text.length > COLLAPSE_AT) {
    body = <LongText text={text} />;
  } else {
    body = <span>{text}</span>;
  }

  const canToggle = !!decoded && sensitive !== "secret" && !link;
  const menuItems = () =>
    valueMenu(field.name, text, {
      copy: () => void copyText(text, "value"),
      navigate: (route) => useStore.getState().navigate(route),
      openRow: source && (() => useStore.getState().openInDatabase(source.table, source.where)),
    });
  const hasMenu = value.t !== "null" || !!source;
  const openMenu = (x: number, y: number) => hasMenu && setMenu({ x, y });
  return (
    <span
      className="field-value val"
      data-src={provenance ? sourceText(provenance) : undefined}
      data-table={provenance?.table}
      onContextMenu={(e) => {
        if (!hasMenu) return;
        e.preventDefault();
        openMenu(e.clientX, e.clientY);
      }}
    >
      {body}
      {hasMenu && (
        <button
          className="row-action"
          aria-label={`Actions for ${field.name}`}
          title="Copy · open row · filter · find everywhere"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            openMenu(r.left, r.bottom);
          }}
        >
          <MoreHorizontal size="0.846rem" />
        </button>
      )}
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems()} onClose={() => setMenu(null)} />}
      {canToggle && (
        <button className="raw-toggle" onClick={() => setRaw(!raw)} title={raw ? "Show decoded tree" : "Show raw value"}>
          {raw ? "tree" : "raw"}
        </button>
      )}
      {save && (
        <button className="row-action" aria-label={`Edit ${field.name}`} onClick={() => setEditing(true)} title="Edit">
          <Pencil size="0.846rem" />
        </button>
      )}
    </span>
  );
}

export function FieldList({ fields, changed, prefix = "", target, source }: { fields: DocField[]; changed?: Set<string>; prefix?: string; target?: EditTarget; source?: (f: DocField) => RowSource | undefined }) {
  return (
    <div className="kv">
      {fields.map((f) => (
        <div key={f.name} className={`kv-row${changed?.has(prefix + f.name) ? " flash" : ""}`}>
          <div className="k mono" title={f.name}>{f.name}</div>
          <div className="v"><FieldValue field={f} target={target} source={source?.(f)} /></div>
        </div>
      ))}
    </div>
  );
}
