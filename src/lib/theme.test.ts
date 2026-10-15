import { describe, expect, it } from "vitest";
import { settings } from "../test/settings";
import { applyAppearance, rowHeightFor } from "./theme";

describe("applyAppearance", () => {
  it("sets the UI and data sizes independently", () => {
    applyAppearance(settings({ uiFontSize: 15, dataFontSize: 11 }));
    const style = document.documentElement.style;
    expect(style.getPropertyValue("--ui-font-size")).toBe("15px");
    expect(style.getPropertyValue("--data-font-size")).toBe("11px");
  });

  it("derives the grid row height from the data size and density", () => {
    expect(rowHeightFor({ dataFontSize: 12, density: "comfortable" })).toBe(26);
    expect(rowHeightFor({ dataFontSize: 12, density: "compact" })).toBe(21);
    expect(rowHeightFor({ dataFontSize: 18, density: "comfortable" })).toBeGreaterThan(rowHeightFor({ dataFontSize: 12, density: "comfortable" }));
    applyAppearance(settings({ dataFontSize: 16, density: "compact" }));
    expect(document.documentElement.style.getPropertyValue("--row-height")).toBe("28px");
  });
});
