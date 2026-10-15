import { useState } from "react";
import { Check, Copy, Eye, Maximize2, Minimize2, Minus, MonitorSmartphone, Pencil, Trash2 } from "lucide-react";
import { api, type DocField, type NewValue } from "../../api";
import { cellText } from "../../lib/format";
import { absoluteFromUnix, flagEmoji, relativeFromUnix, shorten } from "../../lib/wpFormat";
import { selectActiveSession, useStore } from "../../state/store";
import { ValueTree } from "../ValueTree";
import { LinkChip } from "./LinkChip";

const COLLAPSE_AT = 120;

/** Where edits of a field list go: a table row (columns) or an entity's meta. */
export type EditTarget =
  /** `key: null` uses the field name as key (e.g. options); `column` defaults to the field name. */
  | { kind: "row"; table: string; keyColumn: string; key: string | null; column?: string }
  | { kind: "meta"; entity: string; id: string };

function formatBytesShort(n: number) {
  return n < 1024 ? `${n} B` : `${(n / 1024).toFixed(1)} KB`;
}

function Secret({ text }: { text: string }) {
  const [shown, setShown] = useState(false);
  if (shown) return <span className="mono">{text}</span>;
  return (
    <button className="masked" aria-label="Reveal secret" onClick={() => setShown(true)}>
      <Eye size={11} /> ••••••••
    </button>
  );
}

function LongText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const toast = useStore((s) => s.toast);
  return (
    <span className="long">
      <span className={open ? "long-full" : "mono"}>{open ? text : shorten(text, 32, 6)}</span>
      {!open && <span className="hint"> ({formatBytesShort(new TextEncoder().encode(text).length)})</span>}
      <button className="icon-btn tiny" aria-label={open ? "Collapse value" : "Expand value"} onClick={() => setOpen(!open)}>
        {open ? <Minimize2 size={11} /> : <Maximize2 size={11} />}
      </button>
      <button
        className="icon-btn tiny"
        aria-label="Copy value"
        onClick={() => void navigator.clipboard.writeText(text).then(() => toast("success", "Copied value"))}
      >
        <Copy size={11} />
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
    target.kind === "row"
      ? queueChange({
          type: "setCell",
          database,
          table: target.table,
          keyColumn: target.keyColumn,
          key: target.key ?? field.name,
          column: target.column ?? field.name,
          value,
        })
      : value == null
        ? queueChange({ type: "deleteMeta", database, entity: target.entity, id: target.id, key: field.name })
        : queueChange({ type: "setMeta", database, entity: target.entity, id: target.id, key: field.name, value });
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
      {nullable && <button className="btn tiny" onClick={() => onSave(null)} title="Set NULL / delete meta">{"∅"}</button>}
      <button className="btn tiny" onClick={onCancel}>Cancel</button>
    </span>
  );
}

const FLAG_OPPOSITE: Record<string, string> = { yes: "no", no: "yes", true: "false", false: "true", on: "off", off: "on", "1": "0", "0": "1" };

/** Renders one field according to its render hint; raw text is always one toggle away. */
export function FieldValue({ field, target }: { field: DocField; target?: EditTarget }) {
  const nullLabel = useStore((s) => s.settings?.nullLabel ?? "NULL");
  const toast = useStore((s) => s.toast);
  const [raw, setRaw] = useState(false);
  const [editing, setEditing] = useState(false);
  const save = useFieldEditor(field, target);
  const { value, hint, decoded, link, sensitive } = field;
  const text = cellText(value);

  const editTree = save
    ? async (path: string[], v: NewValue) => {
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

  let body: React.ReactNode;
  if (value.t === "null") {
    body = <span className="null">{nullLabel}</span>;
  } else if (sensitive === "secret") {
    body = <Secret text={text} />;
  } else if (link) {
    body = <LinkChip link={link} />;
  } else if (decoded && !raw) {
    body = <ValueTree value={decoded} onEdit={editTree} />;
  } else if (hint && !raw) {
    switch (hint.type) {
      case "money":
        body = <span className="money">{hint.text}</span>;
        break;
      case "time":
        body = <span title={`${absoluteFromUnix(hint.unix)}${hint.utc ? "" : " (site time)"} · ${text}`}>{relativeFromUnix(hint.unix)}</span>;
        break;
      case "flag": {
        const opposite = FLAG_OPPOSITE[text.toLowerCase()];
        body = (
          <button
            className={`flag-pill ${hint.on ? "on" : "off"}`}
            disabled={!save || !opposite}
            title={save && opposite ? `Click to queue “${opposite}”` : undefined}
            onClick={() => save && opposite && void save(opposite)}
          >
            {hint.on ? <Check size={10} /> : <Minus size={10} />} {text}
          </button>
        );
        break;
      }
      case "url":
        body = (
          <a href={hint.href} className="url" onClick={(e) => (e.preventDefault(), void api.openUrl(hint.href))} title={hint.href}>
            {shorten(hint.href.replace(/^https?:\/\//, ""), 48, 12)}
          </a>
        );
        break;
      case "userAgent":
        body = (
          <span title={text}>
            <MonitorSmartphone size={12} style={{ verticalAlign: -2 }} /> {hint.text}
          </span>
        );
        break;
      case "roles":
        body = hint.roles.map((r) => (
          <span key={r} className="badge" style={{ marginRight: 4 }}>{r}</span>
        ));
        break;
      case "country":
        body = <span>{flagEmoji(hint.code)} {hint.code}</span>;
        break;
      case "guest":
        body = <span className="badge">Guest</span>;
        break;
      case "email":
        body = <span>{text}</span>;
        break;
    }
  } else if (text.length > COLLAPSE_AT) {
    body = <LongText text={text} />;
  } else {
    body = <span>{text}</span>;
  }

  const canToggle = (decoded || (hint && hint.type !== "email")) && sensitive !== "secret" && !link;
  return (
    <span className={`field-value${sensitive === "pii" ? " pii" : ""}`}>
      {body}
      {canToggle && (
        <button className="raw-toggle" onClick={() => setRaw(!raw)} title={raw ? "Show formatted" : "Show raw value"}>
          {raw ? "fmt" : "raw"}
        </button>
      )}
      {save && (
        <>
          <button className="row-action" aria-label={`Edit ${field.name}`} onClick={() => setEditing(true)} title="Edit">
            <Pencil size={11} />
          </button>
          {target?.kind === "meta" && (
            <button className="row-action" aria-label={`Delete ${field.name}`} onClick={() => void save(null)} title="Delete meta">
              <Trash2 size={11} />
            </button>
          )}
        </>
      )}
    </span>
  );
}

export function FieldList({ fields, changed, prefix = "", target }: { fields: DocField[]; changed?: Set<string>; prefix?: string; target?: EditTarget }) {
  return (
    <div className="kv">
      {fields.map((f) => (
        <div key={f.name} className={`kv-row${changed?.has(prefix + f.name) ? " flash" : ""}`}>
          <div className="k" title={f.label ? `${f.label} (${f.name})` : f.name}>{f.label ?? f.name}</div>
          <div className="v"><FieldValue field={f} target={target} /></div>
        </div>
      ))}
    </div>
  );
}
