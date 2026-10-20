import { useEffect } from "react";
import { api, toAppError, type Coverage, type ResolvedReference } from "../../api";
import { projectKey, type CoverageState } from "../../state/slices/project";
import { unresolvedCount } from "../../lib/coverageProblems";
import { useStore } from "../../state/store";

export type { CoverageState };

const SAMPLE_SHOWN = 3;
/** References counted at once; each count is one or two read-only queries on the metadata pool. */
const COVERAGE_CONCURRENCY = 3;

/** Nothing that holds a value finds a target: the rule is probably wrong. */
export const nothingResolves = (c: Coverage) => !c.timedOut && c.resolved === 0 && unresolvedCount(c) > 0;

export const coverageKey = (reference: ResolvedReference) => JSON.stringify([reference.from, reference.to, reference.by]);

function fromCoverageKey(key: string): ResolvedReference {
  const [from, to, by] = JSON.parse(key) as [ResolvedReference["from"], ResolvedReference["to"], ResolvedReference["by"]];
  return { from, to, by };
}

/** `1 is 0 or NULL · unresolved 5, 7`: what the counts leave out. */
function coverageDetail(coverage: Coverage): string {
  const zeros = coverage.zeroOrNull;
  const sample = coverage.sampleUnresolved.slice(0, SAMPLE_SHOWN);
  const parts: string[] = [];
  if (zeros > 0) parts.push(`${zeros} ${zeros === 1 ? "is" : "are"} 0 or NULL`);
  if (sample.length > 0) parts.push(`unresolved ${sample.join(", ")}`);
  if (coverage.timedOut) parts.push("sample timed out");
  return parts.join(" · ");
}

export function CoverageCounts({ coverage, suffix = "" }: { coverage: Coverage; suffix?: string }) {
  if (coverage.timedOut && coverage.total === 0) return <span className="o3">timed out</span>;
  const detail = coverageDetail(coverage);
  return (
    <span className="coverage">
      <span>
        {coverage.resolved} of {coverage.total}
        {suffix}
      </span>
      {detail && <span className="o3"> · {detail}</span>}
    </span>
  );
}

const percent = (part: number, total: number) => `${total === 0 ? 0 : (part / total) * 100}%`;

/** `resolved of total` with a bar: resolved, then 0 or NULL, then what points nowhere (empty). */
function CoverageBar({ coverage }: { coverage: Coverage }) {
  if (coverage.timedOut && coverage.total === 0) return <span className="o3">timed out</span>;
  const detail = coverageDetail(coverage);
  return (
    <span className="coverage-cell" title={detail || undefined}>
      <span className="coverage-bar" aria-hidden="true">
        <span className="resolved" style={{ width: percent(coverage.resolved, coverage.total) }} />
        <span className="zero" style={{ width: percent(coverage.zeroOrNull, coverage.total) }} />
      </span>
      <span>
        {coverage.resolved} of {coverage.total}
      </span>
    </span>
  );
}

export function CoverageCell({ state }: { state: CoverageState | undefined }) {
  if (!state || state.status === "running") return <span className="o3">counting…</span>;
  if (state.status === "error") return <span className="o3" title={state.message}>{state.message}</span>;
  return <CoverageBar coverage={state.coverage} />;
}

/**
 * Counts every listed reference the cache lacks, `COVERAGE_CONCURRENCY` at a time. Leaving the page stops
 * the queue and drops the counts still running; a refresh (`refreshCoverage`) empties the cache and counts again.
 */
export function useAutoCoverage(sessionId: string, database: string, references: ResolvedReference[]) {
  const key = projectKey(sessionId, database);
  const cache = useStore((s) => s.coverage[key]);
  const generation = cache?.generation ?? 0;
  const listed = references.map(coverageKey).join("\n");

  useEffect(() => {
    const { setCoverage, coverage } = useStore.getState();
    const known = coverage[key]?.results ?? {};
    const queue = listed
      .split("\n")
      .filter((k) => k && !known[k])
      .map(fromCoverageKey);
    const running = new Set<string>();
    let cancelled = false;
    const work = async () => {
      for (let next = queue.shift(); next && !cancelled; next = queue.shift()) {
        const k = coverageKey(next);
        running.add(k);
        setCoverage(key, k, { status: "running" });
        let state: CoverageState;
        try {
          state = { status: "done", coverage: await api.referenceCoverage(sessionId, database, next) };
        } catch (e) {
          state = { status: "error", message: toAppError(e).message };
        }
        running.delete(k);
        if (!cancelled) setCoverage(key, k, state);
      }
    };
    for (let i = 0; i < COVERAGE_CONCURRENCY; i++) void work();
    return () => {
      cancelled = true;
      for (const k of running) setCoverage(key, k, null);
    };
  }, [key, sessionId, database, listed, generation]);

  return cache?.results ?? {};
}
