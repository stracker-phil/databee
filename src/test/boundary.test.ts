import { describe, expect, it } from "vitest";

const sources = import.meta.glob<string>(
  ["/src/**/*.{ts,tsx}", "!/src/**/*.test.*", "!/src/test/**", "!/src/test-setup.ts", "!/src/**/*.gen.ts", "!/src/**/*.d.ts"],
  { query: "?raw", import: "default", eager: true },
);

const IMPORT_SPECIFIER = /(?:\bfrom\s+|\bimport\s*\(\s*|^\s*import\s+)["']([^"']+)["']/gm;

function specifiers(source: string): string[] {
  return [...source.matchAll(IMPORT_SPECIFIER)].map((m) => m[1]);
}

/** Resolves a relative specifier against the importing file; bare specifiers come back unchanged. */
function resolve(from: string, specifier: string): string {
  if (!specifier.startsWith(".")) return specifier;
  const parts = from.split("/").slice(0, -1);
  for (const segment of specifier.split("/")) {
    if (segment === "..") parts.pop();
    else if (segment !== ".") parts.push(segment);
  }
  return parts.join("/");
}

function inside(path: string, folder: string): boolean {
  return path === folder || path.startsWith(`${folder}/`) || path.startsWith(`${folder}.`);
}

const files = Object.entries(sources);

describe("module boundaries", () => {
  it("finds the sources it checks", () => {
    expect(files.length).toBeGreaterThan(50);
    expect(Object.keys(sources)).toContain("/src/api/index.ts");
  });

  it("keeps Tauri imports in src/api/index.ts and src/<module>/api.ts", () => {
    const allowed = (file: string) => file === "/src/api/index.ts" || /^\/src\/[^/]+\/api\.ts$/.test(file);
    const offenders = files
      .filter(([file]) => !allowed(file))
      .filter(([, source]) => specifiers(source).some((s) => s.startsWith("@tauri-apps/api") || s.startsWith("@tauri-apps/plugin-")))
      .map(([file]) => file);
    expect(offenders).toEqual([]);
  });

  it("keeps the core from importing Stage or the WordPress components", () => {
    const isCore = (file: string) =>
      (inside(file, "/src/components") && !inside(file, "/src/components/wp")) ||
      ["/src/state", "/src/commands", "/src/registry", "/src/lib"].some((folder) => inside(file, folder));
    const offenders = files
      .filter(([file]) => isCore(file))
      .flatMap(([file, source]) =>
        specifiers(source)
          .map((s) => resolve(file, s))
          .filter((target) => inside(target, "/src/stage") || inside(target, "/src/components/wp"))
          .map((target) => `${file} -> ${target}`),
      );
    expect(offenders).toEqual([]);
  });
});
