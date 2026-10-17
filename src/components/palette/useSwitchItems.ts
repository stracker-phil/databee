import { useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import { api, type ConnectionSpec, type DatabaseInfo, type RecentConnection, type SessionInfo } from "../../api";
import { useApiResult } from "../../lib/useApiResult";
import { fail, selectActiveSession, selectVisibleDatabases, useStore } from "../../state/store";
import { fuzzyScore, type Prefix } from "./parse";
import type { Item } from "./usePaletteItems";

interface Connection {
  identity: string;
  spec: ConnectionSpec;
  slug: string;
  session?: SessionInfo;
}

const close = () => useStore.getState().setPaletteOpen(false);

const endpoint = (spec: ConnectionSpec) => spec.socket ?? `${spec.host}:${spec.port}`;

/** Open sessions first (the active one on top), then the recents that are not open. */
function connections(sessions: SessionInfo[], activeId: string | null, recents: RecentConnection[]): Connection[] {
  const open = [...sessions].sort((a, b) => Number(b.id === activeId) - Number(a.id === activeId));
  const openIds = new Set(open.map((s) => s.identity));
  return [
    ...open.map((session) => ({ identity: session.identity, spec: session.spec, slug: session.slug, session })),
    ...recents.filter((r) => !openIds.has(r.identity)).map((r) => ({ identity: r.identity, spec: r.spec, slug: r.slug })),
  ];
}

function connectionItems(list: Connection[], activeId: string | null, needle: string): Item[] {
  return list
    .map((c) => {
      const title = c.spec.name ?? c.session?.displayName ?? c.spec.host;
      const database = c.session?.database ?? c.spec.database ?? "";
      return { c, title, database, s: fuzzyScore([title, endpoint(c.spec), database, c.slug], needle) };
    })
    .filter(({ s }) => s > 0)
    .sort((a, b) => b.s - a.s)
    .map(({ c, title, database }): Item => ({
      key: `connection:${c.identity}`,
      section: "Connections",
      title,
      subtitle: database ? `${endpoint(c.spec)} / ${database}` : endpoint(c.spec),
      note: c.slug,
      badge: c.session ? (c.session.id === activeId ? "current" : "open") : undefined,
      go: () => {
        close();
        const s = useStore.getState();
        if (c.session) s.setActiveSession(c.session.id);
        else void s.connect({ spec: c.spec });
      },
    }));
}

function databaseItems(session: SessionInfo | null, databases: DatabaseInfo[], needle: string): Item[] {
  if (!session) return [];
  return databases
    .map((d) => ({ d, s: fuzzyScore([d.name], needle) }))
    .filter(({ s }) => s > 0)
    .sort((a, b) => b.s - a.s)
    .map(({ d }): Item => ({
      key: `database:${d.name}`,
      section: "Databases",
      title: d.name,
      badge: d.name === session.database ? "current" : undefined,
      go: () => {
        close();
        if (d.name !== session.database) void useStore.getState().switchDatabase(session.id, d.name);
      },
    }));
}

/** `c:`: saved and open connections, then the active connection's databases. */
export function useSwitchItems(prefix: Prefix, needle: string): Item[] {
  const active = prefix === "c:";
  const recents = useApiResult(active ? "recents" : null, api.listRecents, { onError: (e) => fail(e, "Could not list recent connections") });
  const sessions = useStore((s) => s.sessions);
  const activeId = useStore((s) => s.activeSessionId);
  const session = useStore(selectActiveSession) ?? null;
  const databases = useStore(useShallow(selectVisibleDatabases));
  return useMemo(() => {
    if (!active) return [];
    return [...connectionItems(connections(sessions, activeId, recents.data ?? []), activeId, needle), ...databaseItems(session, databases, needle)];
  }, [active, sessions, activeId, recents.data, needle, session, databases]);
}
