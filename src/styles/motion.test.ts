/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

// Read from disk: Vitest blanks every CSS module unless CSS processing is on.
const srcDir = resolve(process.cwd(), "src");
const motionCss = join(srcDir, "styles", "motion.css");

const scanned: [string, string][] = readdirSync(srcDir, { recursive: true, encoding: "utf8" })
  .filter((rel: string) => /\.(css|ts|tsx)$/.test(rel))
  .map((rel: string) => join(srcDir, rel))
  .filter((path) => path !== motionCss && !/\.test\.tsx?$/.test(path) && !path.endsWith("types.gen.ts"))
  .map((path): [string, string] => [path, readFileSync(path, "utf8")]);

const stripComments = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, "");

// A motion property (CSS `transition:` or a JS `transition: "…"` / `animationDuration: "…"`) up to its end.
const DECLARATION = /\b(?:transition|animation)(?:-duration|-delay|Duration|Delay)?["']?\s*:\s*[^;}\n]*/gi;
const LITERAL_DURATION = /(?<![\w-])\d*\.?\d+m?s\b/;

function literalDurations(source: string): string[] {
  return (stripComments(source).match(DECLARATION) ?? []).filter((d) => LITERAL_DURATION.test(d.replace(/var\([^)]*\)/g, ""))).map((d) => d.trim());
}

describe("motion durations", () => {
  it("come from the tokens in motion.css, never a literal", () => {
    const offenders = scanned.flatMap(([path, source]) => literalDurations(source).map((d) => `${path}: ${d}`));
    expect(offenders).toEqual([]);
  });

  it("scan sees the sources and flags literals only", () => {
    expect(scanned.filter(([path]) => path.endsWith(".css")).length).toBeGreaterThan(1);
    expect(scanned.length).toBeGreaterThan(10);
    expect(literalDurations("a { transition: opacity .12s ease; }")).toHaveLength(1);
    expect(literalDurations("a { transition-duration: 150ms; }")).toHaveLength(1);
    expect(literalDurations("a { animation: pulse 1.6s infinite; }")).toHaveLength(1);
    expect(literalDurations('const s = { transition: "opacity 120ms" };')).toHaveLength(1);
    expect(literalDurations("a { transition: opacity var(--motion-fast) var(--ease-out); }")).toEqual([]);
    expect(literalDurations("a { animation: spin var(--motion-spin) linear infinite; }")).toEqual([]);
    expect(literalDurations("a { transition: none; }")).toEqual([]);
  });
});

describe("motion.css", () => {
  const css = readFileSync(motionCss, "utf8");

  it.each(["--motion-fast", "--motion-base", "--motion-slow", "--ease-out", "--ease-in-out"])("defines %s", (token) => {
    expect(stripComments(css)).toMatch(new RegExp(`^\\s*${token}\\s*:`, "m"));
  });

  it("has a prefers-reduced-motion block", () => {
    expect(stripComments(css)).toMatch(/@media\s*\(prefers-reduced-motion:\s*reduce\)/);
  });
});
