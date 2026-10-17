import { EMPTY_UI_STATE, api, type PaneSize, type UiState } from "../../api";
import { fail } from "../requests";
import { selectActiveSession } from "../selectors";
import type { Slice } from "../store";

export interface ViewStateSlice {
  /** Per-connection view state (column widths, pane sizes, sidebar, collapsed groups) by connection identity. */
  uiState: Record<string, UiState>;
  loadUiState: (identity: string) => Promise<void>;
  setTableWidths: (identity: string, table: string, widths: Record<string, number>) => void;
  setPaneSize: (identity: string, pane: PaneSize, px: number) => void;
  toggleSidebar: () => void;
  /** Collapses or expands an object view group (`<document table>::<group id>`) in one database and persists it. */
  toggleGroupCollapsed: (identity: string, database: string, key: string) => void;
}

export const viewStateSlice: Slice<ViewStateSlice> = {
  create: (set, get) => {
    const patchUiState = (identity: string, patch: (current: UiState) => Partial<UiState>) => {
      const current = get().uiState[identity] ?? EMPTY_UI_STATE;
      const ui: UiState = { ...current, ...patch(current) };
      set((s) => ({ uiState: { ...s.uiState, [identity]: ui } }));
      persistUiState(identity, ui);
    };

    return {
      uiState: {},

      async loadUiState(identity) {
        if (get().uiState[identity]) return;
        try {
          const ui = await api.getUiState(identity);
          set((s) => ({ uiState: { ...s.uiState, [identity]: ui } }));
        } catch (e) {
          fail(e, "Could not load view state");
        }
      },

      setTableWidths(identity, table, widths) {
        patchUiState(identity, (current) => ({ tables: { ...current.tables, [table]: { widths } } }));
      },

      setPaneSize(identity, pane, px) {
        patchUiState(identity, () => ({ [pane]: Math.round(px) }));
      },

      toggleSidebar() {
        const session = selectActiveSession(get());
        if (session) patchUiState(session.identity, (current) => ({ sidebarHidden: !current.sidebarHidden }));
      },

      toggleGroupCollapsed(identity, database, key) {
        patchUiState(identity, (current) => {
          const keys = current.collapsedGroups?.[database] ?? [];
          const next = keys.includes(key) ? keys.filter((k) => k !== key) : [...keys, key];
          return { collapsedGroups: { ...current.collapsedGroups, [database]: next } };
        });
      },
    };
  },
};

const uiWrites = new Map<string, { ui: UiState; timer: ReturnType<typeof setTimeout> }>();

/** Debounces view state writes per connection (drags fire constantly): a write lands 500 ms after the last change. */
function persistUiState(identity: string, ui: UiState) {
  clearTimeout(uiWrites.get(identity)?.timer);
  uiWrites.set(identity, { ui, timer: setTimeout(() => writeUiState(identity), 500) });
}

function writeUiState(identity: string) {
  const write = uiWrites.get(identity);
  if (!write) return;
  uiWrites.delete(identity);
  void api.setUiState(identity, write.ui).catch((e) => fail(e, "Could not save view state"));
}

/** Writes debounced view state now; called when the window unloads. */
export function flushUiState() {
  for (const [identity, { timer }] of [...uiWrites]) {
    clearTimeout(timer);
    writeUiState(identity);
  }
}
