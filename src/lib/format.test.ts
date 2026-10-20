import { describe, expect, it } from "vitest";
import { cellDisplay, cellText, charWidth, formatBytes, isoUtc, relativeTime, rowsToJson, rowsToTsv } from "./format";
import type { Cell } from "../api";

describe("charWidth", () => {
  it.each([
    ["the name when no text is longer", "id", ["a"], undefined, "4ch"],
    ["the longest text", "id", ["a", "x".repeat(10)], undefined, "12ch"],
    ["40 characters at most by default", "id", ["x".repeat(100)], undefined, "42ch"],
    ["the given maximum", "id", ["x".repeat(100)], 10, "12ch"],
  ])("sizes to %s", (_name, name, texts, max, expected) => {
    expect(charWidth(name, texts, max)).toBe(expected);
  });

  it("adds the lead to the width of the longest text but not to the name", () => {
    expect(charWidth("ID", ["123"])).toBe("5ch");
    expect(charWidth("ID", ["123"], 40, 2.5)).toBe("7.5ch");
    expect(charWidth("a_long_name", ["1"], 40, 2.5)).toBe("13ch");
  });
});

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

describe("isoUtc", () => {
  it.each([
    [4102444800, "2100-01-01 00:00:00 UTC"],
    [1000, "1970-01-01 00:16:40 UTC"],
  ])("formats %d seconds as a UTC date and time", (seconds, expected) => {
    expect(isoUtc(seconds)).toBe(expected);
  });
});

describe("relativeTime", () => {
  it("rounds to a readable unit", () => {
    const now = Date.parse("2026-10-15T12:00:00Z");
    expect(relativeTime("2026-10-15T11:59:30Z", now)).toBe("just now");
    expect(relativeTime("2026-10-15T11:00:00Z", now)).toBe("1 h ago");
    expect(relativeTime("2026-10-13T12:00:00Z", now)).toBe("2 d ago");
  });

  describe("for a time in the future", () => {
    const now = Date.parse("2026-10-15T12:00:00Z");

    it("reads 'in …' for an ISO string", () => {
      expect(relativeTime("2026-10-15T14:00:00Z", now)).toBe("in 2 h");
      expect(relativeTime("2026-10-18T12:00:00Z", now)).toBe("in 3 d");
    });

    it("reads 'in …' for unix seconds", () => {
      expect(relativeTime(now / 1000 + 3 * 3600, now)).toBe("in 3 h");
      expect(relativeTime(now / 1000 + 10 * 60, now)).toBe("in 10 min");
    });

    it("reads 'just now' when the time is seconds away", () => {
      expect(relativeTime(now / 1000 + 20, now)).toBe("just now");
    });
  });

  it("reads the same past time alike for an ISO string and for unix seconds", () => {
    const now = Date.parse("2026-10-15T12:00:00Z");
    expect(relativeTime((now - 2 * 86400_000) / 1000, now)).toBe(relativeTime("2026-10-13T12:00:00Z", now));
  });
});
