/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { contrastRatio, hexToRgb, hexTokens, over, textMixAlpha } from "./contrast";

const stylesDir = resolve(process.cwd(), "src", "styles");
const schemesDir = join(stylesDir, "schemes");

interface Scheme {
  id: string;
  tokens: Record<string, string>;
  o2: number;
  o3: number;
}

const schemes = readdirSync(schemesDir)
  .filter((f) => f.endsWith(".css"))
  .sort()
  .map((f): Scheme => {
    const css = readFileSync(join(schemesDir, f), "utf8");
    return { id: f.replace(/\.css$/, ""), tokens: hexTokens(css), o2: textMixAlpha(css, "--o2"), o3: textMixAlpha(css, "--o3") };
  });

const BODY = 4.5;
const META = 3;

type Level = "text" | "o2" | "o3";

function ratio(scheme: Scheme, surface: string, level: Level): number {
  const bg = hexToRgb(scheme.tokens[surface]);
  const alpha = level === "text" ? 1 : scheme[level];
  return contrastRatio(over(hexToRgb(scheme.tokens["--text"]), alpha, bg), bg);
}

function failures(level: Level, min: number): string[] {
  return schemes.flatMap((scheme) =>
    ["--bg", "--chrome", "--pop"]
      .filter((surface) => scheme.tokens[surface])
      .map((surface) => ({ surface, r: ratio(scheme, surface, level) }))
      .filter(({ r }) => r < min)
      .map(({ surface, r }) => `${scheme.id}: --${level} on ${surface} is ${r.toFixed(2)}:1, needs ${min}:1`),
  );
}

describe("contrast helpers", () => {
  it("rates black on white 21:1 and a color on itself 1:1", () => {
    expect(contrastRatio(hexToRgb("#000000"), hexToRgb("#ffffff"))).toBeCloseTo(21, 5);
    expect(contrastRatio(hexToRgb("#336699"), hexToRgb("#336699"))).toBeCloseTo(1, 5);
  });

  it("blends a half-opaque color halfway toward the background", () => {
    expect(over(hexToRgb("#ffffff"), 0.5, hexToRgb("#000000"))).toEqual([0.5, 0.5, 0.5]);
  });

  it("reads a text level's share from a scheme's own --o2-mix and --o3-mix", () => {
    const css = "/* --o2-mix: 10%; */\n:root {\n  --o2-mix: 68%;\n  --o3-mix: 52.5%;\n}";
    expect(textMixAlpha(css, "--o2")).toBeCloseTo(0.68, 5);
    expect(textMixAlpha(css, "--o3")).toBeCloseTo(0.525, 5);
  });

  it("rejects a scheme that declares no mix for a text level", () => {
    expect(() => textMixAlpha(":root { --o2-mix: 68%; }", "--o3")).toThrow("--o3-mix");
  });
});

describe("scheme contrast", () => {
  it("covers every scheme file with its surface and text tokens", () => {
    expect(schemes.length).toBeGreaterThan(1);
    for (const { tokens } of schemes) {
      expect(Object.keys(tokens)).toEqual(expect.arrayContaining(["--text", "--bg", "--chrome"]));
    }
  });

  it("keeps --o2-mix above --o3-mix in every scheme", () => {
    const flat = schemes.filter(({ o2, o3 }) => o2 <= o3).map(({ id, o2, o3 }) => `${id}: --o2-mix ${o2 * 100}% is not above --o3-mix ${o3 * 100}%`);
    expect(flat).toEqual([]);
  });

  it("keeps body text readable on every surface it sits on", () => {
    expect(failures("text", BODY)).toEqual([]);
  });

  it("keeps secondary text (--o2) readable on every surface it sits on", () => {
    expect(failures("o2", META)).toEqual([]);
  });

  it("keeps meta text (--o3) readable on every surface it sits on", () => {
    expect(failures("o3", META)).toEqual([]);
  });
});
