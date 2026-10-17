import { setOverrides } from "../../commands/registry";
import { api, type Keybindings, type Settings } from "../../api";
import { fail } from "../requests";
import type { Slice } from "../store";

export interface SettingsSlice {
  settings: Settings | null;
  keybindings: Keybindings;
  loadSettings: () => Promise<void>;
  /** `quiet` skips the toast, for settings that apply as they change. */
  saveSettings: (settings: Settings, opts?: { quiet?: boolean }) => Promise<void>;
  saveKeybindings: (bindings: Keybindings) => Promise<void>;
}

export const settingsSlice: Slice<SettingsSlice> = {
  create: (set, get) => ({
    settings: null,
    keybindings: {},

    async loadSettings() {
      try {
        const [settings, keybindings] = await Promise.all([api.getSettings(), api.getKeybindings().catch(() => ({}))]);
        set({ settings, keybindings });
      } catch (e) {
        fail(e, "Could not load settings");
      }
    },

    async saveSettings(settings, opts) {
      try {
        set({ settings: await api.updateSettings(settings) });
        if (!opts?.quiet) get().toast("success", "Settings saved");
      } catch (e) {
        fail(e, "Could not save settings");
      }
    },

    async saveKeybindings(bindings) {
      try {
        const saved = await api.setKeybindings(bindings);
        set({ keybindings: saved });
        setOverrides(saved);
      } catch (e) {
        fail(e, "Could not save shortcuts");
      }
    },
  }),
};
