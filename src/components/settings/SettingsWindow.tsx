import { useMemo, useState } from "react";
import type { Settings } from "../../api";
import { findRow, resolveSettingsPage, searchSettings, settingsGroups, useSettingsPagesVersion, type SettingsMatch } from "../../registry/settingsPages";
import { useStore } from "../../state/store";
import { ScrollTarget, type RevealTarget } from "./controls";

/** What Settings is asked to show: a page or row id. A new object per request, so asking again reveals again. */
export interface SettingsRequest {
  id?: string;
}

const targetOf = (match: SettingsMatch) => match.row?.id ?? match.page.id;

/** A page in the list; a search hit on a row shows the page, and the row's title below it. */
function NavButton({ page, row, current, onOpen }: SettingsMatch & { current: string; onOpen: (id: string) => void }) {
  const { icon: Icon } = page;
  return (
    <button
      type="button"
      aria-label={row ? `${row.title}, ${page.label}` : undefined}
      aria-current={!row && current === page.id ? "page" : undefined}
      onClick={() => onOpen(row?.id ?? page.id)}
    >
      <Icon size="1.077rem" aria-hidden="true" />
      <span className="settings-nav-text">
        <span className="settings-nav-title">{page.label}</span>
        {row && <span className="settings-nav-setting">{row.title}</span>}
      </span>
    </button>
  );
}

/** The page list under its group headings; a search replaces it with every matching page and row, best first. */
function SettingsNav({ current, onOpen }: { current: string; onOpen: (id: string) => void }) {
  const [query, setQuery] = useState("");
  const searching = query.trim() !== "";
  const results = searching ? searchSettings(query) : [];
  return (
    <nav className="settings-nav" aria-label="Settings pages" data-stage="settings-nav" data-stage-label="Settings pages">
      <div className="settings-titlebar" data-tauri-drag-region />
      <input
        className="input settings-nav-search"
        type="search"
        aria-label="Search Settings"
        placeholder="Search"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && results[0]) onOpen(targetOf(results[0]));
          // A first Escape clears the search; with nothing to clear it closes Settings.
          if (e.key === "Escape" && query) {
            e.preventDefault();
            e.stopPropagation();
            setQuery("");
          }
        }}
      />
      <div className="settings-nav-list scroller">
        {searching && results.length === 0 && <p className="settings-nav-empty">No Results</p>}
        {results.length > 0 && (
          <div className="settings-nav-group" role="group" aria-label="Results">
            {results.map((match) => (
              <NavButton key={targetOf(match)} {...match} current={current} onOpen={onOpen} />
            ))}
          </div>
        )}
        {!searching &&
          settingsGroups().map(({ group, pages }) => (
            <div key={group} className="settings-nav-group" role="group" aria-label={group}>
              <div className="settings-nav-heading" aria-hidden="true">{group}</div>
              {pages.map((page) => (
                <NavButton key={page.id} page={page} current={current} onOpen={onOpen} />
              ))}
            </div>
          ))}
      </div>
    </nav>
  );
}

/** `request.id` may name a row: its page opens scrolled to it, washed in the accent once. */
export function SettingsWindow({ request, onOpen }: { request: SettingsRequest; onOpen: (id: string) => void }) {
  useSettingsPagesVersion();
  const settings = useStore((s) => s.settings);
  const page = resolveSettingsPage(request.id);
  const target = useMemo((): RevealTarget | undefined => {
    const row = page && findRow(page, request.id);
    return row ? { id: row.id, under: row.under } : undefined;
  }, [request, page]);
  if (!settings || !page) return null;
  const set = (patch: Partial<Settings>) => {
    const s = useStore.getState();
    if (s.settings) void s.saveSettings({ ...s.settings, ...patch }, { quiet: true });
  };
  const { Body } = page;

  return (
    <div className="settings-window">
      <SettingsNav current={page.id} onOpen={onOpen} />
      <div className="settings-pane">
        <header data-tauri-drag-region>
          <h2 data-tauri-drag-region>{page.label}</h2>
        </header>
        <div className="settings-body scroller" key={page.id}>
          {page.intro && <p className="settings-intro">{page.intro}</p>}
          <ScrollTarget.Provider value={target}>
            <Body s={settings} set={set} />
          </ScrollTarget.Provider>
        </div>
      </div>
    </div>
  );
}
