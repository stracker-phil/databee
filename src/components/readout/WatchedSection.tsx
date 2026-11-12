import { useRef, useState, type KeyboardEvent } from "react";
import { Pencil, X } from "lucide-react";
import type { Part, WatchedLine } from "../../api";
import { watchValue } from "../../commands/values";
import { plural } from "../../lib/format";
import { sourceText } from "../../lib/provenance";
import { useStore, type ReadoutTab } from "../../state/store";
import { ValueTree } from "../ValueTree";
import { OpenHint } from "../value/OpenHint";
import { Value, type OpenValue } from "../value/Value";
import { LineError, openAddress, Section, type Collapse } from "./Section";

/** `wp_options.option_value · option_name woocommerce-ppcp-data-common › merchant_country`: where the watch reads. */
const watchedSource = (line: WatchedLine) => sourceText(line.source) + line.path.map((step) => ` › ${step}`).join("");

/** ↵ or leaving the field saves (blank: the default label), ⎋ cancels; each only once. */
function LabelField({ line, onRename, onDone }: { line: WatchedLine; onRename: (label: string | null) => void; onDone: () => void }) {
  const stored = line.labelled ? line.label : "";
  const [text, setText] = useState(stored);
  const finished = useRef(false);
  const finish = (save: boolean) => {
    if (finished.current) return;
    finished.current = true;
    onDone();
    if (save && text.trim() !== stored) onRename(text.trim() || null);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === "Enter") finish(true);
    if (e.key === "Escape") finish(false);
  };
  return (
    <input
      className="input mono tiny"
      autoFocus
      aria-label={`Label of ${watchedSource(line)}`}
      placeholder={line.labelled ? undefined : line.label}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={onKeyDown}
      onBlur={() => finish(true)}
    />
  );
}

function WatchedLabel({ line, renaming, onRename, onDone }: { line: WatchedLine; renaming: boolean; onRename: (label: string | null) => void; onDone: () => void }) {
  if (renaming) return <LabelField line={line} onRename={onRename} onDone={onDone} />;
  const address = line.address;
  if (!address) return <span>{line.label}</span>;
  return (
    <button className="line-open" data-stage={`readout:watched:${line.id}`} data-stage-label={line.label} data-tooltip={`Open ${line.source.table} row`} onClick={() => openAddress(address)}>
      {line.label}
      <OpenHint effect="go" />
    </button>
  );
}

function CurrentValue({ line, onOpen }: { line: WatchedLine; onOpen: OpenValue }) {
  if (line.error) return <span className="line-error">{line.error}</span>;
  if (line.missing) {
    return (
      <span>
        <span className="watched-missing">missing</span>
        <span className="o3"> · {line.missing}</span>
      </span>
    );
  }
  if (line.tree) return <ValueTree value={line.tree} onWatch={(path) => watchValue(line.source, [...line.path, ...path])} />;
  if (!line.value) return null;
  return <Value value={line.value} density={line.value.kind.kind === "serialized" ? "full" : "grid"} onOpen={onOpen} />;
}

function WatchedRow({ line, sessionId, database, onOpen }: { line: WatchedLine; sessionId: string; database: string; onOpen: OpenValue }) {
  const [renaming, setRenaming] = useState(false);
  const source = watchedSource(line);
  return (
    <div className="row" data-row-table={line.source.table}>
      <span>
        <WatchedLabel line={line} renaming={renaming} onRename={(label) => void useStore.getState().labelWatched(sessionId, database, line.id, label)} onDone={() => setRenaming(false)} />
      </span>
      <span className="watched-value">
        <CurrentValue line={line} onOpen={onOpen} />
      </span>
      <span className="o3" data-tooltip={source}>
        {source}
      </span>
      <span className="watched-actions">
        <button className="icon-btn" aria-label={`Rename ${line.label}`} data-stage={`readout:watched-rename:${line.id}`} data-stage-label={`Rename ${line.label}`} onClick={() => setRenaming(true)}>
          <Pencil size="0.923rem" />
        </button>
        <button className="icon-btn" aria-label={`Stop watching ${line.label}`} data-stage={`readout:watched-remove:${line.id}`} data-stage-label={`Stop watching ${line.label}`} onClick={() => void useStore.getState().unwatchValue(sessionId, database, line.id)}>
          <X size="0.923rem" />
        </button>
      </span>
    </div>
  );
}

export function WatchedSection({ part, tab, collapse, onOpen }: { part: Part<WatchedLine>; tab: ReadoutTab; collapse: Collapse; onOpen: OpenValue }) {
  return (
    <Section id="watched" label="Watched values" name="watched" hint="right-click a value or a key of a decoded value: Watch on readout" count={plural(part.items.length, "value", "values")} collapse={collapse}>
      <LineError error={part.error} row />
      {part.items.length === 0 && !part.error && <div className="row readout-empty">no watched values</div>}
      {part.items.length > 0 && (
        <div className="readout-table readout-watched">
          {part.items.map((line) => (
            <WatchedRow key={line.id} line={line} sessionId={tab.sessionId} database={tab.database} onOpen={onOpen} />
          ))}
        </div>
      )}
    </Section>
  );
}
