import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { address } from "../lib/address";
import { resetStore, useStore } from "../state/store";
import { connectedStore, readoutTab, session } from "../test/fixtures";
import { activeObject, objectDocument, orderAddress } from "../test/objectFixtures";
import { stageState, stageUi } from "./state";

describe("stageState readout", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("names the database of the active readout tab", () => {
    const tab = readoutTab();
    connectedStore({ tabs: [tab] });
    expect(stageState()).toMatchObject({ tab: "readout: readout", readout: "db" });
  });

  it("reports no readout while another tab is active", () => {
    connectedStore();
    expect(stageState().readout).toBeUndefined();
  });
});

describe("stageState object", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api, true).document.mockResolvedValue(objectDocument());
    connectedStore({ tabs: [] });
    useStore.getState().openObject(orderAddress);
  });

  const objectLine = () => stageState().object;

  it("names the column and the stack size once the stack holds several columns", () => {
    useStore.getState().drill(activeObject().id, 0, address(session.slug, "db", { kind: "row", table: "wp_users", key: "12" }));
    expect(objectLine()).toMatch(/ · column 2 of 2$/);
  });

  it("adds maximized to the line while a pane is maximized", () => {
    useStore.getState().drill(activeObject().id, 0, address(session.slug, "db", { kind: "row", table: "wp_users", key: "12" }));
    useStore.getState().toggleMaximized(activeObject().id);
    expect(objectLine()).toMatch(/ · column 2 of 2 · maximized$/);
  });

  it("names only the address for a single column", () => {
    expect(objectLine()).toBe("databee://shop/db/wp_wc_orders/4711 (document)");
  });
});

describe("stageUi sidebar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
  });

  it("is open by default in the database view", () => {
    connectedStore();
    expect(stageUi().sidebar).toBe("open");
  });

  it("is closed after the sidebar is toggled away", () => {
    connectedStore();
    useStore.getState().toggleSidebar();
    expect(stageUi().sidebar).toBe("closed");
  });
});
