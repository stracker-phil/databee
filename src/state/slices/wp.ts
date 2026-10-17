import { api, type WpInfo, type WpList } from "../../api";
import { onSessionEvent } from "../events";
import { beginRequest, fail, isLatest } from "../requests";
import { selectActiveSession } from "../selectors";
import { useStore, type Slice, type Store } from "../store";
import { remembered, resetMode, retitledRecent } from "./ui";
import { withoutSession } from "./session";

export type { WpList };

export type WpRoute =
  | { screen: "dashboard" }
  | { screen: "list"; list: WpList; params?: Record<string, string>; selected?: { entity: string; id: string } }
  | { screen: "entity"; entity: string; id: string }
  | { screen: "compare"; entity: string; a: string; b: string }
  | { screen: "snapshots" }
  | { screen: "find"; value: string };

export interface EntityMeta {
  key: string;
  /** Which list the entity belongs to, e.g. products for a product post. */
  parent?: WpList;
  title?: string;
  table: string;
  idColumn: string;
  id: string;
}

export interface WpSlice {
  /** WordPress detection per `session/database`; null = not WordPress. */
  wp: Record<string, WpInfo | null>;
  wpNav: { stack: WpRoute[]; index: number };
  /** Facts about the loaded entity page for breadcrumbs and the source-row command. */
  entityMeta: EntityMeta | null;
  setEntityMeta(meta: EntityMeta | null): void;
  loadWp(sessionId: string, database: string, refresh?: boolean): Promise<void>;
  navigate(route: WpRoute): void;
  /** Changes the current route without adding a history entry (selection, filters). */
  replaceRoute(route: WpRoute): void;
  goBack(): void;
  goForward(): void;
  openEntity(entity: string, id: string): void;
  takeSnapshot(entity: string, id: string): Promise<void>;
}

const HOME_NAV: WpSlice["wpNav"] = { stack: [{ screen: "dashboard" }], index: 0 };

export const resetWpNav = () => ({ wpNav: HOME_NAV });

/** Leaves the entity page; only a database known to be WordPress keeps the mode (loadWp switches a newly detected one). */
export function resetWpForDatabase(s: Store, sessionId: string, database: string): Partial<Store> {
  return { ...resetWpNav(), entityMeta: null, ...(s.wp[`${sessionId}/${database}`] ? {} : resetMode()) };
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
    entityMeta: null,
    ...resetWpNav(),

    setEntityMeta(entityMeta) {
      set({ entityMeta });
      const title = entityMeta?.title;
      if (title) set((s) => retitledRecent(s, entityMeta.key, title));
    },

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

    openEntity(entity, id) {
      get().navigate({ screen: "entity", entity, id });
      const meta = get().entityMeta;
      const title = meta?.key === `${entity}:${id}` && meta.title ? meta.title : `${entity} #${id}`;
      set((s) => remembered(s, { kind: "entity", entity, id, title }));
    },

    async takeSnapshot(entity, id) {
      const session = selectActiveSession(get());
      if (!session?.database) return;
      try {
        const info = await api.wpSnapshotTake(
          session.id,
          session.database,
          { entity, id, includeRelated: true, optionPattern: null, includeActions: true },
          `${entity} #${id}`,
        );
        get().toast("success", `Snapshot saved (${info.paths} values). Diff it under Snapshots.`);
      } catch (e) {
        fail(e, "Snapshot failed");
      }
    },
  }),

  sessionClosed(sessionId, s) {
    const wp = withoutSession(s.wp, sessionId);
    // The entity page and WordPress history describe the active session only.
    if (s.activeSessionId !== sessionId) return { wp };
    return { wp, ...resetWpNav(), entityMeta: null };
  },
};
