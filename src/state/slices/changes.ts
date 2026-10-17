import { api, type Address, type ChangeRequest, type PlannedChange } from "../../api";
import { formatAddress } from "../../lib/address";
import { fail } from "../requests";
import { selectActivePending, selectActiveSession, selectLastBatch } from "../selectors";
import type { Slice } from "../store";

export interface ChangesSlice {
  /** Planned edits waiting for review, and applied batches that can be undone. */
  pending: (PlannedChange & { sessionId: string })[];
  applied: { sessionId: string; batchId: number; canUndo: boolean }[];
  /** An apply or undo is in flight; both refuse to start another. */
  applying: boolean;
  /** Bumped to move focus to the pending tray's Apply button. */
  trayFocusRequest: number;
  queueChange: (change: ChangeRequest) => Promise<boolean>;
  /** Plans rule action `action` on a row address (or a table address for table actions) for review. */
  queueAction: (address: Address, action: string, params: Record<string, string>) => Promise<boolean>;
  discardChange: (id: number) => Promise<void>;
  discardAll: () => Promise<void>;
  applyChanges: () => Promise<void>;
  undoLast: () => Promise<void>;
  focusPendingTray: () => void;
}

export const changesSlice: Slice<ChangesSlice> = {
  create: (set, get) => {
    /** Runs one apply or undo at a time; key repeat and double clicks must not send a batch twice. */
    const whileApplying = async (work: () => Promise<void>) => {
      set({ applying: true });
      try {
        await work();
      } finally {
        set({ applying: false });
      }
    };

    /** Keeps the plan Rust returns; a refused plan becomes an error toast. */
    const queue = async (sessionId: string, plan: () => Promise<PlannedChange>) => {
      try {
        const planned = await plan();
        set((s) => ({ pending: [...s.pending, { ...planned, sessionId }] }));
        return true;
      } catch (e) {
        fail(e, "Cannot prepare change");
        return false;
      }
    };

    return {
      pending: [],
      applied: [],
      applying: false,
      trayFocusRequest: 0,

      async queueChange(change) {
        const session = selectActiveSession(get());
        if (!session) return false;
        return queue(session.id, () => api.planChange(session.id, change));
      },

      async queueAction(address, action, params) {
        const session = get().sessions.find((x) => x.slug === address.connection);
        if (!session) return false;
        return queue(session.id, () => api.planAction(session.id, formatAddress(address), action, params));
      },

      async discardChange(id) {
        const item = get().pending.find((p) => p.id === id);
        if (!item) return;
        try {
          await api.discardChange(item.sessionId, id);
          set((s) => ({ pending: s.pending.filter((p) => p !== item) }));
        } catch (e) {
          fail(e, "Cannot discard change");
        }
      },

      async discardAll() {
        for (const item of selectActivePending(get())) await get().discardChange(item.id);
      },

      async applyChanges() {
        const session = selectActiveSession(get());
        const batch = selectActivePending(get());
        if (get().applying || !session || batch.length === 0) return;
        await whileApplying(async () => {
          try {
            const applied = await api.applyChanges(
              session.id,
              batch.map((p) => p.id),
            );
            set((s) => ({
              pending: s.pending.filter((p) => !batch.includes(p)),
              applied: [...s.applied, { sessionId: session.id, batchId: applied.batchId, canUndo: applied.canUndo }].slice(-20),
            }));
            get().toast("success", `Applied ${batch.length} ${batch.length === 1 ? "change" : "changes"}`);
            get().bumpData();
          } catch (e) {
            fail(e, "Changes were not applied");
          }
        });
      },

      async undoLast() {
        const session = selectActiveSession(get());
        const last = selectLastBatch(get());
        if (get().applying || !session || !last) return;
        if (!last.canUndo) {
          get().toast("info", "This change can't be undone automatically");
          return;
        }
        await whileApplying(async () => {
          try {
            await api.undoBatch(session.id, last.batchId);
            set((s) => ({ applied: s.applied.filter((a) => a !== last) }));
            get().toast("success", "Undone");
            get().bumpData();
          } catch (e) {
            fail(e, "Undo failed");
          }
        });
      },

      focusPendingTray() {
        set((s) => ({ trayFocusRequest: s.trayFocusRequest + 1 }));
      },
    };
  },

  sessionClosed: (sessionId, s) => ({
    pending: s.pending.filter((p) => p.sessionId !== sessionId),
    applied: s.applied.filter((a) => a.sessionId !== sessionId),
  }),
};
