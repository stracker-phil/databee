import { useEffect, useRef } from "react";
import { api, toAppError } from "./api";
import { ConnectScreen } from "./components/ConnectScreen";
import { Dialogs } from "./components/Dialogs";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TabBar, TabPanels } from "./components/Tabs";
import { Toasts } from "./components/Toasts";
import { Toolbar } from "./components/Toolbar";
import { CommandPalette } from "./components/CommandPalette";
import { handleKeydown, installCommands, runMenuCommand } from "./commands";
import { answerRemote } from "./commands/remote";
import { PendingTray } from "./components/PendingTray";
import { PeekOverlay } from "./components/value/Peek";
import { useValueHover } from "./components/value/useValueHover";
import { applyAppearance, syncRowHeight, useSystemDark } from "./lib/theme";
import { flushUiState, selectSidebarHidden, useStore } from "./state/store";
import "./styles/fonts.css";
import "./styles/schemes/ayu-mirage.css";
import "./styles/schemes/ayu-light.css";
import "./styles/theme.css";
import "./styles/app.css";

function useBackendEvents() {
  useEffect(() => {
    const { loadSettings, handleLaunchEvents, toast } = useStore.getState();
    const drainLaunches = () =>
      api.takeLaunchEvents().then(handleLaunchEvents, (e) => toast("error", `Launch request failed: ${toAppError(e).message}`));

    // Re-installing applies the user's keybinding overrides, which arrive with the settings.
    void loadSettings().then(() => (installCommands(), drainLaunches()));
    const unlisten = [api.onLaunch(() => void drainLaunches()), api.onCommand(runMenuCommand), api.onRemote((e) => void answerRemote(e))];

    const onKey = handleKeydown;
    const onError = (e: ErrorEvent) => toast("error", `Unexpected error: ${e.message}`);
    const onRejection = (e: PromiseRejectionEvent) => toast("error", `Unexpected error: ${toAppError(e.reason).message}`);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    window.addEventListener("beforeunload", flushUiState);
    return () => {
      unlisten.forEach((p) => void p.then((fn) => fn()).catch(() => {}));
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
      window.removeEventListener("beforeunload", flushUiState);
    };
  }, []);
}

/**
 * Sidebar and tabs. Hiding the sidebar removes it entirely (no rail); its child slot stays (as `false`), so
 * `main` keeps its tree position and the editors and table pages survive.
 */
function Workspace() {
  const sidebarHidden = useStore(selectSidebarHidden);
  return (
    <div className={sidebarHidden ? "workspace no-sidebar" : "workspace"}>
      {!sidebarHidden && <Sidebar />}
      <main className="main">
        <TabBar />
        <TabPanels />
      </main>
    </div>
  );
}

function AppShell({ children }: { children: React.ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useValueHover(ref);
  return (
    <div className="app" ref={ref}>
      <Toolbar />
      {children}
      <PendingTray />
      <StatusBar />
      <PeekOverlay />
    </div>
  );
}

function Main() {
  const hasSessions = useStore((s) => s.sessions.length > 0);
  if (!hasSessions) return <ConnectScreen />;
  return (
    <AppShell>
      <Workspace />
    </AppShell>
  );
}

export default function App() {
  const settings = useStore((s) => s.settings);
  const systemDark = useSystemDark();
  useBackendEvents();

  useEffect(() => {
    if (settings) {
      applyAppearance(settings, systemDark);
      syncRowHeight();
    }
  }, [settings, systemDark]);

  return (
    <>
      <Main />
      <Dialogs />
      <CommandPalette />
      <Toasts />
    </>
  );
}
