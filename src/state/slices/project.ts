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
  /** Stores one result (`null` drops it, e.g. a run cancelled before it answered). */
  setCoverage: (key: string, reference: string, state: CoverageState | null) => void;
  /** Forgets every result of the database, so the Project page counts again. */
  refreshCoverage: (key: string) => void;
  loadProjectRules: (sessionId: string, database: string) => Promise<void>;
  loadRulesInfo: (sessionId: string, database: string) => Promise<void>;
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
