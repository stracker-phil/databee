import { useEffect } from "react";
import { api } from "./api";
import { ConnectScreen } from "./components/ConnectScreen";
import { Dialogs } from "./components/Dialogs";
import { FavoritesBar } from "./components/FavoritesBar";
import { Inspector } from "./components/Inspector";
import { FOCUS_FILTER_EVENT, Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TabBar, TabPanels } from "./components/Tabs";
import { Toasts } from "./components/Toasts";
import { Toolbar } from "./components/Toolbar";
import { applyAppearance } from "./lib/theme";
import { useStore } from "./state/store";
import "./styles/theme.css";
import "./styles/app.css";

export function runMenuAction(id: string) {
  const s = useStore.getState();
  const active = s.tabs.find((t) => t.id === s.activeTabId);
  switch (id) {
    case "new_connection":
      return s.openDialog({ type: "connect" });
    case "new_query":
      return s.openQueryTab();
    case "close_tab":
      return active && s.closeTab(active.id);
    case "add_favorite":
      return s.addCurrentTabToFavorites();
    case "settings":
      return s.openDialog({ type: "settings" });
  }
}

function useBackendEvents() {
  useEffect(() => {
    const { loadSettings, handleLaunchEvents, toast } = useStore.getState();
    const drainLaunches = () =>
      api.takeLaunchEvents().then(handleLaunchEvents, (e) => toast("error", `Launch request failed: ${String(e)}`));

    void loadSettings().then(drainLaunches);
    const unlisten = [api.onLaunch(() => void drainLaunches()), api.onMenu(runMenuAction)];

    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        window.dispatchEvent(new Event(FOCUS_FILTER_EVENT));
      }
    };
    const onError = (e: ErrorEvent) => toast("error", `Unexpected error: ${e.message}`);
    const onRejection = (e: PromiseRejectionEvent) => toast("error", `Unexpected error: ${String(e.reason?.message ?? e.reason)}`);
    window.addEventListener("keydown", onKey);
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      unlisten.forEach((p) => void p.then((fn) => fn()).catch(() => {}));
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, []);
}

export default function App() {
  const settings = useStore((s) => s.settings);
  const hasSessions = useStore((s) => s.sessions.length > 0);
  const inspecting = useStore((s) => s.selectedCell !== null);
  useBackendEvents();

  useEffect(() => {
    if (settings) applyAppearance(settings);
  }, [settings]);

  return (
    <>
      {hasSessions ? (
        <div className="app">
          <Toolbar />
          <FavoritesBar />
          <div className={`workspace${inspecting ? " with-inspector" : ""}`}>
            <Sidebar />
            <main className="main">
              <TabBar />
              <TabPanels />
            </main>
            {inspecting && <Inspector />}
          </div>
          <StatusBar />
        </div>
      ) : (
        <ConnectScreen />
      )}
      <Dialogs />
      <Toasts />
    </>
  );
}
