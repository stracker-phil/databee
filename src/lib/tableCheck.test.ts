import { describe, expect, it } from "vitest";
import { changeCounts, changesText, freshCheck, rowsToRead, withHashes, withRows, type RowChange, type TableCheckState } from "./tableCheck";
import { text } from "../test/fixtures";

const change = (patch: Partial<RowChange> = {}): RowChange => ({ kind: "changed", at: 100, hash: null, cells: null, columns: [], ...patch });
const state = (patch: Partial<TableCheckState> = {}): TableCheckState => ({ ...freshCheck(1), baseline: ["a", "b", "c"], ...patch });

describe("withHashes", () => {
  it("takes the first check as the baseline and marks nothing", () => {
    const next = withHashes(freshCheck(1), ["a", "b"], 10);
    expect(next.baseline).toEqual(["a", "b"]);
    expect(next.rows).toEqual({});
  });

  it("marks a row already gone at the first check as removed", () => {
    const next = withHashes(freshCheck(1), ["a", null], 10);
    expect(next.rows).toEqual({ 1: change({ kind: "removed", at: 10 }) });
  });

  it("marks a row with another checksum as changed and a missing one as removed", () => {
    const next = withHashes(state(), ["a", "x", null], 20);
    expect(next.rows).toEqual({ 1: change({ kind: "changed", at: 20 }), 2: change({ kind: "removed", at: 20 }) });
    expect(next.baseline).toEqual(["a", "b", "c"]);
  });

  it("unmarks a row that is back at its baseline checksum", () => {
    const next = withHashes(state({ rows: { 1: change() } }), ["a", "b", "c"], 30);
    expect(next.rows).toEqual({});
  });

  it("keeps the mark, and the values read for it, while its kind stays", () => {
    const read = change({ at: 100, hash: "x", cells: [text("new")], columns: [0] });
    const next = withHashes(state({ rows: { 1: read } }), ["a", "y", "c"], 40);
    expect(next.rows[1]).toBe(read);
  });

  it("starts a new mark when the kind changes", () => {
    const next = withHashes(state({ rows: { 1: change({ at: 100, hash: "x", cells: [text("new")], columns: [0] }) } }), ["a", null, "c"], 50);
    expect(next.rows[1]).toEqual(change({ kind: "removed", at: 50 }));
  });
});

describe("rowsToRead", () => {
  it("lists changed rows not read at their current checksum", () => {
    const rows = { 0: change({ hash: null }), 1: change({ hash: "old" }), 2: change({ hash: "y" }) };
    expect(rowsToRead(state({ rows }), ["a", "new", "y"])).toEqual([0, 1]);
  });

  it("skips removed rows", () => {
    expect(rowsToRead(state({ rows: { 1: change({ kind: "removed" }) } }), ["a", null, "c"])).toEqual([]);
  });
});

describe("withRows", () => {
  it("stores the checksum, cells and changed columns of a still changed row", () => {
    const next = withRows(state({ rows: { 1: change() } }), [{ row: 1, hash: "x", cells: [text("1"), text("new")], changed: [1] }]);
    expect(next.rows[1]).toEqual(change({ hash: "x", cells: [text("1"), text("new")], columns: [1] }));
  });

  it("unmarks a row read back at its baseline checksum", () => {
    const next = withRows(state({ rows: { 1: change() } }), [{ row: 1, hash: "b", cells: [text("1")], changed: [] }]);
    expect(next.rows).toEqual({});
  });

  it("ignores rows that are not marked changed", () => {
    const rows = { 2: change({ kind: "removed" }) };
    const next = withRows(state({ rows }), [{ row: 2, hash: "x", cells: [], changed: [] }, { row: 0, hash: "x", cells: [], changed: [] }]);
    expect(next.rows).toEqual(rows);
  });
});

describe("changeCounts", () => {
  it("counts changed and removed rows", () => {
    expect(changeCounts({ 0: change(), 1: change(), 2: change({ kind: "removed" }) })).toEqual({ changed: 2, removed: 1 });
  });

  it("counts nothing without marks", () => {
    expect(changeCounts(undefined)).toEqual({ changed: 0, removed: 0 });
  });
});

describe("changesText", () => {
  it.each([
    [{ changed: 1, removed: 0 }, "1 row changed in the database"],
    [{ changed: 3, removed: 0 }, "3 rows changed in the database"],
    [{ changed: 0, removed: 1 }, "1 row removed from the database"],
    [{ changed: 0, removed: 2 }, "2 rows removed from the database"],
    [{ changed: 2, removed: 1 }, "2 rows changed and 1 row removed in the database"],
    [{ changed: 1, removed: 2 }, "1 row changed and 2 rows removed in the database"],
    [{ changed: 0, removed: 0 }, null],
  ])("words %j as %j", (counts, expected) => {
    expect(changesText(counts)).toBe(expected);
  });
});
