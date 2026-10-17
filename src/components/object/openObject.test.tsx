import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type TableStructure } from "../../api";
import { address, formatAddress } from "../../lib/address";
import { useStore } from "../../state/store";
import { browsePage, columnMeta, connectedStore, num, resultSet, tableTab, text } from "../../test/fixtures";
import { activeObject } from "../../test/objectFixtures";
import { TableTab } from "../TableTab";

const mocked = vi.mocked(api, true);

const structure: TableStructure = {
  columns: [],
  indexes: [{ name: "PRIMARY", unique: true, columns: ["ID"] }],
  primaryKey: ["ID"],
  foreignKeys: [],
  ddl: null,
};

const tab = tableTab();
const postsTable = address("shop", "db", { kind: "table", table: "wp_posts", filter: [] });

const page = browsePage({
  result: resultSet({
    columns: [columnMeta("ID", "BIGINT UNSIGNED"), columnMeta("post_title", "TEXT")],
    rows: [
      [num("7"), text("Hello world")],
      [num("8"), text("Second")],
    ],
    kinds: [{ kind: "primaryKey", table: postsTable, columns: ["ID"] }, { kind: "text" }],
  }),
  keys: [[{ column: "ID", value: "7" }], [{ column: "ID", value: "8" }]],
});

describe("opening a row as an object from a table tab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    useStore.setState({ tabs: [tab], activeTabId: tab.id });
    mocked.describeTable.mockResolvedValue(structure);
    mocked.browseTable.mockResolvedValue(page);
  });

  describe("row menu", () => {
    it("offers Open as object, which opens the row's object tab", async () => {
      render(<TableTab tab={tab} />);
      fireEvent.contextMenu(await screen.findByText("Second"));
      await userEvent.click(screen.getByRole("menuitem", { name: "Open as object" }));
      expect(activeObject()).toMatchObject({ view: "document", sessionId: "s1" });
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_posts/8");
    });

    it("offers no object for a table without a single-column primary key", async () => {
      mocked.describeTable.mockResolvedValue({ ...structure, indexes: [], primaryKey: [] });
      render(<TableTab tab={tab} />);
      fireEvent.contextMenu(await screen.findByText("Second"));
      expect(screen.queryByRole("menuitem", { name: "Open as object" })).not.toBeInTheDocument();
    });
  });

  describe("primary key cell", () => {
    it("opens the row as a preview object tab on click", async () => {
      render(<TableTab tab={tab} />);
      await userEvent.click(await screen.findByRole("link", { name: "7" }));
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_posts/7");
      expect(activeObject().preview).toBe(true);
    });

    it("pins the object tab on double-click", async () => {
      render(<TableTab tab={tab} />);
      await userEvent.dblClick(await screen.findByRole("link", { name: "7" }));
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_posts/7");
      expect(activeObject().preview).toBe(false);
    });

    it("opens the row as a preview object tab on Enter", async () => {
      render(<TableTab tab={tab} />);
      await userEvent.click(await screen.findByText("Hello world"));
      await userEvent.keyboard("{ArrowLeft}{Enter}");
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_posts/7");
      expect(activeObject().preview).toBe(true);
    });
  });
});
