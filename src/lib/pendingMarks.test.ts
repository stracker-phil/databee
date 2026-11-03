import { describe, expect, it } from "vitest";
import type { PlannedRow } from "../api";
import { columnMeta, pendingChange } from "../test/fixtures";
import { pendingRowMarks } from "./pendingMarks";

const columns = [columnMeta("ID", "BIGINT UNSIGNED"), columnMeta("post_title", "TEXT")];
const keys = [[{ column: "ID", value: "1" }], [{ column: "ID", value: "2" }]];
const grid = { sessionId: "s1", database: "db", table: "wp_posts", keys, columns };

const row = (effect: PlannedRow["effect"], patch: Partial<PlannedRow> = {}): PlannedRow => ({
  database: "db",
  table: "wp_posts",
  keyColumn: "ID",
  key: "1",
  effect,
  ...patch,
});

describe("pendingRowMarks", () => {
  it("marks a row a pending plan deletes", () => {
    const marks = pendingRowMarks([pendingChange({ description: "Delete row 1", rows: [row({ kind: "delete" })] })], grid);
    expect(marks).toEqual({ 0: { deleted: true, duplicated: false, columns: new Set(), title: "Delete row 1" } });
  });

  it("marks a row a pending plan duplicates", () => {
    const marks = pendingRowMarks([pendingChange({ rows: [row({ kind: "duplicate" }, { key: "2" })] })], grid);
    expect(marks[1]).toMatchObject({ deleted: false, duplicated: true });
    expect(marks[0]).toBeUndefined();
  });

  it.each(["post_title", "POST_TITLE"])("marks the column a set plan names as %s", (column) => {
    const marks = pendingRowMarks([pendingChange({ rows: [row({ kind: "set", column })] })], grid);
    expect([...marks[0].columns]).toEqual([1]);
    expect(marks[0]).toMatchObject({ deleted: false, duplicated: false });
  });

  it("matches the key column case-insensitively", () => {
    const marks = pendingRowMarks([pendingChange({ rows: [row({ kind: "delete" }, { keyColumn: "id" })] })], grid);
    expect(marks[0]?.deleted).toBe(true);
  });

  it("leaves a set on an unknown column without a marked cell", () => {
    const marks = pendingRowMarks([pendingChange({ rows: [row({ kind: "set", column: "missing" })] })], grid);
    expect(marks[0].columns.size).toBe(0);
  });

  it.each([
    ["another session", pendingChange({ sessionId: "s2", rows: [row({ kind: "delete" })] })],
    ["another table", pendingChange({ rows: [row({ kind: "delete" }, { table: "wp_users" })] })],
    ["another database", pendingChange({ rows: [row({ kind: "delete" }, { database: "other" })] })],
  ])("ignores plans of %s", (_name, change) => {
    expect(pendingRowMarks([change], grid)).toEqual({});
  });

  it("leaves rows with composite or missing keys unmarked", () => {
    const marks = pendingRowMarks([pendingChange({ rows: [row({ kind: "delete" })] })], {
      ...grid,
      keys: [
        [
          { column: "ID", value: "1" },
          { column: "post_title", value: "a" },
        ],
        null,
        undefined,
      ],
    });
    expect(marks).toEqual({});
  });

  it("combines several plans on one row and joins their descriptions", () => {
    const marks = pendingRowMarks(
      [
        pendingChange({ id: 1, description: "Set post_title", rows: [row({ kind: "set", column: "post_title" })] }),
        pendingChange({ id: 2, description: "Duplicate row 1", rows: [row({ kind: "duplicate" })] }),
        pendingChange({ id: 3, description: "Delete row 1", rows: [row({ kind: "delete" })] }),
      ],
      grid,
    );
    expect(marks[0]).toEqual({ deleted: true, duplicated: true, columns: new Set([1]), title: "Set post_title\nDuplicate row 1\nDelete row 1" });
  });
});
