import { describe, expect, it } from "vitest";
import { cellDisplay, cellText, formatBytes, relativeTime, rowsToJson, rowsToTsv } from "./format";
import type { Cell } from "../api";

describe("formatBytes", () => {
  it("uses binary units", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatBytes(null)).toBe("");
  });
});

describe("cellDisplay", () => {
  it("shows the configured NULL label", () => {
    expect(cellDisplay({ t: "null" }, "∅", 10)).toBe("∅");
  });

  it("flattens newlines and truncates long text", () => {
    expect(cellDisplay({ t: "text", v: "a\nb" }, "NULL", 50)).toBe("a ⏎ b");
    expect(cellDisplay({ t: "text", v: "abcdef" }, "NULL", 3)).toBe("abc…");
  });

  it("describes binary values by hex preview and size", () => {
    expect(cellDisplay({ t: "bytes", v: { len: 3, hexPreview: "00ff10" } }, "NULL", 50)).toBe("0x00ff10 (3 B)");
  });
});

describe("copy formats", () => {
  const rows: Cell[][] = [
    [{ t: "number", v: "1" }, { t: "text", v: "a\tb" }],
    [{ t: "number", v: "2" }, { t: "null" }],
  ];

  it("exports TSV with header and sanitized tabs", () => {
    expect(rowsToTsv(["id", "name"], rows)).toBe("id\tname\n1\ta b\n2\t");
  });

  it("exports JSON with real nulls", () => {
    expect(JSON.parse(rowsToJson(["id", "name"], rows))).toEqual([
      { id: "1", name: "a\tb" },
      { id: "2", name: null },
    ]);
  });

  it("copies NULL as empty text", () => {
    expect(cellText({ t: "null" })).toBe("");
  });
});

describe("relativeTime", () => {
  it("rounds to a readable unit", () => {
    const now = Date.parse("2026-10-15T12:00:00Z");
    expect(relativeTime("2026-10-15T11:59:30Z", now)).toBe("just now");
    expect(relativeTime("2026-10-15T11:00:00Z", now)).toBe("1 h ago");
    expect(relativeTime("2026-10-13T12:00:00Z", now)).toBe("2 d ago");
  });
});
