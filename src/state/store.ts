import { create, type StoreApi } from "zustand";
import { changesSlice, type ChangesSlice } from "./slices/changes";
import { dataSlice, type DataSlice } from "./slices/data";
import { favoritesSlice, type FavoritesSlice } from "./slices/favorites";
import { navSlice, type NavSlice } from "./slices/nav";
import { sessionSlice, type SessionSlice } from "./slices/session";
import { settingsSlice, type SettingsSlice } from "./slices/settings";
import { tabsSlice, type TabsSlice } from "./slices/tabs";
import { uiSlice, type UiSlice } from "./slices/ui";
import { wpSlice, type WpSlice } from "./slices/wp";

export { fail } from "./requests";
export { flushUiState } from "./slices/ui";
export {
  selectActivePending,
  selectActiveSession,
  selectActiveTab,
  selectCanNav,
  selectCanUndo,
  selectSidebarHidden,
  selectStructure,
  selectVisibleDatabases,
  selectWp,
  isWpView,
  selectWpRoute,
} from "./selectors";
export type { TabNav, TableLocation } from "./slices/nav";
export type { SchemaState, StructureEntry } from "./slices/session";
export type { QueryTab, SelectedCell, Tab, TableTab } from "./slices/tabs";
export type { Dialog, Mode, RecentNav, Toast } from "./slices/ui";
export type { EntityMeta, WpList, WpRoute } from "./slices/wp";

/** Every slice's members; modules add theirs with `declare module "../state/store" { interface Store { … } }`. */
export interface Store extends SettingsSlice, SessionSlice, TabsSlice, ChangesSlice, DataSlice, FavoritesSlice, UiSlice, WpSlice, NavSlice {}

type SetStore = StoreApi<Store>["setState"];
type GetStore = StoreApi<Store>["getState"];

export interface Slice<T> {
  /** Initial state and actions; also re-run by `resetStore`. */
  create(set: SetStore, get: GetStore): T;
  /** What to change when `sessionId` closes; merged with every other slice's result into one update. */
  sessionClosed?(sessionId: string, s: Store): Partial<Store>;
}

// Empty until the slices below register; registerSlice fills it.
export const useStore = create<Store>()(() => ({}) as Store);

const slices: Slice<Partial<Store>>[] = [];

/** Adds a slice's state and actions to the live store; works after creation (modules register late). */
export function registerSlice<T extends Partial<Store>>(slice: Slice<T>) {
  slices.push(slice);
  useStore.setState(slice.create(useStore.setState, useStore.getState));
}

/** The combined update every registered slice makes when `sessionId` closes. */
export function sessionClosedPatch(sessionId: string, s: Store): Partial<Store> {
  return slices.reduce<Partial<Store>>((patch, slice) => ({ ...patch, ...slice.sessionClosed?.(sessionId, s) }), {});
}

/** Restores every registered slice's initial state, then applies `patch`; for tests. */
export function resetStore(patch: Partial<Store> = {}) {
  const initial = slices.reduce<Partial<Store>>((state, slice) => ({ ...state, ...slice.create(useStore.setState, useStore.getState) }), {});
  useStore.setState({ ...initial, ...patch });
}

registerSlice(settingsSlice);
registerSlice(sessionSlice);
registerSlice(tabsSlice);
registerSlice(changesSlice);
registerSlice(dataSlice);
registerSlice(favoritesSlice);
registerSlice(uiSlice);
registerSlice(wpSlice);
registerSlice(navSlice);
