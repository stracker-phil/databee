import { memo, useEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";
import type { Value as StoredValue } from "../../api";
import { cellDisplay, cellText, relativeTime } from "../../lib/format";
import { formatAddress } from "../../lib/address";
import { bySourceText, provenanceText, rowIdentity } from "../../lib/provenance";
import { selectMaxCellChars, selectNullLabel, useStore } from "../../state/store";
import { TableIcon } from "../TableIcon";
import { ValueTree } from "../ValueTree";
import { OpenHint } from "./OpenHint";
import type { ClickEffect } from "./open";
import { useAltHeld } from "./useAltHeld";
import { useDecoded } from "./useDecoded";

/** `grid`: one line, cut to the cell; `full`: the whole value, serialized data expands in place. */
export type Density = "grid" | "full";

/**
 * Called when a value that acts on click is clicked: a primary key, a reference, an enum, serialized data in a
 * grid. `effect` (from `useOpenValue`) says what the click does, for the indicator shown on hover.
 */
export type OpenValue = ((value: StoredValue, event: MouseEvent) => void) & {
  effect?: (value: StoredValue, density: Density) => ClickEffect | null;
};

interface Props {
  value: StoredValue;
  onOpen?: OpenValue;
  density?: Density;
}

function opensOnClick(value: StoredValue, density: Density): boolean {
  const kind = value.kind;
  if (kind.kind === "primaryKey") return kind.address.target.kind === "row";
  if (kind.kind === "reference") return kind.exists !== false && kind.targets.length > 0;
  if (kind.kind === "serialized") return density === "grid";
  return kind.kind === "enum";
}

function stageId(value: StoredValue, prefix = "value"): string | undefined {
  const source = value.source;
  if (!source) return undefined;
  return `${prefix}:${source.table}.${source.column}:${source.row.map((p) => p.value).join(",")}`;
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
  const [hovered, setHovered] = useState(false);
  const altHeld = useAltHeld();
  const revealedRef = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (shown) revealedRef.current?.focus();
  }, [shown]);
  return (
    <span className="val-secret" onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}>
      {shown || (altHeld && hovered) ? (
        <span ref={revealedRef} className="revealed" tabIndex={-1} onFocus={() => setShown(true)} onBlur={() => setShown(false)}>
          {text}
        </span>
      ) : (
        <button aria-label="Reveal secret" title="Click to reveal · ⌥ hover to peek" onClick={() => setShown(true)}>
          ••••••••
        </button>
      )}
    </span>
  );
}

function SerializedTree({ value, preview }: { value: StoredValue; preview: string }) {
  const [open, setOpen] = useState(false);
  const [opened, setOpened] = useState(false);
  const decoded = useDecoded(cellText(value.raw), opened);
  const toggle = () => {
    setOpen(!open);
    setOpened(true);
  };
  return (
    <span className="val-tree">
      <button className="val-ser" aria-expanded={open} aria-label={`${open ? "Collapse" : "Expand"} ${preview}`} onClick={toggle}>
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
      return (
        <span className="val-pk" role={onActivate ? "link" : undefined} onClick={onActivate}>
          {text}
        </span>
      );
    case "reference": {
      if (kind.exists === false) return <span className="val-dang">{text}</span>;
      const inferred = kind.by.kind === "sameName";
      const link = (
        <span className={inferred ? "val-ref inferred" : "val-ref"} role={onActivate ? "link" : undefined} onClick={onActivate}>
          <TargetIcon value={value} />
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

/** The icon of the table a reference points into; none when its targets are in different tables. */
function TargetIcon({ value }: { value: StoredValue }) {
  const kind = value.kind;
  if (kind.kind !== "reference" || kind.targets.length === 0) return null;
  const tables = new Set(kind.targets.map((t) => ("table" in t.target ? t.target.table : null)));
  if (tables.size !== 1) return null;
  const first = kind.targets[0];
  return <TableIcon database={first.database} table={"table" in first.target ? first.target.table : null} />;
}

function peekData(value: StoredValue) {
  const kind = value.kind;
  if (kind.kind !== "reference" || kind.exists === false || kind.targets.length !== 1) return {};
  const source = value.source;
  return {
    "data-ref": formatAddress(kind.targets[0]),
    "data-via": source?.column,
    "data-by": bySourceText(kind.by),
    "data-from": source ? rowIdentity(source.table, source.row) : undefined,
  };
}

/**
 * One stored value drawn by its kind; `data-src` and `data-table` feed the status bar (`useValueHover`), a
 * reference's `data-ref`, `data-via`, `data-by` and `data-from` (its own row) the peek.
 */
export const Value = memo(function Value({ value, onOpen, density = "full" }: Props) {
  const acts = !!onOpen && opensOnClick(value, density);
  const onActivate = acts ? (e: MouseEvent) => onOpen(value, e) : undefined;
  const effect = acts ? (onOpen.effect?.(value, density) ?? null) : null;
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
      {...peekData(value)}
      style={style}
    >
      <Body value={value} density={density} onActivate={onActivate} />
      {value.secondary && <span className="val-sec">{value.secondary.text}</span>}
      {effect && <OpenHint effect={effect} edge />}
    </span>
  );
});
