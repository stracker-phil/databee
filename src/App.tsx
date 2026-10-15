import { useEffect } from "react";
import { api } from "./api";
import { ConnectScreen } from "./components/ConnectScreen";
import { Dialogs } from "./components/Dialogs";
import { FavoritesBar } from "./components/FavoritesBar";
import { Inspector } from "./components/Inspector";
import { Sidebar } from "./components/Sidebar";
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

export default function App() {
  const settings = useStore((s) => s.settings);
  const hasSessions = useStore((s) => s.sessions.length > 0);
  const inspecting = useStore((s) => s.selectedCell !== null);
  const wpMode = useStore((s) => s.mode === "wp" && !!selectWp(s));
  useBackendEvents();

  useEffect(() => {
    if (settings) {
      applyAppearance(settings);
      document.documentElement.dataset.screenShare = settings.screenShareMode ? "on" : "off";
    }
  }, [settings]);

  return (
    <>
      {hasSessions ? (
        <div className="app">
          <Toolbar />
          <FavoritesBar />
          {wpMode ? (
            <WpWorkspace />
          ) : (
            <div className={`workspace${inspecting ? " with-inspector" : ""}`}>
              <Sidebar />
              <main className="main">
                <TabBar />
                <TabPanels />
              </main>
              {inspecting && <Inspector />}
            </div>
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
