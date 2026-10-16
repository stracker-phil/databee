import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type Cell } from "../api";
import { fullCellText } from "./cells";

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
