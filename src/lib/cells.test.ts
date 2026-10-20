import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type Cell } from "../api";
import { browsePage, columnMeta, num, resultSet, text } from "../test/fixtures";
import { fetchStoredValue, fullCellText } from "./cells";

const mocked = vi.mocked(api, true);

const longText: Cell = { t: "longText", v: { len: 100000, preview: "xxxx" } };
const source = { sessionId: "s1", resultId: 7 };

describe("fullCellText", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.fetchCell.mockResolvedValue("x".repeat(100000));
  });

  it("returns the text of a normal cell without fetching", async () => {
    expect(await fullCellText({ t: "text", v: "hello" }, source, 0, 1)).toBe("hello");
    expect(mocked.fetchCell).not.toHaveBeenCalled();
  });

  it("fetches a long text cell by its result, row and column", async () => {
    expect(await fullCellText(longText, source, 3, 2)).toBe("x".repeat(100000));
    expect(mocked.fetchCell).toHaveBeenCalledWith("s1", 7, 3, 2);
  });

  it("rejects when the result has expired", async () => {
    mocked.fetchCell.mockRejectedValue({ kind: "invalid_input", message: "Result expired; run the query again", code: null });
    await expect(fullCellText(longText, source, 0, 0)).rejects.toMatchObject({ message: "Result expired; run the query again" });
  });
});

describe("fetchStoredValue", () => {
  const meta = { table: "wp_postmeta", column: "meta_value", row: [{ column: "ID", value: "1" }] };
  const pageOf = (rows: Cell[][], id = 1, names = ["ID", "meta_value"]) =>
    browsePage({ table: "wp_postmeta", result: resultSet({ id, columns: names.map((name) => columnMeta(name, "LONGTEXT")), rows, totalRows: rows.length }) });

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("reads the row by its key and returns the cell with its text", async () => {
    mocked.browseTable.mockResolvedValue(pageOf([[num("1"), text("stored")]]));
    expect(await fetchStoredValue("s1", "db", meta)).toEqual({ cell: text("stored"), text: "stored" });
    expect(mocked.browseTable).toHaveBeenCalledWith("s1", { database: "db", table: "wp_postmeta", offset: 0, limit: 1, where: [{ column: "ID", value: "1" }] });
  });

  it("finds the column whatever the case of its name in the result", async () => {
    mocked.browseTable.mockResolvedValue(pageOf([[num("1"), text("stored")]], 1, ["id", "META_VALUE"]));
    expect((await fetchStoredValue("s1", "db", meta))?.text).toBe("stored");
  });

  it("fetches a long text cell from the new result, at row 0", async () => {
    mocked.browseTable.mockResolvedValue(pageOf([[num("1"), longText]], 42));
    mocked.fetchCell.mockResolvedValue("full stored text");
    expect(await fetchStoredValue("s1", "db", meta)).toEqual({ cell: longText, text: "full stored text" });
    expect(mocked.fetchCell).toHaveBeenCalledWith("s1", 42, 0, 1);
  });

  it("returns null when no row has the key any more", async () => {
    mocked.browseTable.mockResolvedValue(pageOf([]));
    expect(await fetchStoredValue("s1", "db", meta)).toBeNull();
  });

  it("rejects when the table no longer has the column", async () => {
    mocked.browseTable.mockResolvedValue(pageOf([[num("1"), text("stored")]]));
    await expect(fetchStoredValue("s1", "db", { ...meta, column: "dropped" })).rejects.toThrow("wp_postmeta has no column dropped");
  });
});
