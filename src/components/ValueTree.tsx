import { useState } from "react";
import { AlertTriangle, ChevronDown, ChevronRight, Pencil } from "lucide-react";
import type { DecodedValue, NewValue, ValueNode } from "../api";

/** Called with the key path of an edited leaf and its new typed value. */
export type TreeEdit = (path: string[], value: NewValue) => void;

function LeafEditor({ node, onSave, onCancel }: { node: ValueNode; onSave: (v: NewValue) => void; onCancel: () => void }) {
  const [text, setText] = useState(node.kind === "null" ? "" : (node.text ?? ""));
  const submit = () => {
    switch (node.kind) {
      case "int":
        return onSave({ kind: "int", text });
      case "float":
        return onSave({ kind: "float", text });
      default:
        return onSave({ kind: "string", text });
    }
  };
  if (node.kind === "bool") {
    return (
      <span className="tree-edit">
        <button className="btn tiny" onClick={() => onSave({ kind: "bool", text: node.text !== "true" })}>
          set {node.text === "true" ? "false" : "true"}
        </button>
        <button className="btn tiny" onClick={onCancel}>cancel</button>
      </span>
    );
  }
  return (
    <span className="tree-edit">
      <input
        className="input mono tiny"
        autoFocus
        aria-label="New value"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") submit();
          if (e.key === "Escape") onCancel();
        }}
      />
      <button className="btn tiny" onClick={() => onSave({ kind: "null" })} title="Set to null">null</button>
    </span>
  );
}

type Child = NonNullable<ValueNode["children"]>[number];

const KEY_HINT: Record<Child["keyKind"], string> = {
  int: "",
  string: "",
  public: "",
  protected: "protected",
  private: "private",
};

function summary(node: ValueNode): string {
  const n = node.children?.length ?? 0;
  if (node.kind === "array") return `array(${n})`;
  if (node.kind === "object") return `${node.class ?? "object"} {${n}}`;
  return node.kind;
}

function Leaf({ node }: { node: ValueNode }) {
  if (node.kind === "string") {
    return <span className="vt-string">"{node.text}"</span>;
  }
  if (node.kind === "custom") {
    return (
      <span className="vt-muted">
        {node.class} (custom) {node.text}
      </span>
    );
  }
  return <span className={`vt-${node.kind}`}>{node.text}</span>;
}

function Row({ label, keyKind, node, depth, path, onEdit }: { label?: string; keyKind?: Child["keyKind"]; node: ValueNode; depth: number; path: string[]; onEdit?: TreeEdit }) {
  const hasChildren = (node.children?.length ?? 0) > 0;
  const [open, setOpen] = useState(depth < 2);
  const [editing, setEditing] = useState(false);
  const editable = onEdit && !hasChildren && !["ref", "custom", "enum", "array", "object"].includes(node.kind);
  return (
    <div className="vt-row" role="treeitem" aria-expanded={hasChildren ? open : undefined}>
      <div className="vt-line" onClick={() => hasChildren && setOpen(!open)}>
        <span className="vt-toggle">{hasChildren ? open ? <ChevronDown size="0.846rem" /> : <ChevronRight size="0.846rem" /> : null}</span>
        {label !== undefined && (
          <span className={`vt-key${keyKind === "int" ? " int" : ""}`}>
            {label}
            {keyKind && KEY_HINT[keyKind] && <span className="vt-vis"> {KEY_HINT[keyKind]}</span>}
          </span>
        )}
        {editing ? (
          <LeafEditor
            node={node}
            onCancel={() => setEditing(false)}
            onSave={(v) => {
              setEditing(false);
              onEdit?.(path, v);
            }}
          />
        ) : node.kind === "array" || node.kind === "object" ? (
          <span className="vt-muted">{summary(node)}</span>
        ) : (
          <Leaf node={node} />
        )}
        {node.note && <span className="badge" style={{ marginLeft: 6 }}>{node.note}</span>}
        {editable && !editing && (
          <button className="vt-edit" aria-label={`Edit ${path.join("/") || "value"}`} onClick={(e) => (e.stopPropagation(), setEditing(true))}>
            <Pencil size="0.769rem" />
          </button>
        )}
      </div>
      {hasChildren && open && (
        <div className="vt-children" role="group">
          {node.children!.map((c, i) => (
            <Row key={`${c.key}-${i}`} label={c.key} keyKind={c.keyKind} node={c.node} depth={depth + 1} path={[...path, c.key]} onEdit={onEdit} />
          ))}
        </div>
      )}
    </div>
  );
}

export function ValueTree({ value, onEdit }: { value: DecodedValue; onEdit?: TreeEdit }) {
  return (
    <div className="vt" role="tree" aria-label={`${value.format.toUpperCase()} value`}>
      {value.warning && (
        <div className="vt-warning" role="note">
          <AlertTriangle size="0.923rem" /> {value.warning}
        </div>
      )}
      {value.error && (
        <div className="vt-error" role="alert">
          <AlertTriangle size="0.923rem" /> Could not decode: {value.error}
        </div>
      )}
      {value.root && <Row node={value.root} depth={0} path={[]} onEdit={onEdit} />}
    </div>
  );
}
