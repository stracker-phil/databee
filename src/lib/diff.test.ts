import { describe, expect, it } from "vitest";
import type { DocDelta, Group, RowMark, ValueChange } from "../api";
import { text, value } from "../test/fixtures";
import { source } from "../test/objectFixtures";
import { compareMarks, noMarks, sinceText, withDelta, withRemovedRows } from "./diff";
import { provenanceKey } from "./document";

const live = noMarks({ kind: "live" });

const statusSource = source("wp_orders", "status", "1");
const statusChange: ValueChange = { group: "own", label: "status", source: statusSource, oldSource: statusSource, old: text("pending"), new: text("paid") };

const metaRow = (id: string, key: string, raw: string): RowMark => ({
  group: "meta",
  table: "wp_ordermeta",
  key: [{ column: "id", value: id }],
  label: key,
  value: text(raw),
  fields: [
    ["meta_key", value({ raw: text(key), source: source("wp_ordermeta", "meta_key", id) })],
    ["meta_value", value({ raw: text(raw), source: source("wp_ordermeta", "meta_value", id) })],
  ],
});

const plainRow = (id: string): RowMark => ({
  group: "items",
  table: "wp_items",
  key: [{ column: "id", value: id }],
  label: `id ${id}`,
  value: null,
  fields: [
    ["id", value({ raw: text(id), source: null })],
    ["name", value({ raw: text("Mug"), source: source("wp_items", "name", id) })],
  ],
});

const delta = (patch: Partial<DocDelta>): DocDelta => ({ changed: [], added: [], removed: [], ...patch });

describe("withDelta", () => {
  it("remembers the old value of a changed value and adds a line for it", () => {
    const marks = withDelta(live, delta({ changed: [statusChange] }), 1000);
    expect(marks.changed[provenanceKey(statusSource)]).toEqual({ old: text("pending"), at: 1000 });
    expect(marks.lines).toEqual([{ at: 1000, kind: "changed", table: "wp_orders", label: "status", old: text("pending"), new: text("paid"), source: statusSource }]);
  });

  it("keeps the latest old value and every line when a value changes twice", () => {
    const first = withDelta(live, delta({ changed: [statusChange] }), 1000);
    const second = withDelta(first, delta({ changed: [{ ...statusChange, old: text("paid"), new: text("shipped") }] }), 2000);
    expect(second.changed[provenanceKey(statusSource)]).toEqual({ old: text("paid"), at: 2000 });
    expect(second.lines.map((l) => [l.at, l.old, l.new])).toEqual([
      [1000, text("pending"), text("paid")],
      [2000, text("paid"), text("shipped")],
    ]);
  });

  it("does not change the marks it was given", () => {
    withDelta(live, delta({ changed: [statusChange], added: [metaRow("1", "_a", "x")], removed: [metaRow("2", "_b", "y")] }), 1000);
    expect(live).toEqual(noMarks({ kind: "live" }));
  });

  it("marks every sourced value of an added row and jumps to a meta row's value", () => {
    const row = metaRow("7", "_paid_via", "bacs");
    const marks = withDelta(live, delta({ added: [row] }), 5);
    expect(Object.keys(marks.added).sort()).toEqual([provenanceKey(source("wp_ordermeta", "meta_key", "7")), provenanceKey(source("wp_ordermeta", "meta_value", "7"))]);
    expect(marks.lines).toEqual([{ at: 5, kind: "added", table: "wp_ordermeta", label: "_paid_via", old: null, new: text("bacs"), source: source("wp_ordermeta", "meta_value", "7") }]);
  });

  it("jumps to the first sourced value of an added row that is no meta row", () => {
    const marks = withDelta(live, delta({ added: [plainRow("3")] }), 5);
    expect(marks.lines[0]).toMatchObject({ kind: "added", new: null, source: source("wp_items", "name", "3") });
  });

  it("keeps a removed row once and drops the marks of its values", () => {
    const row = metaRow("9", "_gone", "1");
    const added = withDelta(live, delta({ added: [row] }), 1);
    const removed = withDelta(added, delta({ removed: [row] }), 2);
    expect(removed.added).toEqual({});
    expect(removed.removed).toEqual([row]);
    expect(withDelta(removed, delta({ removed: [row] }), 3).removed).toEqual([row]);
    expect(removed.lines[1]).toMatchObject({ kind: "removed", label: "_gone", old: text("1"), new: null });
  });

  it("takes a removed row back when the same row appears again", () => {
    const row = metaRow("9", "_flaky", "1");
    const removed = withDelta(live, delta({ removed: [row] }), 1);
    expect(withDelta(removed, delta({ added: [row] }), 2).removed).toEqual([]);
  });
});

describe("compareMarks", () => {
  const right = source("wp_orders", "status", "2");
  const changed: ValueChange = { ...statusChange, source: right, oldSource: statusSource };
  const only = delta({ changed: [changed], added: [metaRow("5", "_right_only", "a")], removed: [metaRow("6", "_left_only", "b")] });

  it("marks the old value and the rows only the left side has on the left", () => {
    const marks = compareMarks(only, "left");
    expect([...marks.changed.keys()].sort()).toEqual([provenanceKey(statusSource), provenanceKey(source("wp_ordermeta", "meta_key", "6")), provenanceKey(source("wp_ordermeta", "meta_value", "6"))].sort());
  });

  it("marks the new value and the rows only the right side has on the right", () => {
    const marks = compareMarks(only, "right");
    expect([...marks.changed.keys()].sort()).toEqual([provenanceKey(right), provenanceKey(source("wp_ordermeta", "meta_key", "5")), provenanceKey(source("wp_ordermeta", "meta_value", "5"))].sort());
  });

  it("marks without an old value or a flash and draws no removed rows", () => {
    const marks = compareMarks(only, "left");
    expect([...marks.changed.values()].every((c) => c.old === null && c.at === null)).toBe(true);
    expect(marks.added.size).toBe(0);
    expect(marks.removed.size).toBe(0);
    expect(marks.removedRows).toEqual([]);
  });
});

describe("withRemovedRows", () => {
  const group = (id: string): Group => ({ id, table: "t", via: { kind: "own" }, shape: "rows", columns: [], total: 0, rows: [{ key: null, fields: [] }], capped: false, rowsAddress: null, error: null });

  it("returns the same groups when nothing was removed", () => {
    const groups = [group("meta")];
    expect(withRemovedRows(groups, [])).toBe(groups);
  });

  it("appends a removed row to its own group only", () => {
    const row = metaRow("9", "_gone", "1");
    const [items, meta] = withRemovedRows([group("items"), group("meta")], [row]);
    expect(items.rows).toHaveLength(1);
    expect(meta.rows).toHaveLength(2);
    expect(meta.rows[1]).toEqual({ key: row.key, fields: row.fields });
  });

  it("leaves out a removed row whose group is not shown", () => {
    const [items] = withRemovedRows([group("items")], [metaRow("9", "_gone", "1")]);
    expect(items.rows).toHaveLength(1);
  });
});

describe("sinceText", () => {
  it("says since live started for live mode", () => {
    expect(sinceText({ kind: "live" })).toBe("since live started");
  });

  it("names the snapshot and how long ago it was taken", () => {
    const created = 1_000;
    expect(sinceText({ kind: "snapshot", label: "before deploy", created }, (created + 2 * 3600) * 1000)).toBe("since snapshot before deploy (2 h ago)");
  });

  it("says just now for a fresh snapshot", () => {
    expect(sinceText({ kind: "snapshot", label: "x", created: 1_000 }, 1_010 * 1000)).toBe("since snapshot x (just now)");
  });
});
