import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { resetStore, useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { readoutTab } from "../test/fixtures";
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
