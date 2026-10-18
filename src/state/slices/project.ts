import { api, toAppError, type AppError, type Coverage, type ProjectChange, type ProjectRules, type RulesInfo, type SessionInfo } from "../../api";
import type { ProjectForm } from "../../lib/ruleForms";
import { fail } from "../requests";
import { selectActiveSession } from "../selectors";
import type { Slice, Store } from "../store";
import { EMPTY_NAV } from "./nav";
import { withoutSession } from "./session";
import { activate, findTab, newTabId, replaceTab, type ProjectSection, type ProjectTab } from "./tabs";

interface ProjectRulesEntry {
  version: number;
  rules: ProjectRules | null;
  error: AppError | null;
}

export type CoverageState = { status: "running" } | { status: "done"; coverage: Coverage } | { status: "error"; message: string };

/** Reference coverage of one database for the session, by `coverageKey`; `generation` counts refreshes. */
export interface CoverageCache {
  generation: number;
  results: Record<string, CoverageState>;
}

export interface ProjectSlice {
  /** By `session/database`. */
  projectRules: Record<string, ProjectRulesEntry>;
  /** By `session/database`, for the status bar's preset text. */
  rulesInfo: Record<string, RulesInfo>;
  /** By `session/database`. */
  coverage: Record<string, CoverageCache>;
  /** Each database's own accent by `accentKey` (connection slug, not session: every session of it shares it). */
  accents: Record<string, string>;
  loadAccents: () => Promise<void>;
  /** Stores the database's accent (`null`: the app's) and applies it at once; nothing reloads, since no rule changed. */
  setAccent: (sessionId: string, database: string, accent: string | null) => Promise<void>;
  /** Stores one result (`null` drops it, e.g. a run cancelled before it answered). */
  setCoverage: (key: string, reference: string, state: CoverageState | null) => void;
  /** Forgets every result of the database, so the Project page counts again. */
  refreshCoverage: (key: string) => void;
  loadProjectRules: (sessionId: string, database: string) => Promise<void>;
  loadRulesInfo: (sessionId: string, database: string) => Promise<void>;
  /** Loads the database's `rulesInfo` unless it is loaded or loading (table icons ask for it from many places). */
  ensureRulesInfo: (sessionId: string, database: string) => void;
  /**
   * Opens the Project page of `database` (default: the active session's current one); an open one is reused.
   * A `form` replaces the page's form.
   */
  openProject: (opts?: { sessionId?: string; database?: string; form?: ProjectForm }) => void;
  setProjectForm: (tabId: string, form: ProjectForm | null) => void;
  setProjectSection: (tabId: string, section: ProjectSection) => void;
  /** Stores the change; every view reloads, since rules change what values are. The error is returned, not shown. */
  changeProject: (sessionId: string, database: string, change: ProjectChange) => Promise<AppError | null>;
}

export const projectKey = (sessionId: string, database: string) => `${sessionId}/${database}`;

/** Rust's project key: the accents are stored per connection slug and database. */
export const accentKey = (slug: string, database: string) => `${slug}/${database}`;

/** The database's own accent; `undefined` when it uses the app's. */
export const selectDatabaseAccent = (s: Store, slug: string, database: string | null | undefined): string | undefined =>
  database ? s.accents[accentKey(slug, database)] : undefined;

/** The accent in effect: the active session's current database's own, else the app's (`null` before settings load). */
export function selectEffectiveAccent(s: Store): string | null {
  const session = selectActiveSession(s);
  return (session && selectDatabaseAccent(s, session.slug, session.database)) || (s.settings?.accent ?? null);
}

const loadingRulesInfo = new Set<string>();

/** The session and database a Project page is for: the given ones, else the active session's current database. */
function projectTarget(s: Store, opts: { sessionId?: string; database?: string }): { session: SessionInfo; database: string } | null {
  const session = opts.sessionId ? s.sessions.find((x) => x.id === opts.sessionId) : selectActiveSession(s);
  const database = opts.database ?? session?.database;
  return session && database ? { session, database } : null;
}

export const projectSlice: Slice<ProjectSlice> = {
  create: (set, get) => ({
    projectRules: {},
    rulesInfo: {},
    coverage: {},
    accents: {},

    setCoverage(key, reference, state) {
      set((s) => {
        const cache = s.coverage[key] ?? { generation: 0, results: {} };
        const results = { ...cache.results };
        if (state) results[reference] = state;
        else delete results[reference];
        return { coverage: { ...s.coverage, [key]: { ...cache, results } } };
      });
    },

    refreshCoverage(key) {
      set((s) => ({ coverage: { ...s.coverage, [key]: { generation: (s.coverage[key]?.generation ?? 0) + 1, results: {} } } }));
    },

    async loadAccents() {
      try {
        set({ accents: await api.projectAccents() });
      } catch (e) {
        fail(e, "Could not load the database accents");
      }
    },

    async setAccent(sessionId, database, accent) {
      const session = get().sessions.find((x) => x.id === sessionId);
      if (!session) return;
      try {
        const stored = await api.projectSetAccent(sessionId, database, accent);
        set((s) => {
          const accents = { ...s.accents };
          const key = accentKey(session.slug, database);
          if (stored) accents[key] = stored;
          else delete accents[key];
          return { accents };
        });
      } catch (e) {
        fail(e, "Could not set the accent");
      }
    },

    async loadProjectRules(sessionId, database) {
      const key = projectKey(sessionId, database);
      const version = get().dataVersion;
      const cached: ProjectRulesEntry | undefined = get().projectRules[key];
      if (cached?.version === version) return;
      const write = (entry: Omit<ProjectRulesEntry, "version">) => set((s) => ({ projectRules: { ...s.projectRules, [key]: { ...entry, version } } }));
      const current = () => get().projectRules[key]?.version === version;
      write({ rules: cached?.rules ?? null, error: null });
      try {
        const rules = await api.projectRules(sessionId, database);
        if (current()) write({ rules, error: null });
      } catch (e) {
        if (current()) write({ rules: null, error: toAppError(e) });
      }
    },

    async loadRulesInfo(sessionId, database) {
      try {
        const info = await api.rulesInfo(sessionId, database);
        set((s) => ({ rulesInfo: { ...s.rulesInfo, [projectKey(sessionId, database)]: info } }));
      } catch (e) {
        fail(e, "Could not load the presets");
      }
    },

    ensureRulesInfo(sessionId, database) {
      const key = projectKey(sessionId, database);
      if (key in get().rulesInfo || loadingRulesInfo.has(key)) return;
      loadingRulesInfo.add(key);
      void get()
        .loadRulesInfo(sessionId, database)
        .finally(() => loadingRulesInfo.delete(key));
    },

    openProject(opts = {}) {
      const s = get();
      const target = projectTarget(s, opts);
      if (!target) return s.toast("info", "Choose a database first: the Project page belongs to one");
      const { session, database } = target;
      if (s.activeSessionId !== session.id) s.setActiveSession(session.id);
      const fresh = get();
      const existing = fresh.tabs.find((t): t is ProjectTab => t.kind === "project" && t.sessionId === session.id && t.database === database);
      if (existing) {
        const form = opts.form;
        set((st) => ({ ...(form ? replaceTab(st, { ...existing, form }) : {}), ...activate(st, existing.id) }));
        return;
      }
      set((st) => {
        const tab: ProjectTab = {
          id: newTabId("project"),
          kind: "project",
          sessionId: session.id,
          title: "project",
          database,
          address: { connection: session.slug, database, target: { kind: "project" }, view: null, compare: null },
          form: opts.form ?? null,
          nav: EMPTY_NAV,
        };
        return { tabs: [...st.tabs, tab], ...activate(st, tab.id) };
      });
    },

    setProjectForm(tabId, form) {
      set((s) => {
        const tab = findTab(s, tabId, "project");
        return tab ? replaceTab(s, { ...tab, form }) : {};
      });
    },

    setProjectSection(tabId, section) {
      set((s) => {
        const tab = findTab(s, tabId, "project");
        return tab ? replaceTab(s, { ...tab, section }) : {};
      });
    },

    async changeProject(sessionId, database, change) {
      try {
        await api.projectUpdate(sessionId, database, change);
      } catch (e) {
        return toAppError(e);
      }
      get().bumpData();
      void get().loadRulesInfo(sessionId, database);
      return null;
    },
  }),

  sessionClosed: (sessionId, s) => ({ projectRules: withoutSession(s.projectRules, sessionId), rulesInfo: withoutSession(s.rulesInfo, sessionId), coverage: withoutSession(s.coverage, sessionId) }),
};
