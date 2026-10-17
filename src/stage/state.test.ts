import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { resetStore, useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { wpStore } from "../test/wpFixtures";
import { stageState, stageUi } from "./state";

describe("stageState WordPress facts", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
  });

  it("reports the prefix and the applied presets", () => {
    wpStore();
    expect(stageState()).toMatchObject({ prefix: "wp_", presets: "wordpress, woocommerce" });
  });

  it("reports neither for a database without WordPress info", () => {
    connectedStore();
    const state = stageState();
    expect(state).not.toHaveProperty("prefix");
    expect(state).not.toHaveProperty("presets");
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

  it("is closed in WordPress mode", () => {
    wpStore();
    expect(stageUi().sidebar).toBe("closed");
  });
});
