import { useState, type MouseEvent } from "react";
import { AlertTriangle, ChevronDown, ChevronRight, Pencil } from "lucide-react";
import type { DecodedValue, NewValue, ValueChild, ValueNode } from "../api";
import { useContextMenu } from "./ContextMenu";

/** Called with the child-index path of an edited leaf and its new typed value. */
type TreeEdit = (path: number[], value: NewValue) => void;

/** Opens a node's context menu; `labels` are its keys from the root. */
type TreeMenu = (labels: string[], event: MouseEvent) => void;

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

function keyHint(child: ValueChild): string {
  if (child.keyKind === "protected") return "protected";
  if (child.keyKind === "private") return child.declaringClass ? `private ${child.declaringClass}` : "private";
  return child.rawHex ? `0x${child.rawHex}` : "";
}

function summary(node: ValueNode): string {
  const n = node.children?.length ?? 0;
  if (node.kind === "array") return `array(${n})`;
  if (node.kind === "object") return `${node.class ?? "object"} {${n}}`;
  return node.kind;
}

/** Arrays and objects this small show their leaf pairs on one line. */
const INLINE_CHILDREN = 4;
const INLINE_TEXT = 60;

function inlinePairs(node: ValueNode): boolean {
  const children = node.children ?? [];
  if (children.length === 0 || children.length > INLINE_CHILDREN) return false;
  if (children.some((c) => (c.node.children?.length ?? 0) > 0 || c.node.kind === "array" || c.node.kind === "object")) return false;
  return children.reduce((n, c) => n + c.key.length + (c.node.text?.length ?? 0), 0) <= INLINE_TEXT;
}

/** Quotes are dim, so a string `"2.04"` and a number `2.04` look different. */
function Leaf({ node }: { node: ValueNode }) {
  if (node.kind === "string") {
    return (
      <span className="vt-string">
        <span className="vt-quote">"</span>
        {node.text}
        <span className="vt-quote">"</span>
      </span>
    );
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

/** `path` addresses the node by child indexes; `labels` are the same steps as shown. A row is
 * either a child or the root `node`. */
function Row({
  child,
  node: root,
  depth,
  path,
  labels,
  onEdit,
  onMenu,
}: {
  child?: ValueChild;
  node?: ValueNode;
  depth: number;
  path: number[];
  labels: string[];
  onEdit?: TreeEdit;
  onMenu?: TreeMenu;
}) {
  const node = child?.node ?? root!;
  const hasChildren = (node.children?.length ?? 0) > 0;
  const [open, setOpen] = useState(depth < 2);
  const [editing, setEditing] = useState(false);
  // Inline pairs have no edit buttons, so an editable tree keeps one row per leaf.
  if (hasChildren && !onEdit && inlinePairs(node)) return <InlineRow child={child} node={node} labels={labels} onMenu={onMenu} />;
  const editable = onEdit && !hasChildren && !["ref", "custom", "enum", "array", "object"].includes(node.kind);
  return (
    <div className="vt-row" role="treeitem" aria-expanded={hasChildren ? open : undefined}>
      <div className="vt-line" onClick={() => hasChildren && setOpen(!open)} onContextMenu={onMenu && ((e) => onMenu(labels, e))}>
        <span className="vt-toggle">{hasChildren ? open ? <ChevronDown size="0.846rem" /> : <ChevronRight size="0.846rem" /> : null}</span>
        {child && <Key child={child} />}
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
          <button className="vt-edit" aria-label={`Edit ${labels.join("/") || "value"}`} onClick={(e) => (e.stopPropagation(), setEditing(true))}>
            <Pencil size="0.769rem" />
          </button>
        )}
      </div>
      {hasChildren && open && (
        <div className="vt-children" role="group">
          {node.children!.map((c, i) => (
            <Row
              key={`${c.key}-${i}`}
              child={c}
              depth={depth + 1}
              path={[...path, i]}
              labels={[...labels, c.key]}
              onEdit={onEdit}
              onMenu={onMenu}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/** The stored key, with its visibility or raw bytes when the key has them. */
function Key({ child }: { child: ValueChild }) {
  const hint = keyHint(child);
  return (
    <span className={`vt-key${child.keyKind === "int" ? " int" : ""}`}>
      {child.key}
      {hint && <span className="vt-vis"> {hint}</span>}
    </span>
  );
}

function InlineRow({ child, node, labels, onMenu }: { child?: ValueChild; node: ValueNode; labels: string[]; onMenu?: TreeMenu }) {
  return (
    <div className="vt-row" role="treeitem">
      <div className="vt-line" onContextMenu={onMenu && ((e) => onMenu(labels, e))}>
        <span className="vt-toggle" />
        {child && <Key child={child} />}
        <span className="vt-muted">{summary(node)}</span>
        {node.children!.map((c, i) => (
          <span key={`${c.key}-${i}`} className="vt-pair" onContextMenu={onMenu && ((e) => onMenu([...labels, c.key], e))}>
            <Key child={c} />
            <Leaf node={c.node} />
          </span>
        ))}
      </div>
    </div>
  );
}

/** `onWatch` (with a node's keys from the root) adds "Watch on readout" to every node's context menu. */
export function ValueTree({ value, onEdit, onWatch }: { value: DecodedValue; onEdit?: TreeEdit; onWatch?: (path: string[]) => void }) {
  const { open, element: menu } = useContextMenu();
  // The node's menu, not the menu of the value or row around the tree.
  const onMenu: TreeMenu | undefined =
    onWatch &&
    ((labels, e) => {
      e.stopPropagation();
      open(e, [{ label: "Watch on readout", action: () => onWatch(labels) }]);
    });
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
      {value.root && <Row node={value.root} depth={0} path={[]} labels={[]} onEdit={onEdit} onMenu={onMenu} />}
      {menu}
    </div>
  );
}
