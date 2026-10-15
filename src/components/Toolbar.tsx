import { FilePlus2, PlugZap, Settings as SettingsIcon, Unplug } from "lucide-react";
import { selectActiveSession, useStore } from "../state/store";

export function Toolbar() {
  const sessions = useStore((s) => s.sessions);
  const session = useStore(selectActiveSession);
  const schema = useStore((s) => (session ? s.schema[session.id] : undefined));
  const showSystem = useStore((s) => s.settings?.showSystemSchemas ?? false);
  const { setActiveSession, switchDatabase, disconnect, openQueryTab, openDialog } = useStore.getState();

  const databases = (schema?.databases ?? []).filter((d) => showSystem || !d.isSystem || d.name === session?.database);

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
          <select
            aria-label="Database"
            value={session.database ?? ""}
            onChange={(e) => void switchDatabase(session.id, e.target.value)}
          >
            {!session.database && <option value="">No database</option>}
            {databases.map((d) => (
              <option key={d.name} value={d.name}>{d.name}</option>
            ))}
          </select>
          <button className="icon-btn" aria-label="Disconnect" title="Disconnect" onClick={() => void disconnect(session.id)}>
            <Unplug size={14} />
          </button>
        </div>
      ) : (
        <span className="app-title">databee</span>
      )}
      <div className="spacer" data-tauri-drag-region />
      <button className="icon-btn" aria-label="New connection" title="New connection (⌘N)" onClick={() => openDialog({ type: "connect" })}>
        <PlugZap size={16} />
      </button>
      <button className="icon-btn" aria-label="New query" title="New query (⌘T)" disabled={!session} onClick={() => openQueryTab()}>
        <FilePlus2 size={16} />
      </button>
      <button className="icon-btn" aria-label="Settings" title="Settings (⌘,)" onClick={() => openDialog({ type: "settings" })}>
        <SettingsIcon size={16} />
      </button>
    </header>
  );
}
