import { EMPTY_UI_STATE, api, type AppError, type ColumnInfo, type ConnectionSpec, type PaneSize, type UiState } from "../../api";
import { fail } from "../requests";
import { selectActiveSession } from "../selectors";
import type { Slice, Store } from "../store";

export type Mode = "wp" | "db";

export interface Toast {
  id: number;
  kind: "error" | "info" | "success";
  message: string;
  /** How many times the same message was reported while it was showing. */
  count: number;
}

export type Dialog =
  /** `page`: the Settings page shown (ids from `settingsPages()` in `registry/settingsPages.ts`); the first one when absent. */
  | { type: "settings"; page?: string }
  | { type: "connect"; spec?: ConnectionSpec; error?: AppError }
  | { type: "password"; spec: ConnectionSpec; account: string | null; error?: string; savePassword: boolean }
  | { type: "confirm"; title: string; message: string; confirmLabel: string; onConfirm: () => void }
  | { type: "prompt"; title: string; value: string; onSubmit: (value: string) => void }
  | { type: "insertRow"; database: string; table: string; columns: ColumnInfo[] };

export type RecentNav =
  | { kind: "entity"; entity: string; id: string; title: string }
  | { kind: "table"; database: string; table: string };

/**
 * What is under the pointer: a value (its provenance text and table), or a table link such as a sidebar
 * row or a section band (`linked`), which lights every element of that table (docs/ux.md).
 */
export interface ValueHover {
  src: string | null;
  table: string | null;
  linked: boolean;
}

export const NO_HOVER: ValueHover = { src: null, table: null, linked: false };

export interface UiSlice {
  toasts: Toast[];
  dialog: Dialog | null;
  sidebarPanel: "tables" | "history";
  paletteOpen: boolean;
  /** Text the palette opens with, e.g. `t:` for tables. */
  paletteSeed: string;
  /** Per-connection UI state (column widths, split width) by connection identity. */
  uiState: Record<string, UiState>;
  /** Recently opened entities and tables, newest first. */
  recentNav: RecentNav[];
  mode: Mode;
  /** Shown in the status bar instead of the address while set. */
  hover: ValueHover;
  /** The user picked a mode explicitly; don't switch automatically anymore. */
  modeChosen: boolean;
  toast(kind: Toast["kind"], message: string): void;
  dismissToast(id: number): void;
  openDialog(dialog: Dialog): void;
  closeDialog(): void;
  setSidebarPanel(panel: UiSlice["sidebarPanel"]): void;
  setPaletteOpen(open: boolean, seed?: string): void;
  loadUiState(identity: string): Promise<void>;
  setTableWidths(identity: string, table: string, widths: Record<string, number>): void;
  /** Persists a dragged pane size for this connection. */
  setPaneSize(identity: string, pane: PaneSize, px: number): void;
  /** Shows or hides the sidebar of the active connection and persists the choice. */
  toggleSidebar(): void;
  setMode(mode: Mode): void;
  setHover(hover: ValueHover): void;
}

let nextToastId = 1;

/** Back to the Database view; a WordPress database switches again until the user picks a mode. */
export const resetMode = () => ({ mode: "db" as Mode, modeChosen: false });

const recentKey = (r: RecentNav) => JSON.stringify(r.kind === "entity" ? [r.kind, r.entity, r.id] : [r.kind, r.database, r.table]);

/** Keeps the last 20 opened entities and tables for the palette's Recent section. */
export function remembered(s: Store, item: RecentNav): Partial<Store> {
  const rest = s.recentNav.filter((r) => recentKey(r) !== recentKey(item));
  return { recentNav: [item, ...rest].slice(0, 20) };
}

export const uiSlice: Slice<UiSlice> = {
  create: (set, get) => {
    /** Applies `patch` to one connection's UI state, then saves it debounced. */
    const patchUiState = (identity: string, patch: (current: UiState) => Partial<UiState>) => {
      const current = get().uiState[identity] ?? EMPTY_UI_STATE;
      const ui: UiState = { ...current, ...patch(current) };
      set((s) => ({ uiState: { ...s.uiState, [identity]: ui } }));
      persistUiState(identity, ui);
    };

    return {
      toasts: [],
      dialog: null,
      sidebarPanel: "tables",
      paletteOpen: false,
      paletteSeed: "",
      uiState: {},
      recentNav: [],
      hover: NO_HOVER,
      ...resetMode(),

      toast(kind, message) {
        if (kind === "error") void api.logError(message).catch(() => {});
        const same = get().toasts.find((t) => t.kind === kind && t.message === message);
        if (same) {
          set((s) => ({ toasts: s.toasts.map((t) => (t.id === same.id ? { ...t, count: t.count + 1 } : t)) }));
          return;
        }
        const id = nextToastId++;
        set((s) => ({ toasts: [...s.toasts.slice(-4), { id, kind, message, count: 1 }] }));
      },

      dismissToast(id) {
        set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
      },

      openDialog(dialog) {
        set({ dialog });
      },

      closeDialog() {
        set({ dialog: null });
      },

      setSidebarPanel(sidebarPanel) {
        set({ sidebarPanel });
      },

      setPaletteOpen(paletteOpen, seed = "") {
        set({ paletteOpen, paletteSeed: seed });
      },

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

      setMode(mode) {
        set({ mode, modeChosen: true });
      },

      setHover(hover) {
        const current = get().hover;
        if (current.src !== hover.src || current.table !== hover.table || current.linked !== hover.linked) set({ hover });
      },
    };
  },
};

const uiWrites = new Map<string, { ui: UiState; timer: ReturnType<typeof setTimeout> }>();

/** Debounces UI state writes per connection (drags fire constantly): a write lands 500 ms after the last change. */
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

/** Writes debounced UI state now; called when the window unloads. */
export function flushUiState() {
  for (const [identity, { timer }] of [...uiWrites]) {
    clearTimeout(timer);
    writeUiState(identity);
  }
}
