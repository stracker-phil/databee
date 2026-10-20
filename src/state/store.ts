import { create, type StoreApi } from "zustand";
import { changesSlice, type ChangesSlice } from "./slices/changes";
import { columnsSlice, type ColumnsSlice } from "./slices/columns";
import { dataSlice, type DataSlice } from "./slices/data";
import { healthSlice, type HealthSlice } from "./slices/health";
import { liveSlice, type LiveSlice } from "./slices/live";
import { navSlice, type NavSlice } from "./slices/nav";
import { objectsSlice, type ObjectsSlice } from "./slices/objects";
import { projectSlice, type ProjectSlice } from "./slices/project";
import { readoutSlice, type ReadoutSlice } from "./slices/readout";
import { savedSlice, type SavedSlice } from "./slices/saved";
import { sessionSlice, type SessionSlice } from "./slices/session";
import { settingsSlice, type SettingsSlice } from "./slices/settings";
import { querySlice, type QuerySlice } from "./slices/query";
import { tabsSlice, type TabsSlice } from "./slices/tabs";
import { uiSlice, type UiSlice } from "./slices/ui";
import { viewStateSlice, type ViewStateSlice } from "./slices/viewState";

export { fail } from "./requests";
export { NO_HOVER } from "./slices/ui";
export { flushUiState } from "./slices/viewState";
export {
  selectActivePending,
  selectActiveSession,
  selectActiveTab,
  selectCanNav,
  selectCanUndo,
  selectOffersUndo,
  selectCollapsedGroups,
  selectDocument,
  selectFocusedValue,
  selectMaxCellChars,
  selectMaxPanes,
  selectNullLabel,
  selectSessionBySlug,
  selectSidebarHidden,
  selectStructure,
  selectVisibleDatabases,
} from "./selectors";
export { hasValueColumn } from "./slices/tabs";
export type { ObjectTab, ProjectTab, QueryTab, ReadoutTab, SelectedCell, Tab, TableTab } from "./slices/tabs";
export type { RecentNav, Toast, ValueHover } from "./slices/ui";
export { queryMarksKey, selectActiveLive, selectCanDismissMarks, selectChangeTarget } from "./slices/live";
export { accentKey, selectDatabaseAccent, selectEffectiveAccent } from "./slices/project";
export { blockedWhileLost, type LostHealth } from "./slices/health";

/** Every slice's members; modules add theirs with `declare module "../state/store" { interface Store { … } }`. */
export interface Store extends SettingsSlice, SessionSlice, TabsSlice, QuerySlice, ChangesSlice, DataSlice, SavedSlice, UiSlice, ViewStateSlice, NavSlice, ObjectsSlice, ColumnsSlice, ProjectSlice, ReadoutSlice, LiveSlice, HealthSlice {}

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
registerSlice(querySlice);
registerSlice(changesSlice);
registerSlice(dataSlice);
registerSlice(savedSlice);
registerSlice(uiSlice);
registerSlice(viewStateSlice);
registerSlice(navSlice);
registerSlice(objectsSlice);
registerSlice(columnsSlice);
registerSlice(projectSlice);
registerSlice(readoutSlice);
registerSlice(liveSlice);
registerSlice(healthSlice);
