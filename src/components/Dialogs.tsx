import { useId, useState } from "react";
import { KeyRound } from "lucide-react";
import { type ColumnInfo, type ConnectionSpec, type Home } from "../api";
import { DEFAULT_PIN_ROWS, pinChoice, pinOf, type PinChoice } from "../lib/pins";
import { useStore } from "../state/store";
import { Modal } from "./Modal";
import { CONNECT_TITLE_ID, ConnectForm } from "./ConnectScreen";
import { SettingsDialog } from "./settings/SettingsDialog";
import { ShortcutsDialog } from "./ShortcutsDialog";

function ConfirmDialog({ title, message, confirmLabel, onConfirm }: { title: string; message: string; confirmLabel: string; onConfirm: () => void }) {
  const close = useStore((s) => s.closeDialog);
  return (
    <Modal onClose={close} labelledBy="confirm-title">
      <h2 id="confirm-title">{title}</h2>
      <p className="muted" style={{ margin: 0 }}>{message}</p>
      <div className="actions">
        <button data-stage="dialog:cancel" className="btn" onClick={close}>Cancel</button>
        <button
          data-stage="dialog:confirm"
          className="btn btn-danger"
          autoFocus
          onClick={() => {
            close();
            onConfirm();
          }}
        >
          {confirmLabel}
        </button>
      </div>
    </Modal>
  );
}

function PromptDialog({ title, value, onSubmit }: { title: string; value: string; onSubmit: (v: string) => void }) {
  const close = useStore((s) => s.closeDialog);
  const [text, setText] = useState(value);
  return (
    <Modal onClose={close} labelledBy="prompt-title">
      <form
        className="form-contents"
        onSubmit={(e) => {
          e.preventDefault();
          if (!text.trim()) return;
          close();
          onSubmit(text.trim());
        }}
      >
        <h2 id="prompt-title">{title}</h2>
        <input className="input" autoFocus aria-label={title} value={text} onChange={(e) => setText(e.target.value)} />
        <div className="actions">
          <button data-stage="dialog:cancel" type="button" className="btn" onClick={close}>Cancel</button>
          <button data-stage="dialog:confirm" type="submit" className="btn btn-primary" disabled={!text.trim()}>Save</button>
        </div>
      </form>
    </Modal>
  );
}

const MAX_PIN_ROWS = 255;

function PinDialog({ title, pin, onSubmit }: { title: string; pin: Home | null; onSubmit: (pin: Home | null) => void }) {
  const close = useStore((s) => s.closeDialog);
  const [choice, setChoice] = useState<PinChoice>(pinChoice(pin));
  const [rows, setRows] = useState(String(pin?.show === "rows" ? (pin.limit ?? DEFAULT_PIN_ROWS) : DEFAULT_PIN_ROWS));
  const limit = Math.min(MAX_PIN_ROWS, Math.max(1, Math.trunc(Number(rows)) || DEFAULT_PIN_ROWS));
  const option = (value: PinChoice, label: string) => (
    <label className="check">
      <input type="radio" name="pin" data-stage={`dialog:pin:${value}`} checked={choice === value} onChange={() => setChoice(value)} />
      {label}
    </label>
  );
  return (
    <Modal onClose={close} labelledBy="pin-title">
      <form
        className="form-contents"
        onSubmit={(e) => {
          e.preventDefault();
          close();
          onSubmit(pinOf(choice, limit));
        }}
      >
        <h2 id="pin-title">Pin {title}</h2>
        {option("none", "Not pinned")}
        {option("count", "Count")}
        {option("value", "Value")}
        <div className="pin-rows">
          {option("rows", "Rows")}
          <input
            className="input"
            type="number"
            min={1}
            max={MAX_PIN_ROWS}
            aria-label="Rows to show"
            data-stage="dialog:pin:limit"
            value={rows}
            disabled={choice !== "rows"}
            onChange={(e) => setRows(e.target.value)}
          />
        </div>
        <div className="actions">
          <button data-stage="dialog:cancel" type="button" className="btn" onClick={close}>Cancel</button>
          <button data-stage="dialog:confirm" type="submit" className="btn btn-primary">Save</button>
        </div>
      </form>
    </Modal>
  );
}

export function PasswordDialog({
  spec,
  account,
  error,
  savePassword,
}: {
  spec: ConnectionSpec;
  account: string | null;
  error?: string;
  savePassword: boolean;
}) {
  const close = useStore((s) => s.closeDialog);
  const connect = useStore((s) => s.connect);
  const [password, setPassword] = useState("");
  const [save, setSave] = useState(savePassword);
  const [busy, setBusy] = useState(false);
  return (
    <Modal onClose={close} labelledBy="pw-title">
      <form
        className="form-contents"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          await connect({ spec, password, savePassword: save });
          setBusy(false);
        }}
      >
        <h2 id="pw-title" style={{ display: "flex", gap: "0.615rem", alignItems: "center" }}>
          <KeyRound size="1.231rem" /> {account ? `Password for ${account}` : "Password"}
        </h2>
        {error && <div className="form-error">{error}</div>}
        <input
          className="input"
          type="password"
          autoFocus
          aria-label="Password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <label className="check">
          <input type="checkbox" checked={save} onChange={(e) => setSave(e.target.checked)} />
          Save in Keychain
        </label>
        <div className="actions">
          <button data-stage="dialog:cancel" type="button" className="btn" onClick={close}>Cancel</button>
          <button data-stage="dialog:confirm" type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? "Connecting…" : "Connect"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

/** How a column is inserted: a typed value (the string "NULL" too), SQL NULL, or left to its default. */
type InsertMode = "value" | "null" | "default";

function InsertField({ column, mode, value, onChange }: { column: ColumnInfo; mode: InsertMode; value: string; onChange: (mode: InsertMode, value: string) => void }) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>
        {column.name} <span className="muted">{column.columnType}{column.nullable ? "" : " · required"}</span>
      </label>
      <div className="insert-field">
        <input
          id={id}
          className="input mono"
          placeholder={mode === "null" ? "NULL" : mode === "default" ? (column.default ?? "default") : ""}
          value={mode === "value" ? value : ""}
          onChange={(e) => onChange("value", e.target.value)}
        />
        <select className="input" aria-label={`${column.name} mode`} value={mode} onChange={(e) => onChange(e.target.value as InsertMode, value)}>
          <option value="value">Value</option>
          {column.nullable && <option value="null">NULL</option>}
          <option value="default">Default</option>
        </select>
      </div>
    </div>
  );
}

function InsertRowDialog({ database, table, columns }: { database: string; table: string; columns: ColumnInfo[] }) {
  const close = useStore((s) => s.closeDialog);
  const queueChange = useStore((s) => s.queueChange);
  const [fields, setFields] = useState<Record<string, { mode: InsertMode; value: string }>>({});
  const editable = columns.filter((c) => !c.autoIncrement && !c.generated);
  const fieldOf = (name: string) => fields[name] ?? { mode: "default" as const, value: "" };
  return (
    <Modal onClose={close} className="wide" labelledBy="insert-title">
      <form
        className="form-contents"
        onSubmit={async (e) => {
          e.preventDefault();
          const values: [string, string | null][] = editable
            .map((c) => [c.name, fieldOf(c.name)] as const)
            .filter(([, f]) => f.mode !== "default")
            .map(([name, f]) => [name, f.mode === "null" ? null : f.value]);
          if (await queueChange({ type: "insertRow", database, table, values })) close();
        }}
      >
        <h2 id="insert-title">Insert into {table}</h2>
        <p className="hint" style={{ margin: 0 }}>Typing sets a value; Default leaves the column out of the INSERT.</p>
        <div className="insert-grid">
          {editable.map((c) => (
            <InsertField
              key={c.name}
              column={c}
              {...fieldOf(c.name)}
              onChange={(mode, value) => setFields((f) => ({ ...f, [c.name]: { mode, value } }))}
            />
          ))}
        </div>
        <div className="actions">
          <button data-stage="dialog:cancel" type="button" className="btn" onClick={close}>Cancel</button>
          <button data-stage="dialog:confirm" type="submit" className="btn btn-primary">Queue insert</button>
        </div>
      </form>
    </Modal>
  );
}

export function Dialogs() {
  const dialog = useStore((s) => s.dialog);
  const hasSessions = useStore((s) => s.sessions.length > 0);
  const close = useStore((s) => s.closeDialog);
  if (!dialog) return null;
  switch (dialog.type) {
    case "confirm":
      return <ConfirmDialog {...dialog} />;
    case "prompt":
      return <PromptDialog {...dialog} />;
    case "pin":
      return <PinDialog {...dialog} />;
    case "password":
      return <PasswordDialog key={dialog.account ?? ""} {...dialog} />;
    case "settings":
      return <SettingsDialog />;
    case "insertRow":
      return <InsertRowDialog {...dialog} />;
    case "shortcuts":
      return <ShortcutsDialog scopes={dialog.scopes} />;
    case "connect":
      // Without sessions the connect screen is the main view and shows this state itself.
      if (!hasSessions) return null;
      return (
        <Modal onClose={close} className="connect-dialog" labelledBy={CONNECT_TITLE_ID}>
          <ConnectForm initial={dialog.spec} error={dialog.error} />
        </Modal>
      );
  }
}
