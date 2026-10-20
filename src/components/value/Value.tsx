import { memo, useEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { Value as StoredValue } from "../../api";
import { cellDisplay, cellText, isoUtc, relativeTime } from "../../lib/format";
import { formatAddress } from "../../lib/address";
import { bySourceText, provenanceText, rowIdentity } from "../../lib/provenance";
import { fetchStoredValue } from "../../lib/cells";
import { useApiResult } from "../../lib/useApiResult";
import { selectActiveTab, selectMaxCellChars, selectNullLabel, useStore } from "../../state/store";
import { tabDatabase } from "../../state/tabKinds";
import { TableIcon } from "../TableIcon";
import { watchValue } from "../../commands/values";
import { ValueTree } from "../ValueTree";
import { OpenHint, OpenSwap } from "./OpenHint";
import type { ClickEffect } from "./open";
import { useAltPeek } from "./altPeek";
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
  const [el, setEl] = useState<HTMLSpanElement | null>(null);
  const peeked = useAltPeek().target === el && el !== null;
  const revealedRef = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (shown) revealedRef.current?.focus();
  }, [shown]);
  return (
    <span ref={setEl} className="val-secret" data-peek="">
      {shown || peeked ? (
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
  const source = value.source;
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
      {open && decoded && <ValueTree value={decoded} onWatch={source ? (path) => watchValue(source, path) : undefined} />}
    </span>
  );
}

/** The whole stored text; a grid's `longText` cell holds only a preview, so its row is read again by key. */
function usePeekText(value: StoredValue): string | null {
  const long = value.raw.t === "longText";
  const source = value.source;
  const key = long && source ? JSON.stringify(source) : null;
  const fetched = useApiResult(key, async () => {
    const s = useStore.getState();
    const tab = selectActiveTab(s);
    const database = tab && tabDatabase(s, tab);
    if (!tab || !database || !source) return null;
    return (await fetchStoredValue(tab.sessionId, database, source))?.text ?? null;
  });
  return long ? (fetched.data ?? null) : cellText(value.raw);
}

/** The ⌥ peek of a serialized value: its decoded tree, the first levels open, in an overlay below it. */
function SerializedPeek({ value, anchor }: { value: StoredValue; anchor: Element }) {
  const text = usePeekText(value);
  const decoded = useDecoded(text ?? "", text !== null);
  const rect = anchor.getBoundingClientRect();
  return createPortal(
    <div className="value-peek" role="tooltip" style={{ left: rect.left, top: rect.bottom + 4 }}>
      {decoded ? <ValueTree value={decoded} /> : <span className="o3">Decoding…</span>}
    </div>,
    document.body,
  );
}

function Serialized({ value, preview, density, onActivate, effect }: { value: StoredValue; preview: string; density: Density; onActivate?: Activate; effect: ClickEffect | null }) {
  const [el, setEl] = useState<HTMLSpanElement | null>(null);
  const peeked = useAltPeek().target;
  if (density === "full" && value.raw.t !== "longText") return <SerializedTree value={value} preview={preview} />;
  return (
    <span ref={setEl} data-peek="">
      {el && peeked === el && <SerializedPeek value={value} anchor={el} />}
      <span className="val-ser" role={onActivate ? "button" : undefined} onClick={onActivate}>
        {effect ? (
          <OpenSwap effect={effect}>
            <span className="ser-mark">▸</span>
          </OpenSwap>
        ) : (
          "▸ "
        )}
        {preview}
      </span>
      {density === "grid" && (
        <span className="val-o3">
          {" "}
          <PlainText value={value} density={density} />
        </span>
      )}
    </span>
  );
}

const withSecondary = (main: ReactNode, secondary: string | null, className = "val-sec") => (
  <>
    {main}
    {secondary && <span className={className}>{secondary}</span>}
  </>
);

/** The stored value with its relative time; its ⌥ peek shows the UTC date in an overlay below it. */
function TimeValue({ value, density, utc }: { value: StoredValue; density: Density; utc: number }) {
  const [el, setEl] = useState<HTMLSpanElement | null>(null);
  const peeked = useAltPeek().target;
  const rect = el && peeked === el ? el.getBoundingClientRect() : null;
  return (
    <span ref={setEl} className="val-time" data-peek="">
      <PlainText value={value} density={density} />
      <span className="val-sec">{relativeTime(utc)}</span>
      {rect &&
        createPortal(
          <div className="time-peek" role="tooltip" style={{ left: rect.left, top: rect.bottom + 4 }}>
            {isoUtc(utc)}
          </div>,
          document.body,
        )}
    </span>
  );
}

function Body({ value, density, onActivate, effect }: { value: StoredValue; density: Density; onActivate?: Activate; effect: ClickEffect | null }) {
  const kind = value.kind;
  const text = cellText(value.raw);
  if (value.raw.t === "null" && (kind.kind === "null" || kind.kind === "noReference")) return <NullText />;
  switch (kind.kind) {
    case "primaryKey":
      return (
        <span className="val-pk" role={onActivate ? "link" : undefined} onClick={onActivate}>
          <LeadIcon value={value} effect={effect} />
          {text}
        </span>
      );
    case "reference": {
      if (kind.exists === false) return <span className="val-dang">{text}</span>;
      const inferred = kind.by.kind === "sameName";
      const link = (
        <span className={inferred ? "val-ref inferred" : "val-ref"} role={onActivate ? "link" : undefined} onClick={onActivate}>
          <LeadIcon value={value} effect={effect} />
          {text}
          {effect && !leadTable(value) && <OpenHint effect={effect} />}
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
      if (kind.utc === null) return <PlainText value={value} density={density} />;
      return <TimeValue value={value} density={density} utc={kind.utc} />;
    case "empty":
      return <span className="val-o3">""</span>;
    case "serialized":
      return <Serialized value={value} preview={kind.preview} density={density} onActivate={onActivate} effect={effect} />;
    case "secret":
      return <SecretText text={text} />;
    case "binary":
      if (value.raw.t !== "bytes") return <PlainText value={value} density={density} />;
      return <span className="val-o3">{`<${value.raw.v.len} bytes>`}</span>;
    default:
      return <PlainText value={value} density={density} />;
  }
}

/** The table a key's row is in or a reference points into; none when a reference's targets are in different tables. */
function leadTable(value: StoredValue): { database: string; table: string } | null {
  const kind = value.kind;
  if (kind.kind === "primaryKey") return kind.address.target.kind === "row" ? { database: kind.address.database, table: kind.address.target.table } : null;
  if (kind.kind !== "reference" || kind.targets.length === 0) return null;
  const tables = new Set(kind.targets.map((t) => ("table" in t.target ? t.target.table : null)));
  const first = kind.targets[0];
  if (tables.size !== 1 || !("table" in first.target)) return null;
  return { database: first.database, table: first.target.table };
}

/** Characters a leading table icon takes before the value: 1em plus its gap, in the mono font's `ch`. */
export const LEAD_ICON_CH = 2.5;

export const hasLeadIcon = (value: StoredValue) => leadTable(value) !== null;

/** The icon of `leadTable`, which turns into the indicator of `effect` while hovered. */
function LeadIcon({ value, effect }: { value: StoredValue; effect: ClickEffect | null }) {
  const lead = leadTable(value);
  return lead && <TableIcon database={lead.database} table={lead.table} effect={effect} />;
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
 * One stored value drawn by its kind; `data-src` and `data-table` feed the status bar (`useValueHover`),
 * `data-lead-table` (`leadTable`) the linked table highlight (`TableFocus`), a reference's `data-ref`, `data-via`, `data-by` and `data-from` (its own row) the peek.
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
      data-lead-table={leadTable(value)?.table}
      data-stage={stage}
      data-stage-label={stage ? cellText(value.raw) : undefined}
      data-stage-list={stage ? "values" : undefined}
      {...peekData(value)}
      style={style}
    >
      <Body value={value} density={density} onActivate={onActivate} effect={effect} />
      {value.secondary && <span className="val-sec">{value.secondary.text}</span>}
    </span>
  );
});
