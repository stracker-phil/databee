import { useState } from "react";
import { KeyRound } from "lucide-react";
import { accountOf, type ColumnInfo } from "../api";
import { useStore } from "../state/store";
import { Modal } from "./Modal";
import { ConnectForm } from "./ConnectScreen";
import { SettingsDialog } from "./SettingsDialog";

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
        style={{ display: "contents" }}
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

export function PasswordDialog({ spec, error, savePassword }: { spec: Parameters<typeof accountOf>[0]; error?: string; savePassword: boolean }) {
  const close = useStore((s) => s.closeDialog);
  const connect = useStore((s) => s.connect);
  const [password, setPassword] = useState("");
  const [save, setSave] = useState(savePassword);
  const [busy, setBusy] = useState(false);
  return (
    <Modal onClose={close} labelledBy="pw-title">
      <form
        style={{ display: "contents" }}
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          await connect({ spec, password, savePassword: save });
          setBusy(false);
        }}
      >
        <h2 id="pw-title" style={{ display: "flex", gap: "0.615rem", alignItems: "center" }}>
          <KeyRound size="1.231rem" /> Password for {accountOf(spec)}
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

function InsertRowDialog({ database, table, columns }: { database: string; table: string; columns: ColumnInfo[] }) {
  const close = useStore((s) => s.closeDialog);
  const queueChange = useStore((s) => s.queueChange);
  const [values, setValues] = useState<Record<string, string>>({});
  const editable = columns.filter((c) => !c.extra.includes("auto_increment") && !c.extra.includes("GENERATED"));
  return (
    <Modal onClose={close} className="wide" labelledBy="insert-title">
      <form
        style={{ display: "contents" }}
        onSubmit={async (e) => {
          e.preventDefault();
          const entries = Object.entries(values).filter(([, v]) => v !== "");
          if (await queueChange({ type: "insertRow", database, table, values: entries.map(([k, v]) => [k, v === "NULL" ? null : v]) })) close();
        }}
      >
        <h2 id="insert-title">Insert into {table}</h2>
        <p className="hint" style={{ margin: 0 }}>Empty fields use the column default; type NULL for NULL.</p>
        <div className="insert-grid">
          {editable.map((c) => (
            <label key={c.name} className="field">
              <span>
                {c.name} <span className="muted">{c.columnType}{c.nullable ? "" : " · required"}</span>
              </span>
              <input className="input mono" placeholder={c.default ?? ""} value={values[c.name] ?? ""} onChange={(e) => setValues({ ...values, [c.name]: e.target.value })} />
            </label>
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
    case "password":
      return <PasswordDialog key={accountOf(dialog.spec)} {...dialog} />;
    case "settings":
      return <SettingsDialog />;
    case "insertRow":
      return <InsertRowDialog {...dialog} />;
    case "connect":
      // Without sessions the connect screen is the main view and shows this state itself.
      if (!hasSessions) return null;
      return (
        <Modal onClose={close} className="connect-dialog">
          <ConnectForm initial={dialog.spec} error={dialog.error} />
        </Modal>
      );
  }
}
