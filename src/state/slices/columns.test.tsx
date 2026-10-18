import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { installCommands } from "../../commands";
import { address } from "../../lib/address";
import { connectedStore, session } from "../../test/fixtures";
import { activeObject, objectDocument, orderAddress } from "../../test/objectFixtures";
import { useStore } from "../store";

const mocked = vi.mocked(api, true);

const rowOf = (table: string, key: string) => address(session.slug, "db", { kind: "row", table, key });
const userRow = rowOf("wp_users", "12");
const postRow = rowOf("wp_posts", "5");

const store = () => useStore.getState();

function openStack(columns: 1 | 2 | 3) {
  store().openObject(orderAddress);
  if (columns > 1) store().drill(activeObject().id, 0, userRow);
  if (columns > 2) store().drill(activeObject().id, 1, postRow);
  return activeObject().id;
}

describe("maximized panes", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    mocked.document.mockResolvedValue(objectDocument());
  });

  describe("toggleMaximized", () => {
    it("does nothing while the stack has one column", () => {
      store().toggleMaximized(openStack(1));
      expect(activeObject().maximized).toBeUndefined();
    });

    it("maximizes a stack of several columns and restores it on the next toggle", () => {
      const id = openStack(2);
      store().toggleMaximized(id);
      expect(activeObject().maximized).toBe(true);
      store().toggleMaximized(id);
      expect(activeObject().maximized).toBeUndefined();
    });
  });

  describe("closeRightmostColumn", () => {
    it("restores a maximized stack before it closes a column", () => {
      const id = openStack(2);
      store().toggleMaximized(id);

      store().closeRightmostColumn();
      expect(activeObject().maximized).toBeUndefined();
      expect(activeObject().columns).toHaveLength(2);

      store().closeRightmostColumn();
      expect(activeObject().columns).toHaveLength(1);
    });
  });

  describe("changing the stack", () => {
    it("stays maximized when a column is drilled", () => {
      const id = openStack(2);
      store().toggleMaximized(id);
      store().drill(id, 1, postRow);
      expect(activeObject().maximized).toBe(true);
      expect(activeObject().columns).toHaveLength(3);
    });

    it("clears when the stack pops down to one column", () => {
      const id = openStack(2);
      store().toggleMaximized(id);
      store().popToColumn(id, 0);
      expect(activeObject().columns).toHaveLength(1);
      expect(activeObject().maximized).toBeUndefined();
    });

    it("stays maximized across back and forward between stacks of several columns", async () => {
      const id = openStack(3);
      store().toggleMaximized(id);

      await store().navBack();
      expect(activeObject().columns).toHaveLength(2);
      expect(activeObject().maximized).toBe(true);

      await store().navForward();
      expect(activeObject().columns).toHaveLength(3);
      expect(activeObject().maximized).toBe(true);
    });
  });
});

describe("setPaneWidths", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    mocked.document.mockResolvedValue(objectDocument());
  });

  it("stores the shares of the shown panes and null makes them equal again", () => {
    const id = openStack(2);
    store().setPaneWidths(id, [0.3, 0.7]);
    expect(activeObject().paneWidths).toEqual([0.3, 0.7]);
    store().setPaneWidths(id, null);
    expect(activeObject().paneWidths).toBeUndefined();
  });
});
