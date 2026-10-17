import type { Coverage } from "../../api";

export type CoverageState = { status: "running" } | { status: "done"; coverage: Coverage } | { status: "error"; message: string };

const SAMPLE_SHOWN = 3;

/** Source rows that hold a value (not 0 or NULL) but find no target. */
export const unresolved = (c: Coverage) => c.total - c.resolved - c.zeroOrNull;

/** Nothing that holds a value finds a target: the rule is probably wrong. */
export const nothingResolves = (c: Coverage) => !c.timedOut && c.resolved === 0 && unresolved(c) > 0;

export function CoverageCounts({ coverage, suffix = "" }: { coverage: Coverage; suffix?: string }) {
  if (coverage.timedOut && coverage.total === 0) return <span className="o3">timed out</span>;
  const zeros = coverage.zeroOrNull;
  const sample = coverage.sampleUnresolved.slice(0, SAMPLE_SHOWN);
  return (
    <span className="coverage">
      <span>
        {coverage.resolved} of {coverage.total}
        {suffix}
      </span>
      {zeros > 0 && (
        <span className="o3">
          {" "}
          · {zeros} {zeros === 1 ? "is" : "are"} 0 or NULL
        </span>
      )}
      {sample.length > 0 && <span className="o3"> · unresolved {sample.join(", ")}</span>}
      {coverage.timedOut && <span className="o3"> · sample timed out</span>}
    </span>
  );
}

export function CoverageCell({ state }: { state: CoverageState | undefined }) {
  if (!state) return <span className="o3">not checked</span>;
  if (state.status === "running") return <span className="o3">running…</span>;
  if (state.status === "error") return <span className="o3" title={state.message}>{state.message}</span>;
  return <CoverageCounts coverage={state.coverage} />;
}
