import type { Coverage } from "../api";

const SAMPLE_SHOWN = 3;

interface CoverageProblem {
  /** `5 of 40 point nowhere (98, 99)`. */
  text: string;
  /** The unresolved rows as a table filter; `null` without one. */
  filter: string | null;
}

/** Source rows that hold a value (not 0 or NULL) but find no target. */
export const unresolvedCount = (c: Coverage) => c.total - c.resolved - c.zeroOrNull;

/** Values that point nowhere. 0 or NULL are "no reference" (WordPress stores 0 for none) and never a problem. */
export function coverageProblem(coverage: Coverage): CoverageProblem | null {
  const unresolved = unresolvedCount(coverage);
  if (unresolved <= 0) return null;
  const sample = coverage.sampleUnresolved.slice(0, SAMPLE_SHOWN);
  const text = `${unresolved} of ${coverage.total} point nowhere${sample.length ? ` (${sample.join(", ")})` : ""}`;
  return { text, filter: coverage.rows?.unresolved ?? null };
}
