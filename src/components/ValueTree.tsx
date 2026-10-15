import { useState } from "react";
import { AlertTriangle, ChevronDown, ChevronRight } from "lucide-react";
import type { DecodedValue, ValueNode } from "../api";

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

function Row({ label, keyKind, node, depth }: { label?: string; keyKind?: Child["keyKind"]; node: ValueNode; depth: number }) {
  const hasChildren = (node.children?.length ?? 0) > 0;
  const [open, setOpen] = useState(depth < 2);
  return (
    <div className="vt-row" role="treeitem" aria-expanded={hasChildren ? open : undefined}>
      <div className="vt-line" onClick={() => hasChildren && setOpen(!open)}>
        <span className="vt-toggle">{hasChildren ? open ? <ChevronDown size={11} /> : <ChevronRight size={11} /> : null}</span>
        {label !== undefined && (
          <span className={`vt-key${keyKind === "int" ? " int" : ""}`}>
            {label}
            {keyKind && KEY_HINT[keyKind] && <span className="vt-vis"> {KEY_HINT[keyKind]}</span>}
          </span>
        )}
        {node.kind === "array" || node.kind === "object" ? <span className="vt-muted">{summary(node)}</span> : <Leaf node={node} />}
        {node.note && <span className="badge" style={{ marginLeft: 6 }}>{node.note}</span>}
      </div>
      {hasChildren && open && (
        <div className="vt-children" role="group">
          {node.children!.map((c, i) => (
            <Row key={`${c.key}-${i}`} label={c.key} keyKind={c.keyKind} node={c.node} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}

export function ValueTree({ value }: { value: DecodedValue }) {
  return (
    <div className="vt" role="tree" aria-label={`${value.format.toUpperCase()} value`}>
      {value.warning && (
        <div className="vt-warning" role="note">
          <AlertTriangle size={12} /> {value.warning}
        </div>
      )}
      {value.error && (
        <div className="vt-error" role="alert">
          <AlertTriangle size={12} /> Could not decode: {value.error}
        </div>
      )}
      {value.root && <Row node={value.root} depth={0} />}
    </div>
  );
}
