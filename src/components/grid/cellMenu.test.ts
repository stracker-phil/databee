import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Cell } from "../../api";
import { rowsToJson, rowsToTsv } from "../../lib/format";
import { num, text, value } from "../../test/fixtures";
import type { MenuEntry } from "../ContextMenu";
import { cellMenu, type CellMenuContext } from "./cellMenu";

const bytes: Cell = { t: "bytes", v: { len: 4, hexPreview: "0x00" } };
const source = { table: "wp_postmeta", column: "meta_value", row: [{ column: "meta_id", value: "1" }] };

const names = ["id", "name"];
const rows: Cell[][] = [
  [num("1"), text("a")],
  [num("2"), bytes],
];

const copy = vi.fn();
const copyValue = vi.fn();
const start = vi.fn();
const setNull = vi.fn();

function menu(patch: Partial<CellMenuContext> = {}): MenuEntry[] {
  return cellMenu({ row: 0, col: 1, rows, names, value: value(), copy, copyValue, ...patch });
}

const labels = (entries: MenuEntry[]) => entries.map((e) => (e === "separator" ? "separator" : e.label));

function entry(entries: MenuEntry[], label: string) {
  const found = entries.find((e) => e !== "separator" && e.label === label);
  if (!found || found === "separator") throw new Error(`no entry ${label}`);
  return found;
}

describe("cellMenu", () => {
  beforeEach(() => vi.clearAllMocks());

  it("offers copying the value, the row and all rows", () => {
    expect(labels(menu())).toEqual([
      "Copy value",
      "Copy row as TSV",
      "Copy row as JSON",
      "separator",
      "Copy all 2 rows as TSV",
      "Copy all 2 rows as JSON",
    ]);
  });

  it("copies the value through copyValue", () => {
    entry(menu(), "Copy value").action?.();
    expect(copyValue).toHaveBeenCalled();
  });

  describe("editing", () => {
    it("omits the edit entries without edit", () => {
      const l = labels(menu());
      expect(l).not.toContain("Edit value");
      expect(l).not.toContain("Set to NULL");
    });

    it("adds enabled edit entries that call edit.start and edit.setNull", () => {
      const entries = menu({ edit: { start, setNull } });
      const edit = entry(entries, "Edit value");
      expect(edit.disabled).toBeFalsy();
      edit.action?.();
      expect(start).toHaveBeenCalled();
      entry(entries, "Set to NULL").action?.();
      expect(setNull).toHaveBeenCalled();
    });

    it("disables Edit value for a bytes cell", () => {
      const entries = menu({ row: 1, col: 1, edit: { start, setNull } });
      expect(entry(entries, "Edit value").disabled).toBe(true);
      expect(labels(entries)).toContain("Set to NULL");
    });
  });

  describe("rule entries", () => {
    const ruleLabels = ["Add reference…", "Kind…", "Style…"];

    it("appear after Watch on readout, wrapped in separators, when the value has a source", () => {
      const l = labels(menu({ value: value({ source }) }));
      expect(l.slice(1, 7)).toEqual(["separator", "Watch on readout", ...ruleLabels, "separator"]);
    });

    it.each([
      ["has no source", { row: 0, col: 1, value: value() }],
      ["is undefined", { row: 0, col: 1, value: undefined }],
      ["is a bytes cell", { row: 1, col: 1, value: value({ source }) }],
    ])("are absent when the value %s", (_, patch) => {
      const l = labels(menu(patch));
      for (const label of ruleLabels) expect(l).not.toContain(label);
    });

    it("leave a sourced bytes cell with Watch on readout only", () => {
      const l = labels(menu({ row: 1, col: 1, value: value({ source }) }));
      expect(l.slice(1, 4)).toEqual(["separator", "Watch on readout", "separator"]);
    });

    it.each([
      ["has no source", value()],
      ["is undefined", undefined],
    ])("omit Watch on readout when the value %s", (_, v) => {
      expect(labels(menu({ value: v }))).not.toContain("Watch on readout");
    });
  });

  describe("copying rows", () => {
    it("copies the row as TSV", () => {
      entry(menu(), "Copy row as TSV").action?.();
      expect(copy).toHaveBeenCalledWith(rowsToTsv(names, [rows[0]]), "row");
    });

    it("copies the row as JSON", () => {
      entry(menu(), "Copy row as JSON").action?.();
      expect(copy).toHaveBeenCalledWith(rowsToJson(names, [rows[0]]), "row as JSON");
    });

    it("copies all rows as TSV", () => {
      entry(menu(), "Copy all 2 rows as TSV").action?.();
      expect(copy).toHaveBeenCalledWith(rowsToTsv(names, rows), "2 rows");
    });

    it("copies all rows as JSON", () => {
      entry(menu(), "Copy all 2 rows as JSON").action?.();
      expect(copy).toHaveBeenCalledWith(rowsToJson(names, rows), "2 rows as JSON");
    });
  });

  describe("row actions", () => {
    it("are appended after a separator", () => {
      const rowActions: MenuEntry[] = [{ label: "Delete row", action: vi.fn() }];
      const l = labels(menu({ rowActions }));
      expect(l.slice(-3)).toEqual(["Copy all 2 rows as JSON", "separator", "Delete row"]);
    });

    it("leave the menu ending with the copy-all JSON entry when absent", () => {
      expect(labels(menu()).at(-1)).toBe("Copy all 2 rows as JSON");
    });
  });
});
