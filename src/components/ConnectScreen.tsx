import { useEffect, useState } from "react";
import { Database, Trash2 } from "lucide-react";
import { api, type AppError, type ConnectionSpec, type RecentConnection } from "../api";
import { relativeTime } from "../lib/format";
import { fail, useStore } from "../state/store";

/** Heading id of the form, for the connect sheet's `aria-labelledby`. */
export const CONNECT_TITLE_ID = "connect-title";

interface FormState {
  name: string;
  host: string;
  port: string;
  socket: string;
  useSocket: boolean;
  user: string;
  password: string;
  database: string;
  savePassword: boolean;
}

function toForm(spec: ConnectionSpec | undefined, defaultPort: number): FormState {
  return {
    name: spec?.name ?? "",
    host: spec?.host ?? "127.0.0.1",
    port: String(spec?.port ?? defaultPort),
    socket: spec?.socket ?? "",
    useSocket: !!spec?.socket,
    user: spec?.user ?? "root",
    password: "",
    database: spec?.database ?? "",
    savePassword: true,
  };
}

/** Returns the spec, or an error message describing the first invalid field. */
export function validate(form: FormState, defaultPort: number): ConnectionSpec | string {
  const port = Number(form.port);
  if (!form.useSocket && !form.host.trim()) return "Host is required";
  if (!form.useSocket && (!Number.isInteger(port) || port < 1 || port > 65535)) return "Port must be between 1 and 65535";
  if (form.useSocket && !form.socket.trim()) return "Socket path is required";
  if (!form.user.trim()) return "User is required";
  return {
    name: form.name.trim() || null,
    host: form.host.trim() || "127.0.0.1",
    port: Number.isInteger(port) && port > 0 ? port : defaultPort,
    socket: form.useSocket ? form.socket.trim() : null,
    user: form.user.trim(),
    database: form.database.trim() || null,
  };
}

/** The recent's title; its label for Stage and screen readers uses the same text. */
const recentTitle = (spec: ConnectionSpec) => spec.name ?? spec.database ?? spec.host;

function describe(spec: ConnectionSpec) {
  const where = spec.socket ?? `${spec.host}:${spec.port}`;
  return `${spec.user}@${where}${spec.database ? ` / ${spec.database}` : ""}`;
}

export function ConnectForm({ initial, error }: { initial?: ConnectionSpec; error?: AppError }) {
  const defaultPort = useStore((s) => s.settings?.defaultPort ?? 3306);
  const connect = useStore((s) => s.connect);
  const [form, setForm] = useState(() => toForm(initial, defaultPort));
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recents, setRecents] = useState<RecentConnection[]>([]);

  useEffect(() => {
    api.listRecents().then(setRecents, () => setRecents([]));
  }, []);

  const set = <K extends keyof FormState>(key: K, value: FormState[K]) => setForm((f) => ({ ...f, [key]: value }));

  const submit = async (spec: ConnectionSpec, password?: string, savePassword?: boolean) => {
    setBusy(true);
    await connect({ spec, password: password || undefined, savePassword });
    setBusy(false);
  };

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const result = validate(form, defaultPort);
    if (typeof result === "string") {
      setProblem(result);
      return;
    }
    setProblem(null);
    void submit(result, form.password, form.savePassword);
  };

  const removeRecent = async (identity: string) => {
    try {
      await api.removeRecent(identity);
      setRecents((r) => r.filter((x) => x.identity !== identity));
    } catch (e) {
      fail(e, "Could not remove the recent connection");
    }
  };

  const shownError = problem ?? error?.message;

  return (
    <div className="connect-card">
      <form className="connect-form" onSubmit={onSubmit} noValidate data-stage="connect:form" data-stage-label="New connection">
        <h1 id={CONNECT_TITLE_ID}>New connection</h1>
        <p className="muted" style={{ margin: "-6px 0 2px" }}>MySQL or MariaDB, local or in Docker.</p>
        {shownError && (
          <div className="form-error" role="alert">
            {shownError}
          </div>
        )}
        <label className="field">
          <span>Name (optional)</span>
          <input data-stage="connect:name" className="input" value={form.name} onChange={(e) => set("name", e.target.value)} placeholder="Shop (DDEV)" />
        </label>
        <div className="segmented" style={{ alignSelf: "flex-start" }}>
          <button data-stage="connect:tcp" type="button" aria-pressed={!form.useSocket} onClick={() => set("useSocket", false)}>TCP</button>
          <button data-stage="connect:socket" type="button" aria-pressed={form.useSocket} onClick={() => set("useSocket", true)}>Socket</button>
        </div>
        {form.useSocket ? (
          <label className="field">
            <span>Socket path</span>
            <input data-stage="connect:socket-path" className="input mono" value={form.socket} onChange={(e) => set("socket", e.target.value)} placeholder="/tmp/mysql.sock" />
          </label>
        ) : (
          <div className="field-row">
            <label className="field" style={{ flex: 3 }}>
              <span>Host</span>
              <input data-stage="connect:host" className="input" value={form.host} onChange={(e) => set("host", e.target.value)} />
            </label>
            <label className="field" style={{ flex: 1 }}>
              <span>Port</span>
              <input data-stage="connect:port" className="input" inputMode="numeric" value={form.port} onChange={(e) => set("port", e.target.value)} />
            </label>
          </div>
        )}
        <div className="field-row">
          <label className="field">
            <span>User</span>
            <input data-stage="connect:user" className="input" value={form.user} onChange={(e) => set("user", e.target.value)} />
          </label>
          <label className="field">
            <span>Password</span>
            <input data-stage="connect:password" className="input" type="password" value={form.password} onChange={(e) => set("password", e.target.value)} placeholder="Keychain" />
          </label>
        </div>
        <label className="field">
          <span>Database (optional)</span>
          <input data-stage="connect:database" className="input" value={form.database} onChange={(e) => set("database", e.target.value)} />
        </label>
        <label className="check">
          <input type="checkbox" checked={form.savePassword} onChange={(e) => set("savePassword", e.target.checked)} />
          Save password in Keychain
        </label>
        <button data-stage="connect:connect" type="submit" className="btn btn-primary" disabled={busy} style={{ alignSelf: "flex-end", height: "2.154rem", padding: "0 16px" }}>
          {busy ? "Connecting…" : "Connect"}
        </button>
      </form>
      <aside className="recents" data-stage="connect:recents" data-stage-label="Recent connections">
        <h2>Recent</h2>
        {recents.length === 0 && <div className="sidebar-empty">Connections you open appear here.</div>}
        {recents.map((r) => (
          <div key={r.identity} className="recent">
            <button
              type="button"
              className="recent-main"
              data-stage={`recent:${r.identity}`}
              data-stage-list="recents"
              data-stage-label={recentTitle(r.spec)}
              aria-label={`Connect to ${recentTitle(r.spec)}`}
              onDoubleClick={() => void submit(r.spec)}
              onClick={() => setForm(toForm(r.spec, defaultPort))}
            >
              <Database size="1.231rem" className="muted" />
              <div className="info">
                <div className="title">{recentTitle(r.spec)}</div>
                <div className="sub">
                  {describe(r.spec)} · {relativeTime(r.lastUsed)}
                </div>
              </div>
            </button>
            <button type="button" className="icon-btn" aria-label={`Remove ${r.identity}`} onClick={() => void removeRecent(r.identity)}>
              <Trash2 size="1rem" />
            </button>
          </div>
        ))}
        {recents.length > 0 && <div className="hint" style={{ padding: "8px" }}>Click to edit, double-click to connect.</div>}
      </aside>
    </div>
  );
}

export function ConnectScreen() {
  const dialog = useStore((s) => s.dialog);
  const defaultPort = useStore((s) => s.settings?.defaultPort ?? 3306);
  const state = dialog?.type === "connect" ? dialog : undefined;
  return (
    <div className="connect-screen">
      <div className="drag-strip" data-tauri-drag-region />
      {/* The form reads the default port once; the key remounts it when the configured one loads. */}
      <ConnectForm key={`${defaultPort}:${state?.spec ? JSON.stringify(state.spec) : "empty"}`} initial={state?.spec} error={state?.error} />
    </div>
  );
}
