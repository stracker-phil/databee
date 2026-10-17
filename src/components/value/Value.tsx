import { memo, useEffect, useState, type MouseEvent, type ReactNode } from "react";
import { api, type DecodedValue, type Value as StoredValue } from "../../api";
import { cellDisplay, cellText, relativeTime } from "../../lib/format";
import { provenanceText } from "../../lib/provenance";
import { fail, selectMaxCellChars, selectNullLabel, useStore } from "../../state/store";
import { ValueTree } from "../ValueTree";

/** `grid`: one line, cut to the cell; `full`: the whole value, serialized data expands in place. */
export type Density = "grid" | "full";

/** Called when a value that acts on click is clicked: a reference, an enum, serialized data in a grid. */
export type OpenValue = (value: StoredValue, event: MouseEvent) => void;

interface Props {
  value: StoredValue;
  onOpen?: OpenValue;
  density?: Density;
}

/** Whether a click on the value does something (docs/ux.md, Value grammar). */
export function opensOnClick(value: StoredValue, density: Density): boolean {
  const kind = value.kind;
  if (kind.kind === "reference") return kind.exists !== false && kind.targets.length > 0;
  if (kind.kind === "serialized") return density === "grid";
  return kind.kind === "enum";
}

function stageId(value: StoredValue): string | undefined {
  const source = value.source;
  if (!source) return undefined;
  return `value:${source.table}.${source.column}:${source.row.map((p) => p.value).join(",")}`;
}

type Activate = (e: MouseEvent) => void;

function NullText() {
  const nullLabel = useStore(selectNullLabel);
  return <span className="val-null">{nullLabel}</span>;
}

function PlainText({ value, density }: { value: StoredValue; density: Density }) {
  const nullLabel = useStore(selectNullLabel);
  const maxChars = useStore(selectMaxCellChars);
  return <>{density === "grid" ? cellDisplay(value.raw, nullLabel, maxChars) : cellText(value.raw)}</>;
}

function SecretText({ text }: { text: string }) {
  const [shown, setShown] = useState(false);
  if (shown) return <>{text}</>;
  return (
    <button className="val-secret" aria-label="Reveal secret" onClick={() => setShown(true)}>
      ••••••••
    </button>
  );
}

/** PHP-serialized or JSON text, decoded once it is first expanded. */
function useDecoded(text: string, open: boolean): DecodedValue | null {
  const [decoded, setDecoded] = useState<DecodedValue | null>(null);
  useEffect(() => {
    if (!open || decoded) return;
    let alive = true;
    api.decodeValue(text).then(
      (d) => {
        if (alive) setDecoded(d);
      },
      (e) => {
        if (alive) fail(e, "Could not decode the value");
      },
    );
    return () => {
      alive = false;
    };
  }, [text, open, decoded]);
  return decoded;
}

function SerializedTree({ value, preview }: { value: StoredValue; preview: string }) {
  const [open, setOpen] = useState(false);
  const decoded = useDecoded(cellText(value.raw), open);
  return (
    <span className="val-tree">
      <button className="val-ser" aria-expanded={open} aria-label={`${open ? "Collapse" : "Expand"} ${preview}`} onClick={() => setOpen(!open)}>
        {open ? "▾" : "▸"} {preview}
      </button>
      {open && decoded && <ValueTree value={decoded} />}
    </span>
  );
}

function Serialized({ value, preview, density, onActivate }: { value: StoredValue; preview: string; density: Density; onActivate?: Activate }) {
  if (density === "full" && value.raw.t !== "longText") return <SerializedTree value={value} preview={preview} />;
  return (
    <>
      <span className="val-ser" role={onActivate ? "button" : undefined} onClick={onActivate}>
        ▸ {preview}
      </span>
      {density === "grid" && (
        <span className="val-o3">
          {" "}
          <PlainText value={value} density={density} />
        </span>
      )}
    </>
  );
}

const withSecondary = (main: ReactNode, secondary: string | null, className = "val-sec") => (
  <>
    {main}
    {secondary && <span className={className}>{secondary}</span>}
  </>
);

function Body({ value, density, onActivate }: { value: StoredValue; density: Density; onActivate?: Activate }) {
  const kind = value.kind;
  const text = cellText(value.raw);
  if (value.raw.t === "null" && (kind.kind === "null" || kind.kind === "noReference")) return <NullText />;
  switch (kind.kind) {
    case "primaryKey":
      return <span className="val-pk">{text}</span>;
    case "reference": {
      if (kind.exists === false) return <span className="val-dang">{text}</span>;
      const inferred = kind.by.kind === "sameName";
      const link = (
        <span className={inferred ? "val-ref inferred" : "val-ref"} role={onActivate ? "link" : undefined} onClick={onActivate}>
          {text}
        </span>
      );
      return withSecondary(link, inferred ? "inferred" : null, "val-note");
    }
    case "noReference":
      return <span className="val-o3">{text}</span>;
    case "enum":
      return (
        <span className="val-enum" role={onActivate ? "button" : undefined} onClick={onActivate}>
          {text}
        </span>
      );
    case "number":
      return <span className="val-num">{text}</span>;
    case "money":
      return withSecondary(<span className="val-num">{text}</span>, kind.currency ? cellText(kind.currency.raw) : null);
    case "timestamp":
      return withSecondary(<PlainText value={value} density={density} />, kind.utc === null ? null : relativeTime(kind.utc));
    case "empty":
      return <span className="val-o3">""</span>;
    case "serialized":
      return <Serialized value={value} preview={kind.preview} density={density} onActivate={onActivate} />;
    case "secret":
      return <SecretText text={text} />;
    case "binary":
      if (value.raw.t !== "bytes") return <PlainText value={value} density={density} />;
      return <span className="val-o3">{`<${value.raw.v.len} bytes>`}</span>;
    default:
      return <PlainText value={value} density={density} />;
  }
}

/** One stored value drawn by its kind; `data-src` and `data-table` feed the status bar (`useValueHover`). */
export const Value = memo(function Value({ value, onOpen, density = "full" }: Props) {
  const acts = !!onOpen && opensOnClick(value, density);
  const onActivate = acts ? (e: MouseEvent) => onOpen(value, e) : undefined;
  const style = value.style ? { color: `var(--style-${value.style})` } : undefined;
  const stage = acts || value.kind.kind === "secret" ? stageId(value) : undefined;
  return (
    <span
      className={`val val-${value.kind.kind}`}
      data-src={provenanceText(value) ?? undefined}
      data-table={value.source?.table}
      data-stage={stage}
      data-stage-label={stage ? cellText(value.raw) : undefined}
      data-stage-list={stage ? "values" : undefined}
      style={style}
    >
      <Body value={value} density={density} onActivate={onActivate} />
      {value.secondary && <span className="val-sec">{value.secondary.text}</span>}
    </span>
  );
});
