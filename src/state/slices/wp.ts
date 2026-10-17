import { api, type WpInfo, type WpList } from "../../api";
import { onSessionEvent } from "../events";
import { beginRequest, fail, isLatest } from "../requests";
import { selectActiveSession } from "../selectors";
import { useStore, type Slice, type Store } from "../store";
import { remembered, resetMode } from "./ui";
import { withoutSession } from "./session";

export type { WpList };

export type WpRoute =
  | { screen: "dashboard" }
  | { screen: "list"; list: WpList; params?: Record<string, string>; selected?: { entity: string; id: string } }
  | { screen: "snapshots" }
  | { screen: "find"; value: string };

export interface WpSlice {
  /** WordPress detection per `session/database`; null = not WordPress. */
  wp: Record<string, WpInfo | null>;
  wpNav: { stack: WpRoute[]; index: number };
  loadWp(sessionId: string, database: string, refresh?: boolean): Promise<void>;
  navigate(route: WpRoute): void;
  /** Changes the current route without adding a history entry (selection, filters). */
  replaceRoute(route: WpRoute): void;
  goBack(): void;
  goForward(): void;
  /** Opens an entity record (`order 4711`) as an object tab; Rust picks the table that holds the key. */
  openEntity(entity: string, id: string): Promise<void>;
}

const HOME_NAV: WpSlice["wpNav"] = { stack: [{ screen: "dashboard" }], index: 0 };

export const resetWpNav = () => ({ wpNav: HOME_NAV });

/** Back to the dashboard; only a database known to be WordPress keeps the mode (loadWp switches a newly detected one). */
export function resetWpForDatabase(s: Store, sessionId: string, database: string): Partial<Store> {
  return { ...resetWpNav(), ...(s.wp[`${sessionId}/${database}`] ? {} : resetMode()) };
}

/** Route identity that does not depend on property order. */
function routeKey(route: WpRoute): string {
  const sorted = (value: unknown): unknown =>
    value && typeof value === "object"
      ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, sorted(v)]))
      : value;
  return JSON.stringify(sorted(route));
}

let legacyEntitiesReported = false;

/** `<config dir>/entities.toml` is no longer read; said once per run. */
function reportLegacyEntities(info: WpInfo | null) {
  if (!info?.legacyEntitiesFile || legacyEntitiesReported) return;
  legacyEntitiesReported = true;
  useStore.getState().toast("info", `${info.legacyEntitiesFile} is no longer read. User presets replace it; see docs/presets.md.`);
}

onSessionEvent((event) => {
  if (event.type === "databaseChanged") void useStore.getState().loadWp(event.sessionId, event.database);
});

export const wpSlice: Slice<WpSlice> = {
  create: (set, get) => ({
    wp: {},
    ...resetWpNav(),

    async loadWp(sessionId, database, refresh = false) {
      const key = `${sessionId}/${database}`;
      const token = beginRequest(`wp:${key}`);
      try {
        const info = await api.wpDetect(sessionId, database, refresh);
        if (!isLatest(`wp:${key}`, token)) return;
        reportLegacyEntities(info);
        set((s) => {
          if (!s.sessions.some((x) => x.id === sessionId)) return {};
          const active = s.activeSessionId === sessionId && selectActiveSession(s)?.database === database;
          return {
            wp: { ...s.wp, [key]: info },
            // WordPress databases open in WordPress mode until the user picks a mode.
            mode: info && active && !s.modeChosen ? "wp" : s.mode,
          };
        });
      } catch (e) {
        if (isLatest(`wp:${key}`, token)) fail(e, "WordPress detection failed");
      }
    },

    navigate(route) {
      set((s) => {
        const current = s.wpNav.stack[s.wpNav.index];
        if (current && routeKey(current) === routeKey(route)) return { mode: "wp" };
        const stack = [...s.wpNav.stack.slice(0, s.wpNav.index + 1), route].slice(-100);
        return { mode: "wp", wpNav: { stack, index: stack.length - 1 } };
      });
    },

    replaceRoute(route) {
      set((s) => {
        const stack = [...s.wpNav.stack];
        stack[s.wpNav.index] = route;
        return { wpNav: { ...s.wpNav, stack } };
      });
    },

    goBack() {
      set((s) => ({ wpNav: { ...s.wpNav, index: Math.max(0, s.wpNav.index - 1) } }));
    },

    goForward() {
      set((s) => ({ wpNav: { ...s.wpNav, index: Math.min(s.wpNav.stack.length - 1, s.wpNav.index + 1) } }));
    },

    async openEntity(entity, id) {
      const session = selectActiveSession(get());
      if (!session?.database) return;
      try {
        const document = await api.document(session.id, `${entity} ${id}`, session.database);
        get().keepDocument(session.id, document);
        get().openObject(document.address);
        set((s) => remembered(s, { kind: "entity", entity, id, title: `${entity} #${id}` }));
      } catch (e) {
        fail(e, `Could not open ${entity} #${id}`);
      }
    },
  }),

  sessionClosed(sessionId, s) {
    const wp = withoutSession(s.wp, sessionId);
    // The WordPress history describes the active session only.
    if (s.activeSessionId !== sessionId) return { wp };
    return { wp, ...resetWpNav() };
  },
};
