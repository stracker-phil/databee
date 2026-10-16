import { describe, expect, it } from "vitest";
import { relativeTime } from "./format";
import { flagEmoji, initials, shorten } from "./wpFormat";

describe("wpFormat", () => {
  const now = Date.parse("2026-10-15T12:00:00Z");
  const t = now / 1000;

  it("describes past and future times relatively", () => {
    expect(relativeTime(t - 10, now)).toBe("just now");
    expect(relativeTime(t - 2 * 3600, now)).toBe("2 h ago");
    expect(relativeTime(t + 3 * 86400, now)).toBe("in 3 d");
    expect(relativeTime(t - 400 * 86400, now)).toBe("1.1 y ago");
  });

  it("turns country codes into flags", () => {
    expect(flagEmoji("AT")).toBe("🇦🇹");
    expect(flagEmoji("austria")).toBe("");
  });

  it("builds initials from names and emails", () => {
    expect(initials("Jane Doe")).toBe("JD");
    expect(initials("jay.dee@shop.test")).toBe("JT");
    expect(initials("admin")).toBe("A");
  });

  it("shortens long tokens keeping both ends", () => {
    expect(shorten("short")).toBe("short");
    expect(shorten("0cAFcWeA6zUQyhl2k98qbng9BHiGwe5-BDEyC-gNYwZ20Afg", 6, 3)).toBe("0cAFcW…Afg");
  });
});
