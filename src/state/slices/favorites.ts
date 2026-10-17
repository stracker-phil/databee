import { api, toAppError, type Favorite, type FavoriteTarget } from "../../api";
import { beginRequest, fail, isLatest } from "../requests";
import { selectActiveSession } from "../selectors";
import type { Slice } from "../store";

export interface FavoritesSlice {
  favorites: Favorite[];
  loadFavorites(): Promise<void>;
  addFavorite(name: string, target: FavoriteTarget, global?: boolean): Promise<void>;
  addCurrentTabToFavorites(): void;
  renameFavorite(favorite: Favorite, name: string): Promise<void>;
  toggleFavoriteScope(favorite: Favorite): Promise<void>;
  removeFavorite(id: string): Promise<void>;
  moveFavorite(id: string, beforeId: string | null): Promise<void>;
  openFavorite(favorite: Favorite): void;
}

export const favoritesSlice: Slice<FavoritesSlice> = {
  create: (set, get) => ({
    favorites: [],

    async loadFavorites() {
      const session = selectActiveSession(get());
      // One key for all sessions: a reply for a session that is no longer active must not land.
      const token = beginRequest("favorites");
      if (!session) {
        set({ favorites: [] });
        return;
      }
      try {
        const favorites = await api.listFavorites(session.account);
        if (isLatest("favorites", token)) set({ favorites });
      } catch (e) {
        if (isLatest("favorites", token)) fail(e, "Could not load favorites");
      }
    },

    async addFavorite(name, target, global = false) {
      const session = selectActiveSession(get());
      if (!session) return;
      try {
        await api.createFavorite({ name, scope: global ? null : session.account, target });
        await get().loadFavorites();
      } catch (e) {
        const error = toAppError(e);
        // Rust refuses a duplicate target; that is a notice, not a failure.
        if (error.kind === "invalid_input") return get().toast("info", error.message);
        fail(e, "Could not add favorite");
      }
    },

    addCurrentTabToFavorites() {
      const tab = get().tabs.find((t) => t.id === get().activeTabId);
      if (!tab) return;
      const saved = (target: FavoriteTarget) => get().favorites.some((f) => JSON.stringify(f.target) === JSON.stringify(target));
      if (tab.kind === "table") {
        const target: FavoriteTarget = { kind: "table", database: tab.database, table: tab.table };
        if (saved(target)) return get().toast("info", `${tab.table} is already in favorites`);
        void get().addFavorite(tab.table, target);
        return;
      }
      if (tab.kind === "object") return get().toast("info", "Objects cannot be favorites yet; add their table instead");
      if (tab.kind === "project") return get().toast("info", "The Project page cannot be a favorite");
      if (!tab.sql.trim()) {
        get().toast("info", "Write a query first, then add it to favorites");
        return;
      }
      if (saved({ kind: "query", sql: tab.sql })) return get().toast("info", "This query is already in favorites");
      set({
        dialog: {
          type: "prompt",
          title: "Name this query",
          value: tab.title,
          onSubmit: (name) => void get().addFavorite(name, { kind: "query", sql: tab.sql }),
        },
      });
    },

    async renameFavorite(favorite, name) {
      try {
        await api.updateFavorite({ ...favorite, name });
        await get().loadFavorites();
      } catch (e) {
        fail(e, "Could not rename favorite");
      }
    },

    async toggleFavoriteScope(favorite) {
      const session = selectActiveSession(get());
      if (!session) return;
      try {
        await api.updateFavorite({ ...favorite, scope: favorite.scope ? null : session.account });
        await get().loadFavorites();
      } catch (e) {
        fail(e, "Could not update favorite");
      }
    },

    async removeFavorite(id) {
      try {
        await api.removeFavorite(id);
        set((s) => ({ favorites: s.favorites.filter((f) => f.id !== id) }));
      } catch (e) {
        fail(e, "Could not remove favorite");
      }
    },

    async moveFavorite(id, beforeId) {
      const previous = get().favorites;
      const ids = previous.map((f) => f.id).filter((x) => x !== id);
      const before = beforeId ? ids.indexOf(beforeId) : -1;
      ids.splice(before >= 0 ? before : ids.length, 0, id);
      const byId = new Map(previous.map((f) => [f.id, f]));
      set({ favorites: ids.flatMap((x) => byId.get(x) ?? []) });
      try {
        await api.reorderFavorites(ids);
      } catch (e) {
        set({ favorites: previous });
        fail(e, "Could not reorder favorites");
      }
    },

    openFavorite(favorite) {
      if (favorite.target.kind === "table") get().openTableTab(favorite.target.database, favorite.target.table);
      else get().openQueryTab(favorite.target.sql, favorite.name);
    },
  }),
};
