import { useState } from "react";
import type { Settings } from "../../api";
import { hasSection, resolveSettingsPage, settingsGroups, useSettingsPagesVersion, type SettingsNavEntry } from "../../registry/settingsPages";
import { useStore } from "../../state/store";
import { SectionTarget } from "./controls";

const targetOf = (match: SettingsNavEntry) => match.section?.id ?? match.page.id;

/** The page list, narrowed by the search field; a page found by a section's title opens at that section. */
function SettingsNav({ current, onPage }: { current: string; onPage: (id: string) => void }) {
  const [query, setQuery] = useState("");
  const groups = settingsGroups(query);
  const first = groups[0]?.pages[0];
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
          if (e.key === "Enter" && first) onPage(targetOf(first));
        }}
      />
      <div className="settings-nav-list">
        {groups.length === 0 && <p className="settings-nav-empty">No Results</p>}
        {groups.map((g) => (
          <div key={g.group} className="settings-nav-group">
            <div className="settings-nav-heading">{g.group}</div>
            {g.pages.map((match) => {
              const {
                page: { id, label, icon: Icon },
                section,
              } = match;
              return (
                <button
                  key={id}
                  type="button"
                  aria-label={section ? `${label} ${section.title}` : undefined}
                  aria-current={current === id ? "page" : undefined}
                  onClick={() => onPage(targetOf(match))}
                >
                  <Icon size="1.077rem" /> {label}
                  {section && <span className="settings-nav-section"> {section.title}</span>}
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </nav>
  );
}

/** `page` is a page id or a section id; a section opens its page scrolled to it. */
export function SettingsWindow({ page: shown, onPage }: { page: string | undefined; onPage: (id: string) => void }) {
  useSettingsPagesVersion();
  const settings = useStore((s) => s.settings);
  const page = resolveSettingsPage(shown);
  if (!settings || !page) return null;
  const target = hasSection(page, shown) ? shown : undefined;
  const set = (patch: Partial<Settings>) => {
    const s = useStore.getState();
    if (s.settings) void s.saveSettings({ ...s.settings, ...patch }, { quiet: true });
  };
  const { Body } = page;

  return (
    <div className="settings-window">
      <SettingsNav current={page.id} onPage={onPage} />
      <div className="settings-pane">
        <header data-tauri-drag-region>
          <h2 data-tauri-drag-region>{page.label}</h2>
        </header>
        <div className="settings-body" key={page.id}>
          {page.intro && <p className="settings-intro">{page.intro}</p>}
          <SectionTarget.Provider value={target}>
            <Body s={settings} set={set} />
          </SectionTarget.Provider>
        </div>
      </div>
    </div>
  );
}
