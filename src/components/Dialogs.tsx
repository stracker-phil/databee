import { useState } from "react";
import { KeyRound } from "lucide-react";
import { accountOf } from "../api";
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
        <button className="btn" onClick={close}>Cancel</button>
        <button
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
          <button type="button" className="btn" onClick={close}>Cancel</button>
          <button type="submit" className="btn btn-primary" disabled={!text.trim()}>Save</button>
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
        <h2 id="pw-title" style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <KeyRound size={16} /> Password for {accountOf(spec)}
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
          <button type="button" className="btn" onClick={close}>Cancel</button>
          <button type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? "Connecting…" : "Connect"}
          </button>
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
