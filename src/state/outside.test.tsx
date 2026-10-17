import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { formatAddress } from "../lib/address";
import { connectedStore } from "../test/fixtures";
import { activeObject } from "../test/objectFixtures";
import { useStore } from "./store";

const mocked = vi.mocked(api, true);

describe("handleLaunchEvents open", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
  });

  it("opens the address the launch names", async () => {
    await act(() => useStore.getState().handleLaunchEvents([{ type: "open", address: "databee://shop/db/wp_wc_orders/4711", connection: null }]));
    expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_wc_orders/4711");
  });

  it("toasts the reason when the address names a connection that is not open", async () => {
    await act(() => useStore.getState().handleLaunchEvents([{ type: "open", address: "databee://ghost/db/wp_posts", connection: null }]));
    expect(useStore.getState().tabs).toEqual([]);
    expect(useStore.getState().toasts).toEqual([expect.objectContaining({ kind: "error", message: "No open connection is named ghost" })]);
    expect(mocked.document).not.toHaveBeenCalled();
  });
});
