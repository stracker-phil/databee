import { Database, FilePlus2, LayoutDashboard, PlugZap, Search, Settings as SettingsIcon, Unplug } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { useBindingLabel } from "../commands";
import { selectActiveSession, selectVisibleDatabases, selectWp, useStore } from "../state/store";
import { KeyCap } from "./Field";
import { Path } from "./Path";

type Session = NonNullable<ReturnType<typeof selectActiveSession>>;

/** Every icon in the toolbar's button row has this size. */
const ICON_SIZE = "1.077rem";

/**
 * Connection and database as plain mono text. A transparent native select covers each name, so a click opens
 * the existing picker as a menu while the toolbar shows only text.
 */
function ConnectionText({ session }: { session: Session }) {
  const sessions = useStore((s) => s.sessions);
  const databases = useStore(useShallow(selectVisibleDatabases));
  const { setActiveSession, switchDatabase } = useStore.getState();
  return (
    <div className="conn-text mono" data-stage="toolbar:connection" data-stage-label={`Connection ${session.identity}`} title={`${session.identity} · ${session.serverVersion}`}>
      <span className="menu-text">
        <span>{session.displayName}</span>
        {sessions.length > 1 && (
          <select aria-label="Connection" value={session.id} onChange={(e) => setActiveSession(e.target.value)}>
            {sessions.map((s) => (
              <option key={s.id} value={s.id}>{s.displayName}</option>
            ))}
          </select>
        )}
      </span>
      <span className="o3"> / </span>
      <span className="menu-text o2">
        <span>{session.database ?? "no database"}</span>
        <select data-stage="toolbar:database" aria-label="Database" value={session.database ?? ""} onChange={(e) => void switchDatabase(session.id, e.target.value)}>
          {!session.database && <option value="">No database</option>}
          {databases.map((d) => (
            <option key={d.name} value={d.name}>{d.name}</option>
          ))}
        </select>
      </span>
    </div>
  );
}

function ModeSwitch() {
  const mode = useStore((s) => s.mode);
  const setMode = useStore((s) => s.setMode);
  return (
    <div className="segmented mode-switch" role="group" aria-label="Mode" data-stage="toolbar:mode">
      <button data-stage="toolbar:mode:wp" aria-pressed={mode === "wp"} onClick={() => setMode("wp")} title="WordPress dashboard">
        <LayoutDashboard size="0.923rem" /> WordPress
      </button>
      <button data-stage="toolbar:mode:db" aria-pressed={mode === "db"} onClick={() => setMode("db")} title="Tables and SQL">
        <Database size="0.923rem" /> Database
      </button>
    </div>
  );
}

/** Opens the command palette on click; it is not a text input. */
function SearchField() {
  const keys = useBindingLabel("palette.open");
  return (
    <button type="button" className="inset-field toolbar-search" aria-label="Go to" data-stage="toolbar:search" onClick={() => useStore.getState().setPaletteOpen(true)}>
      <Search size="0.923rem" className="inset-field-icon" aria-hidden="true" />
      <span className="placeholder">Order ID, email, PayPal ID, table…</span>
      <KeyCap keys={keys} />
    </button>
  );
}

export function Toolbar() {
  const session = useStore(selectActiveSession);
  const wp = useStore(selectWp);
  const { disconnect, openQueryTab, openDialog, setMode } = useStore.getState();

  return (
    <header className="toolbar" data-tauri-drag-region data-stage="toolbar" data-stage-label="Toolbar">
      {session ? (
        <>
          <ConnectionText session={session} />
          <button data-stage="toolbar:disconnect" className="icon-btn" aria-label="Disconnect" title="Disconnect" onClick={() => void disconnect(session.id)}>
            <Unplug size={ICON_SIZE} />
          </button>
        </>
      ) : (
        <span className="app-title">DataBee</span>
      )}
      {wp && <ModeSwitch />}
      {session && <Path />}
      <div className="spacer" data-tauri-drag-region />
      {session && <SearchField />}
      <button data-stage="toolbar:new-connection" className="icon-btn" aria-label="New connection" title="New connection (⌘N)" onClick={() => openDialog({ type: "connect" })}>
        <PlugZap size={ICON_SIZE} />
      </button>
      <button data-stage="toolbar:new-query" className="icon-btn" aria-label="New query" title="New query (⌘T)" disabled={!session} onClick={() => (setMode("db"), openQueryTab())}>
        <FilePlus2 size={ICON_SIZE} />
      </button>
      <button data-stage="toolbar:settings" className="icon-btn" aria-label="Settings" title="Settings (⌘,)" onClick={() => openDialog({ type: "settings" })}>
        <SettingsIcon size={ICON_SIZE} />
      </button>
    </header>
  );
}
