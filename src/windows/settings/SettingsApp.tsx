import { useEffect, useState } from "react";
import { api } from "../../api";
import { setOverrides } from "../../commands";
import { SettingsWindow } from "../../components/settings/SettingsWindow";
import { Toasts } from "../../components/Toasts";
import { applyAppearance, syncRowHeight, useSystemDark } from "../../lib/theme";
import { trackWindowFocus } from "../../lib/windowFocus";
import { useStore } from "../../state/store";
import "../../styles/fonts.css";
import "../../styles/schemes/ayu-mirage.css";
import "../../styles/schemes/monokai.css";
import "../../styles/schemes/nord.css";
import "../../styles/schemes/solarized-dark.css";
import "../../styles/schemes/ayu-light.css";
import "../../styles/schemes/catppuccin-latte.css";
import "../../styles/schemes/github-light.css";
import "../../styles/schemes/solarized-light.css";
import "../../styles/theme.css";
import "../../styles/app.css";
import "../../styles/settings.css";

/** The page Rust opened the window at (`?page=`); later pages arrive as `open_settings`. */
function initialPage(): string | undefined {
  return new URLSearchParams(window.location.search).get("page") ?? undefined;
}

/** Loads the settings, and again whenever any window changed them. */
function useSettings() {
  useEffect(() => {
    const load = async () => {
      await useStore.getState().loadSettings();
      setOverrides(useStore.getState().keybindings);
    };
    void load();
    const unlisten = api.onSettingsChanged(() => void load());
    return () => void unlisten.then((fn) => fn()).catch(() => {});
  }, []);
}

function useOpenSettings(setPage: (page: string) => void) {
  useEffect(() => {
    const unlisten = api.onOpenSettings(setPage);
    return () => void unlisten.then((fn) => fn()).catch(() => {});
  }, [setPage]);
}

/** ⌘F focuses the search; Esc hides the window unless a field or a recording shortcut takes it. */
function useWindowKeys() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        document.querySelector<HTMLInputElement>(".settings-nav-search")?.focus();
      }
      const target = e.target as HTMLElement | null;
      const inField = !!target?.closest("input, textarea, select, [contenteditable], [data-recording]");
      if (e.key === "Escape" && !inField && !e.defaultPrevented) void api.hideSettings();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

function useAppearance() {
  const settings = useStore((s) => s.settings);
  const systemDark = useSystemDark();
  useEffect(() => {
    if (!settings) return;
    applyAppearance(settings, systemDark);
    syncRowHeight();
  }, [settings, systemDark]);
}

export function SettingsApp() {
  const [page, setPage] = useState(initialPage);
  useSettings();
  useOpenSettings(setPage);
  useWindowKeys();
  useAppearance();
  useEffect(trackWindowFocus, []);

  return (
    <>
      <SettingsWindow page={page} onPage={setPage} />
      <Toasts />
    </>
  );
}
