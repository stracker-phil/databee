import { api, type Health, type HealthEvent } from "../../api";
import { fail } from "../requests";
import type { Slice, Store } from "../store";

/** A session whose server stopped answering, as Rust reports it (`session_health` events). */
export type LostHealth = Exclude<Health, { state: "ok" }>;

export interface HealthSlice {
  /** Sessions that are `down` or `reconnecting`, by session id; a session that is not here is ok. */
  health: Record<string, LostHealth>;
  /** A `session_health` event; a session that is back reloads the view. */
  setHealth: (event: HealthEvent) => void;
  /** Retry: reconnects with the password Rust holds. `quiet` (the focus retry) toasts nothing when it fails. */
  retryConnection: (sessionId: string, quiet?: boolean) => Promise<void>;
  /** Retries every `down` session once, quietly; runs when the window regains focus. */
  retryLostSessions: () => void;
}

const nameOf = (s: Store, sessionId: string) => s.sessions.find((x) => x.id === sessionId)?.displayName ?? "the server";

/**
 * Stops an action that needs the server while `sessionId` is not connected, with a short toast instead of an
 * error; true when it stopped. `what` completes "can't …", e.g. "run queries".
 */
export function blockedWhileLost(get: () => Store, sessionId: string | null | undefined, what: string): boolean {
  if (!sessionId || !get().health[sessionId]) return false;
  get().toast("info", `Connection lost: can't ${what} until it is back`);
  return true;
}

export const healthSlice: Slice<HealthSlice> = {
  create: (set, get) => ({
    health: {},

    setHealth({ sessionId, health }) {
      const s = get();
      if (!s.sessions.some((x) => x.id === sessionId)) return;
      const lost = s.health[sessionId];
      if (health.state !== "ok") {
        if (!lost) s.toast("info", `Lost the connection to ${nameOf(s, sessionId)}`);
        set((st) => ({ health: { ...st.health, [sessionId]: health } }));
        return;
      }
      if (!lost) return;
      const { [sessionId]: _back, ...rest } = s.health;
      set({ health: rest });
      s.toast("success", `Reconnected to ${nameOf(s, sessionId)}`);
      if (s.activeSessionId === sessionId) void s.refreshView();
    },

    async retryConnection(sessionId, quiet = false) {
      try {
        // The state arrives as events, in order, and this reply may overtake them; only an `ok` reply is applied too,
        // since Rust sends no event for a session it already considers healthy.
        const health = await api.reconnectSession(sessionId);
        if (health.state === "ok") get().setHealth({ sessionId, health });
        if (!quiet && health.state === "down") get().toast("info", `Still no connection to ${nameOf(get(), sessionId)}: ${health.error}`);
      } catch (e) {
        if (!quiet) fail(e, "Retry failed");
      }
    },

    retryLostSessions() {
      for (const [sessionId, health] of Object.entries(get().health)) {
        if (health.state === "down") void get().retryConnection(sessionId, true);
      }
    },
  }),

  sessionClosed(sessionId, s) {
    const { [sessionId]: _closed, ...health } = s.health;
    return { health };
  },
};
