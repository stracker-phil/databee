import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { settings } from "../test/settings";
import { applyAppearance, useResolvedScheme } from "./theme";

const root = document.documentElement;

describe("useResolvedScheme", () => {
  const resolved = (s: Parameters<typeof useResolvedScheme>[0]) => renderHook(() => useResolvedScheme(s)).result.current;

  it.each([
    ["dark", "ayu-mirage", true],
    ["light", "ayu-light", false],
    ["system", "ayu-light", false],
  ] as const)("picks the scheme for theme %s with a light system", (theme, id, dark) => {
    expect(resolved(settings({ theme }))).toEqual({ id, dark });
  });

  it("uses the chosen scheme of the active side", () => {
    expect(resolved(settings({ theme: "light", lightScheme: "ayu-mirage" }))).toEqual({ id: "ayu-mirage", dark: true });
  });

  it("falls back to the default scheme before settings load", () => {
    expect(resolved(null)).toEqual({ id: "ayu-light", dark: false });
  });
});

describe("applyAppearance", () => {
  beforeEach(() => {
    delete root.dataset.scheme;
    delete root.dataset.accent;
    root.removeAttribute("style");
  });

  it.each([
    ["dark", false, "ayu-mirage"],
    ["light", true, "ayu-light"],
    ["system", true, "ayu-mirage"],
    ["system", false, "ayu-light"],
  ] as const)("sets the scheme for theme %s with system dark %s", (theme, systemDark, scheme) => {
    applyAppearance(settings({ theme }), systemDark);
    expect(root.dataset.scheme).toBe(scheme);
  });

  it("sets the accent", () => {
    applyAppearance(settings({ accent: "gold" }), false);
    expect(root.dataset.accent).toBe("gold");
  });

  it.each([
    [1, "1px"],
    [2, "3px"],
    [3, "5px"],
    [4, "8px"],
  ])("maps row padding %i to --pad %s", (rowPadding, pad) => {
    applyAppearance(settings({ rowPadding }), false);
    expect(root.style.getPropertyValue("--pad")).toBe(pad);
  });

  it("sets the UI and data sizes independently", () => {
    applyAppearance(settings({ uiFontSize: 15, dataFontSize: 11 }), false);
    expect(root.style.getPropertyValue("--ui-font-size")).toBe("15px");
    expect(root.style.getPropertyValue("--data-font-size")).toBe("11px");
  });

  it("puts a custom mono font in front of the bundled one", () => {
    applyAppearance(settings({ fontMono: "Fira Code" }), false);
    expect(root.style.getPropertyValue("--font-mono")).toContain('"Fira Code"');
  });

  it("removes the font override when the font is cleared", () => {
    applyAppearance(settings({ fontMono: "Fira Code", fontUi: "Inter" }), false);
    applyAppearance(settings({ fontMono: null, fontUi: null }), false);
    expect(root.style.getPropertyValue("--font-mono")).toBe("");
    expect(root.style.getPropertyValue("--font-ui")).toBe("");
  });
});
