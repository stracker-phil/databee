import { useId, useState } from "react";
import { api, type ActionParam, type ActionPreview, type Address, type ObjectAction, type PreviewRow } from "../../api";
import { bindingFor, bindingLabel } from "../../commands/registry";
import { formatAddress } from "../../lib/address";
import { objectTitle } from "../../lib/document";
import { plural } from "../../lib/format";
import { useApiResult } from "../../lib/useApiResult";
import { selectSessionBySlug, useStore } from "../../state/store";
import { Modal } from "../Modal";
import { Value } from "../value/Value";
import { writesSummary } from "./actions";

const PREVIEW_DELAY_MS = 250;

const initialValue = (param: ActionParam) => param.default ?? (param.optional ? "" : (param.choices[0] ?? ""));

function ParamField({ param, value, onChange }: { param: ActionParam; value: string; onChange: (value: string) => void }) {
  const listId = useId();
  return (
    <label className="action-param">
      <span className="mono o2">{param.name}</span>
      <input className="input mono" list={param.choices.length > 0 ? listId : undefined} value={value} onChange={(e) => onChange(e.target.value)} />
      {param.choices.length > 0 && (
        <datalist id={listId}>
          {param.choices.map((choice) => (
            <option key={choice} value={choice} />
          ))}
        </datalist>
      )}
    </label>
  );
}

function RowWrites({ row }: { row: PreviewRow }) {
  const key = row.key.map((part) => part.value).join(", ");
  return (
    <div className="action-row" role="group" aria-label={`${row.table} #${key}`}>
      <div className="action-row-head mono">
        <span className="o2">{row.table}</span> <span className="key">#{key}</span>
        {row.change.kind === "delete" && <span className="o3"> deleted</span>}
      </div>
      {row.change.kind === "update" &&
        row.change.columns.map((column) => (
          <div key={column.column} className="action-column">
            <span className="mono o2">{column.column}</span>
            <Value value={column.old} />
            <span className="o3">→</span>
            <Value value={column.new} />
          </div>
        ))}
    </div>
  );
}

function Writes({ preview }: { preview: ActionPreview }) {
  if (preview.sql.length > 0) {
    return (
      <div className="action-writes">
        <p className="muted">Free SQL: affects rows not known in advance.</p>
        {preview.sql.map((statement) => (
          <pre key={statement} className="action-sql">{statement}</pre>
        ))}
      </div>
    );
  }
  return (
    <div className="action-writes">
      {preview.rows.map((row) => (
        <RowWrites key={`${row.table}:${row.key.map((part) => part.value).join(",")}`} row={row} />
      ))}
    </div>
  );
}

function summary(preview: ActionPreview): string {
  const rows = preview.sql.length > 0 ? "rows not known in advance" : plural(preview.rows.length, "row", "rows");
  return `${rows} · ${preview.canUndo ? "undo available" : "no undo"}`;
}

/** Runs a rule action: its params, and every row and value it writes, previewed by Rust as the params change. */
export function ActionSheet({ address, action }: { address: Address; action: ObjectAction }) {
  const close = useStore((s) => s.closeDialog);
  const session = useStore((s) => selectSessionBySlug(s, address.connection));
  const [params, setParams] = useState<Record<string, string>>(() => Object.fromEntries(action.params.map((p) => [p.name, initialValue(p)])));
  const key = session ? JSON.stringify(params) : null;
  const result = useApiResult(key, () => api.previewAction(session!.id, formatAddress(address), action.name, params), { delayMs: PREVIEW_DELAY_MS, keepData: true });
  const preview = result.state === "loaded" ? result.data : null;
  const apply = bindingLabel(bindingFor("changes.apply"));

  const queue = async () => {
    if (!preview || !session) return;
    if (await useStore.getState().queueActionPreview(session.id, preview.id)) close();
  };

  return (
    <Modal onClose={close} labelledBy="action-title" className="wide action-sheet">
      <form
        className="form-contents"
        onSubmit={(e) => {
          e.preventDefault();
          void queue();
        }}
      >
        <div>
          <h2 id="action-title" className="mono">{action.name}</h2>
          <div className="o3">
            {objectTitle(address)} · {action.preset}
          </div>
          <div className="o3">writes {writesSummary(action)}</div>
        </div>
        {action.params.map((param) => (
          <ParamField key={param.name} param={param} value={params[param.name] ?? ""} onChange={(value) => setParams((p) => ({ ...p, [param.name]: value }))} />
        ))}
        {result.state === "failed" && <p className="action-error">{result.error.message}</p>}
        {result.data && <Writes preview={result.data} />}
        <p className="muted" style={{ margin: 0 }}>
          {result.data ? `${summary(result.data)}. ` : "Previewing… "}
          The change is queued; review its SQL in the pending tray before {apply || "applying it"}.
        </p>
        <div className="actions">
          <button data-stage="dialog:cancel" type="button" className="btn" onClick={close}>
            Cancel
          </button>
          <button data-stage="dialog:confirm" type="submit" className="btn btn-primary" disabled={!preview}>
            Queue change
          </button>
        </div>
      </form>
    </Modal>
  );
}
