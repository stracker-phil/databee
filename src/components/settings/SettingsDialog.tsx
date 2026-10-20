import { X } from "lucide-react";
import type { Settings } from "../../api";
import { resolveSettingsPage, settingsGroups, useSettingsPagesVersion } from "../../registry/settingsPages";
import { useStore } from "../../state/store";
import { Modal } from "../Modal";

/** macOS-style settings: sidebar of registered pages, grouped cards, every change applies immediately. */
export function SettingsDialog() {
  useSettingsPagesVersion();
  const settings = useStore((s) => s.settings);
  const save = useStore((s) => s.saveSettings);
  const close = useStore((s) => s.closeDialog);
  const shown = useStore((s) => (s.dialog?.type === "settings" ? s.dialog.page : undefined));
  const page = resolveSettingsPage(shown);
  if (!settings || !page) return null;

  const setPage = (id: string) => useStore.getState().openDialog({ type: "settings", page: id });
  const set = (patch: Partial<Settings>) => {
    const current = useStore.getState().settings;
    if (current) void save({ ...current, ...patch }, { quiet: true });
  };
  const { Body } = page;

  return (
    <Modal onClose={close} variant="panel" className="settings-window" labelledBy="settings-title" stageWindow="settings" accent={settings.accent}>
      <nav className="settings-nav" aria-label="Settings pages" data-stage="settings-nav" data-stage-label="Settings pages">
        {settingsGroups().map((g) => (
          <div key={g.group} className="settings-nav-group">
            <div className="settings-nav-heading">{g.group}</div>
            {g.pages.map(({ id, label, icon: Icon }) => (
              <button key={id} data-stage={`settings-nav:${id}`} data-stage-list="settings-nav" type="button" aria-current={page.id === id ? "page" : undefined} onClick={() => setPage(id)}>
                <Icon size="1.077rem" /> {label}
              </button>
            ))}
          </div>
        ))}
      </nav>
      <div className="settings-pane" data-stage={`settings:${page.id}`} data-stage-label={`Settings: ${page.label}`}>
        <header>
          <h2 id="settings-title">{page.label}</h2>
          <button data-stage="settings:close" type="button" className="icon-btn" aria-label="Close settings" title="Close (Esc)" onClick={close}>
            <X size="1.077rem" />
          </button>
        </header>
        <div className="settings-body">
          {page.intro && <p className="settings-intro">{page.intro}</p>}
          <Body s={settings} set={set} />
        </div>
      </div>
    </Modal>
  );
}
