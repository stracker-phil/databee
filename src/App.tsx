import { useEffect } from "react";
import { api } from "./api";
import { ConnectScreen } from "./components/ConnectScreen";
import { Dialogs } from "./components/Dialogs";
import { FavoritesBar } from "./components/FavoritesBar";
import { Inspector } from "./components/Inspector";
import { Sidebar } from "./components/Sidebar";
import { DEFAULT_PANE_HEIGHT, Splitter, usePaneSize, useWindowSize } from "./components/Splitter";
import { StatusBar } from "./components/StatusBar";
import { TabBar, TabPanels } from "./components/Tabs";
import { Toasts } from "./components/Toasts";
import { Toolbar } from "./components/Toolbar";
import { CommandPalette } from "./components/CommandPalette";
import { handleKeydown, installCommands, runCommand } from "./commands";
import { PendingTray } from "./components/PendingTray";
import { WpWorkspace } from "./components/wp/WpWorkspace";
import { applyAppearance } from "./lib/theme";
import { selectWp, useStore } from "./state/store";
import "./styles/theme.css";
import "./styles/app.css";

function useBackendEvents() {
  useEffect(() => {
    const { loadSettings, handleLaunchEvents, toast } = useStore.getState();
    const drainLaunches = () =>
      api.takeLaunchEvents().then(handleLaunchEvents, (e) => toast("error", `Launch request failed: ${String(e)}`));

    installCommands();
    void loadSettings().then(() => (installCommands(), drainLaunches()));
    const unlisten = [api.onLaunch(() => void drainLaunches()), api.onCommand(runCommand)];

    const onKey = handleKeydown;
    const onError = (e: ErrorEvent) => toast("error", `Unexpected error: ${e.message}`);
    const onRejection = (e: PromiseRejectionEvent) => toast("error", `Unexpected error: ${String(e.reason?.message ?? e.reason)}`);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      unlisten.forEach((p) => void p.then((fn) => fn()).catch(() => {}));
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, []);
}

/** Sidebar, tabs and the value inspector, docked right or below the tabs. */
function DbWorkspace() {
  const inspecting = useStore((s) => s.selectedCell !== null);
  const bottom = useStore((s) => s.settings?.inspectorDock === "bottom");
  const { width, height } = useWindowSize();
  const side = usePaneSize("inspectorWidth", 360, 240, width - 700);
  const below = usePaneSize("paneHeight", DEFAULT_PANE_HEIGHT, 160, height - 260);
  const main = (
    <main className="main">
      <TabBar />
      <TabPanels />
    </main>
  );
  if (!inspecting) {
    return (
      <div className="workspace">
        <Sidebar />
        {main}
      </div>
    );
  }
  if (bottom) {
    return (
      <div className="workspace">
        <Sidebar />
        <div className="main-stack dock-bottom" style={{ gridTemplateRows: `minmax(0, 1fr) auto ${below.size}px` }}>
          {main}
          <Splitter axis="y" paneAfter gap label="Resize inspector" {...below} />
          <Inspector />
        </div>
      </div>
    );
  }
  return (
    <div className="workspace" style={{ gridTemplateColumns: `19.231rem minmax(0, 1fr) auto ${side.size}px` }}>
      <Sidebar />
      {main}
      <Splitter axis="x" paneAfter gap label="Resize inspector" {...side} />
      <Inspector />
    </div>
  );
}

export default function App() {
  const settings = useStore((s) => s.settings);
  const hasSessions = useStore((s) => s.sessions.length > 0);
  const wpMode = useStore((s) => s.mode === "wp" && !!selectWp(s));
  useBackendEvents();

  useEffect(() => {
    if (settings) {
      applyAppearance(settings);
    }
  }, [settings]);

  return (
    <>
      {hasSessions ? (
        <div className="app">
          <Toolbar />
          {!wpMode && <FavoritesBar />}
          {wpMode ? (
            <WpWorkspace />
          ) : (
            <DbWorkspace />
          )}
          <PendingTray />
          <StatusBar />
        </div>
      ) : (
        <ConnectScreen />
      )}
      <Dialogs />
      <CommandPalette />
      <Toasts />
    </>
  );
}
