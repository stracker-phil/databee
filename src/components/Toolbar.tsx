import { Database, Eye, EyeOff, FilePlus2, LayoutDashboard, PlugZap, Settings as SettingsIcon, Unplug } from "lucide-react";
import { selectActiveSession, selectWp, useStore } from "../state/store";

export function Toolbar() {
  const sessions = useStore((s) => s.sessions);
  const session = useStore(selectActiveSession);
  const schema = useStore((s) => (session ? s.schema[session.id] : undefined));
  const settings = useStore((s) => s.settings);
  const wp = useStore(selectWp);
  const mode = useStore((s) => s.mode);
  const { setActiveSession, switchDatabase, disconnect, openQueryTab, openDialog, setMode, saveSettings } = useStore.getState();

  const showSystem = settings?.showSystemSchemas ?? false;
  const databases = (schema?.databases ?? []).filter((d) => showSystem || !d.isSystem || d.name === session?.database);
  const screenShare = settings?.screenShareMode ?? false;

  return (
    <header className="toolbar" data-tauri-drag-region>
      {session ? (
        <div className="conn-pill">
          <span className="dot" title={`Connected · ${session.serverVersion}`} />
          {sessions.length > 1 ? (
            <select aria-label="Connection" value={session.id} onChange={(e) => setActiveSession(e.target.value)}>
              {sessions.map((s) => (
                <option key={s.id} value={s.id}>{s.displayName}</option>
              ))}
            </select>
          ) : (
            <span className="name" title={session.identity}>{session.displayName}</span>
          )}
          <select aria-label="Database" value={session.database ?? ""} onChange={(e) => void switchDatabase(session.id, e.target.value)}>
            {!session.database && <option value="">No database</option>}
            {databases.map((d) => (
              <option key={d.name} value={d.name}>{d.name}</option>
            ))}
          </select>
          <button className="icon-btn" aria-label="Disconnect" title="Disconnect" onClick={() => void disconnect(session.id)}>
            <Unplug size="1.077rem" />
          </button>
        </div>
      ) : (
        <span className="app-title">databee</span>
      )}
      {wp && (
        <div className="segmented mode-switch" role="group" aria-label="Mode">
          <button aria-pressed={mode === "wp"} onClick={() => setMode("wp")} title="WordPress dashboard">
            <LayoutDashboard size="0.923rem" /> WordPress
          </button>
          <button aria-pressed={mode === "db"} onClick={() => setMode("db")} title="Tables and SQL">
            <Database size="0.923rem" /> Database
          </button>
        </div>
      )}
      <div className="spacer" data-tauri-drag-region />
      {settings && (
        <button
          className={`icon-btn${screenShare ? " on" : ""}`}
          aria-label="Screen-share mode"
          aria-pressed={screenShare}
          title={screenShare ? "Screen-share mode on: personal data is blurred" : "Screen-share mode: blur emails, names and addresses"}
          onClick={() => void saveSettings({ ...settings, screenShareMode: !screenShare })}
        >
          {screenShare ? <EyeOff size="1.231rem" /> : <Eye size="1.231rem" />}
        </button>
      )}
      <button className="icon-btn" aria-label="New connection" title="New connection (⌘N)" onClick={() => openDialog({ type: "connect" })}>
        <PlugZap size="1.231rem" />
      </button>
      <button className="icon-btn" aria-label="New query" title="New query (⌘T)" disabled={!session} onClick={() => (setMode("db"), openQueryTab())}>
        <FilePlus2 size="1.231rem" />
      </button>
      <button className="icon-btn" aria-label="Settings" title="Settings (⌘,)" onClick={() => openDialog({ type: "settings" })}>
        <SettingsIcon size="1.231rem" />
      </button>
    </header>
  );
}
