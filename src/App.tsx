import { useEffect, useRef } from "react";
import { api, toAppError } from "./api";
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
import { handleKeydown, installCommands, runMenuCommand } from "./commands";
import { PendingTray } from "./components/PendingTray";
import { useValueHover } from "./components/value/useValueHover";
import { WpWorkspace } from "./components/wp/WpWorkspace";
import { applyAppearance, syncRowHeight, useSystemDark } from "./lib/theme";
import { flushUiState, selectSidebarHidden, selectWp, useStore } from "./state/store";
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
    const unlisten = [api.onLaunch(() => void drainLaunches()), api.onCommand(runMenuCommand)];

    const onKey = handleKeydown;
    const onError = (e: ErrorEvent) => toast("error", `Unexpected error: ${e.message}`);
    const onRejection = (e: PromiseRejectionEvent) => toast("error", `Unexpected error: ${String(e.reason?.message ?? e.reason)}`);
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

/** Room the sidebar and tab area keep when the inspector is dragged wider. */
const RESERVED_WIDTH = 700;
/** Room the tab bar and a few result rows keep when the bottom inspector is dragged taller. */
const RESERVED_HEIGHT = 260;

/**
 * Sidebar, tabs and the value inspector, docked right or below the tabs. `main` keeps its tree position
 * in every layout, so docking never remounts the editors; only the grid template changes. Hiding the
 * sidebar removes it entirely (no rail); its child slot stays (as `false`), so `main` stays mounted too.
 */
function DbWorkspace() {
  const inspecting = useStore((s) => s.selectedCell !== null);
  const bottom = useStore((s) => s.settings?.inspectorDock === "bottom");
  const { width, height } = useWindowSize();
  const side = usePaneSize("inspectorWidth", 360, 240, width - RESERVED_WIDTH);
  const below = usePaneSize("bottomPaneHeight", DEFAULT_PANE_HEIGHT, 160, height - RESERVED_HEIGHT);
  const sidebarHidden = useStore(selectSidebarHidden);
  const pane = bottom ? below : side;
  const classes = ["workspace"];
  if (inspecting) classes.push(bottom ? "dock-bottom" : "dock-right");
  if (sidebarHidden) classes.push("no-sidebar");
  return (
    <div className={classes.join(" ")} style={inspecting ? ({ "--inspector-size": `${pane.size}px` } as React.CSSProperties) : undefined}>
      {!sidebarHidden && <Sidebar />}
      <main className="main">
        <TabBar />
        <TabPanels />
      </main>
      {inspecting && (
        <>
          <Splitter axis={bottom ? "y" : "x"} paneAfter label="Resize inspector" {...pane} />
          <Inspector />
        </>
      )}
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
    </div>
  );
}

function Workspace() {
  const hasSessions = useStore((s) => s.sessions.length > 0);
  const wpMode = useStore((s) => s.mode === "wp" && !!selectWp(s));
  if (!hasSessions) return <ConnectScreen />;
  if (wpMode) {
    return (
      <AppShell>
        <WpWorkspace />
      </AppShell>
    );
  }
  return (
    <AppShell>
      <FavoritesBar />
      <DbWorkspace />
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
      <Workspace />
      <Dialogs />
      <CommandPalette />
      <Toasts />
    </>
  );
}
