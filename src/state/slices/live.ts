import { api, type DocDelta, type SnapshotInfo } from "../../api";
import { formatAddress } from "../../lib/address";
import { objectColumnAt } from "../../lib/columns";
import { clockTime, hasMarks, noMarks, withDelta, type ChangeMarks, type MarksSince } from "../../lib/diff";
import { documentText } from "../../lib/document";
import { fail } from "../requests";
import { selectActiveTab } from "../selectors";
import type { Slice, Store } from "../store";
import { documentKey } from "./objects";
import { withoutSession } from "./session";
import type { Tab, TabOfKind } from "./tabs";

/** A live object column; `id` is the Rust watch, null while it starts. */
export interface LiveWatch {
  sessionId: string;
  /** The row address (`documentText`). */
  address: string;
  id: string | null;
}

/** What the live and snapshot commands act on: the active object column, or a query tab of a saved query. */
export interface ChangeTarget {
  /** The `changeMarks` key: the document key of an object column, `<session>/q:<tab id>` for a query tab. */
  key: string;
  kind: "object" | "query";
  sessionId: string;
  database: string;
  /** A row address, or the saved query's (`…/q/<id>`). */
  address: string;
  title: string;
}

export interface LiveSlice {
  /** Live object columns by document key. */
  live: Record<string, LiveWatch>;
  /** Change marks by `ChangeTarget.key`; `x` drops them. */
  changeMarks: Record<string, ChangeMarks>;
  /** `w`: live mode on or off for the active object column. */
  toggleLive: () => Promise<void>;
  /** One round: every live column polls once; columns no tab shows any more stop. */
  pollLive: () => Promise<void>;
  /** `x`: drops the change marks of the active target; live mode stays on. */
  dismissMarks: () => void;
  /** `s`: asks for a label, then stores the active target as it is now. */
  takeSnapshot: () => void;
  /** "Changes since snapshot…": lists the active target's snapshots to pick one. */
  chooseSnapshot: () => Promise<void>;
  showSnapshotDiff: (target: ChangeTarget, id: string) => Promise<void>;
}

export const queryMarksKey = (sessionId: string, tabId: string) => `${sessionId}/q:${tabId}`;

let timer: ReturnType<typeof setTimeout> | null = null;
const polling = new Set<string>();

function stopTimer() {
  if (timer) clearTimeout(timer);
  timer = null;
}

const CHANGE_TARGETS: { [K in Tab["kind"]]?: (s: Store, tab: TabOfKind<K>) => ChangeTarget | null } = {
  object(_, tab) {
    const column = objectColumnAt(tab.columns, tab.active);
    if (!column || column.address.target.kind !== "row") return null;
    const address = documentText(column.address);
    const { table, key } = column.address.target;
    return { key: documentKey(tab.sessionId, address), kind: "object", sessionId: tab.sessionId, database: column.address.database, address, title: `${table} #${key}` };
  },
  query(s, tab) {
    const connection = s.sessions.find((x) => x.id === tab.sessionId)?.slug;
    if (!tab.saved || !connection) return null;
    const { database, id } = tab.saved;
    const address = formatAddress({ connection, database, target: { kind: "query", id }, view: null, compare: null });
    return { key: queryMarksKey(tab.sessionId, tab.id), kind: "query", sessionId: tab.sessionId, database, address, title: tab.title };
  },
};

export function selectChangeTarget(s: Store): ChangeTarget | null {
  const tab = selectActiveTab(s);
  const target = tab && (CHANGE_TARGETS[tab.kind] as ((s: Store, tab: Tab) => ChangeTarget | null) | undefined);
  return tab && target ? target(s, tab) : null;
}

export function selectActiveLive(s: Store): number | null {
  const target = selectChangeTarget(s);
  return target && s.live[target.key] ? (s.settings?.liveIntervalSecs ?? 2) : null;
}

export const selectActiveMarks = (s: Store): ChangeMarks | undefined => {
  const target = selectChangeTarget(s);
  return target ? s.changeMarks[target.key] : undefined;
};

export const selectCanDismissMarks = (s: Store) => hasMarks(selectActiveMarks(s));

function shown(s: Store, key: string): boolean {
  return s.tabs.some((tab) => tab.kind === "object" && tab.columns.some((c) => c.kind === "object" && documentKey(tab.sessionId, documentText(c.address)) === key));
}

const isEmptyDelta = (delta: DocDelta) => delta.changed.length + delta.added.length + delta.removed.length === 0;

const without = <T>(record: Record<string, T>, key: string): Record<string, T> => Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));

export const liveSlice: Slice<LiveSlice> = {
  create: (set, get) => {
    stopTimer();
    polling.clear();

    const schedule = () => {
      if (timer || Object.keys(get().live).length === 0) return;
      const seconds = get().settings?.liveIntervalSecs ?? 2;
      timer = setTimeout(() => {
        timer = null;
        void get()
          .pollLive()
          .finally(schedule);
      }, seconds * 1000);
    };

    const stop = (key: string) => {
      const watch = get().live[key];
      if (!watch) return;
      set((s) => ({ live: without(s.live, key) }));
      if (watch.id) void api.watchStop(watch.sessionId, watch.id).catch(() => {});
    };

    const mark = (key: string, since: MarksSince, delta: DocDelta, fresh: boolean) => {
      if (!fresh && isEmptyDelta(delta)) return;
      set((s) => {
        const current = s.changeMarks[key];
        const base = fresh || !current ? noMarks(since) : current;
        return { changeMarks: { ...s.changeMarks, [key]: withDelta(base, delta, Date.now()) } };
      });
    };

    const poll = async (key: string, watch: LiveWatch) => {
      if (!watch.id || polling.has(key)) return;
      polling.add(key);
      try {
        const { delta, document } = await api.watchPoll(watch.sessionId, watch.id);
        if (get().live[key]?.id !== watch.id) return;
        if (document) get().keepDocument(watch.sessionId, document);
        mark(key, { kind: "live" }, delta, false);
      } catch (e) {
        if (get().live[key]?.id !== watch.id) return;
        set((s) => ({ live: without(s.live, key) }));
        fail(e, "Live mode stopped");
      } finally {
        polling.delete(key);
      }
    };

    return {
      live: {},
      changeMarks: {},

      async toggleLive() {
        const target = selectChangeTarget(get());
        if (target?.kind !== "object") return;
        const { key, sessionId, address } = target;
        if (get().live[key]) return stop(key);
        set((s) => {
          const marks = s.changeMarks[key];
          const keep = marks?.since.kind === "live";
          return { live: { ...s.live, [key]: { sessionId, address, id: null } }, changeMarks: keep ? s.changeMarks : { ...s.changeMarks, [key]: noMarks({ kind: "live" }) } };
        });
        try {
          const id = await api.watchStart(sessionId, address);
          if (!get().live[key]) return void api.watchStop(sessionId, id).catch(() => {});
          set((s) => ({ live: { ...s.live, [key]: { sessionId, address, id } } }));
          schedule();
        } catch (e) {
          set((s) => ({ live: without(s.live, key) }));
          fail(e, "Could not start live mode");
        }
      },

      async pollLive() {
        const s = get();
        const entries = Object.entries(s.live);
        for (const [key] of entries) if (!shown(s, key)) stop(key);
        await Promise.all(entries.filter(([key]) => shown(s, key)).map(([key, watch]) => poll(key, watch)));
      },

      dismissMarks() {
        const target = selectChangeTarget(get());
        if (target) set((s) => ({ changeMarks: without(s.changeMarks, target.key) }));
      },

      takeSnapshot() {
        const target = selectChangeTarget(get());
        if (!target) return;
        get().openDialog({
          type: "prompt",
          title: `Snapshot of ${target.title}`,
          value: `snapshot ${clockTime(Date.now())}`,
          onSubmit: (label) => {
            void api
              .snapshotTake(target.sessionId, target.address, label)
              .then((info) => get().toast("success", `Snapshot ${info.label} taken`))
              .catch((e) => fail(e, "Could not take the snapshot"));
          },
        });
      },

      async chooseSnapshot() {
        const target = selectChangeTarget(get());
        if (!target) return;
        let snapshots: SnapshotInfo[];
        try {
          snapshots = await api.snapshotList(target.sessionId, target.database, target.address);
        } catch (e) {
          return void fail(e, "Could not list the snapshots");
        }
        if (snapshots.length === 0) return get().toast("info", `No snapshots of ${target.title} yet`);
        get().openDialog({ type: "snapshots", title: target.title, snapshots, onPick: (id) => void get().showSnapshotDiff(target, id) });
      },

      async showSnapshotDiff(target, id) {
        try {
          const { snapshot, delta, document } = await api.snapshotDiff(target.sessionId, id);
          if (document) get().keepDocument(target.sessionId, document);
          mark(target.key, { kind: "snapshot", label: snapshot.label, created: snapshot.created }, delta, true);
          if (isEmptyDelta(delta)) get().toast("info", `Nothing changed since snapshot ${snapshot.label}`);
        } catch (e) {
          fail(e, "Could not compare with the snapshot");
        }
      },
    };
  },

  sessionClosed: (sessionId, s) => ({ live: withoutSession(s.live, sessionId), changeMarks: withoutSession(s.changeMarks, sessionId) }),
};
