import { api, type AppError, type ColumnInfo, type ConnectionSpec, type Home, type SnapshotInfo } from "../../api";
import type { Slice, Store } from "../store";

export interface Toast {
  id: number;
  kind: "error" | "info" | "success";
  message: string;
  /** How many times the same message was reported while it was showing. */
  count: number;
}

type Dialog =
  /** `page`: the Settings page shown (ids from `settingsPages()` in `registry/settingsPages.ts`); the first one when absent. */
  | { type: "settings"; page?: string }
  | { type: "connect"; spec?: ConnectionSpec; error?: AppError }
  | { type: "password"; spec: ConnectionSpec; account: string | null; error?: string; savePassword: boolean }
  | { type: "confirm"; title: string; message: string; confirmLabel: string; onConfirm: () => void }
  | { type: "prompt"; title: string; value: string; onSubmit: (value: string) => void }
  | { type: "insertRow"; database: string; table: string; columns: ColumnInfo[] }
  /** How a saved query shows on the readout; `null` is not pinned. */
  | { type: "pin"; title: string; pin: Home | null; onSubmit: (pin: Home | null) => void }
  /** The shortcut overview of the scopes that were active when it opened. */
  | { type: "shortcuts"; scopes: string[] }
  /** The snapshots of a row or saved query, newest first; picking one marks what changed since. */
  | { type: "snapshots"; title: string; snapshots: SnapshotInfo[]; onPick: (id: string) => void };

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
  recentNav: RecentNav[];
  /** Shown in the status bar instead of the address while set. */
  hover: ValueHover;
  toast: (kind: Toast["kind"], message: string) => void;
  dismissToast: (id: number) => void;
  openDialog: (dialog: Dialog) => void;
  closeDialog: () => void;
  setSidebarPanel: (panel: UiSlice["sidebarPanel"]) => void;
  setPaletteOpen: (open: boolean, seed?: string) => void;
  setHover: (hover: ValueHover) => void;
}

let nextToastId = 1;

const recentKey = (r: RecentNav) => JSON.stringify(r.kind === "entity" ? [r.kind, r.entity, r.id] : [r.kind, r.database, r.table]);

/** Keeps the last 20 opened entities and tables for the palette's Recent section. */
export function remembered(s: Store, item: RecentNav): Partial<Store> {
  const rest = s.recentNav.filter((r) => recentKey(r) !== recentKey(item));
  return { recentNav: [item, ...rest].slice(0, 20) };
}

export const uiSlice: Slice<UiSlice> = {
  create: (set, get) => ({
    toasts: [],
    dialog: null,
    sidebarPanel: "tables",
    paletteOpen: false,
    paletteSeed: "",
    recentNav: [],
    hover: NO_HOVER,

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

    setHover(hover) {
      const current = get().hover;
      if (current.src !== hover.src || current.table !== hover.table || current.linked !== hover.linked) set({ hover });
    },
  }),
};
