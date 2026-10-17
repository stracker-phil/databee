import { useEffect, useMemo, useRef, useState } from "react";
import type { EditorView } from "@codemirror/view";
import { CheckCircle2, Copy, Loader2, Pin, Play, PlayCircle, Save, Square } from "lucide-react";
import { api, toAppError, type Provenance, type ResultSet } from "../api";
import { EVENTS, useBindingLabel } from "../commands";
import { formatCount } from "../lib/format";
import { pinLabel } from "../lib/pins";
import { useApiResult } from "../lib/useApiResult";
import { resultValueAt } from "../lib/values";
import { useResolvedScheme } from "../lib/theme";
import { isPreset, savedOriginText } from "../state/slices/saved";
import { fail, queryMarksKey, useStore, type QueryTab as QueryTabState } from "../state/store";
import type { MenuEntry } from "./ContextMenu";
import { ErrorPanel } from "./ErrorPanel";
import { ChangeList } from "./object/ChangeList";
import { currentStatement, QueryEditor } from "./QueryEditor";
import { ResultsGrid } from "./ResultsGrid";
import { Splitter, usePaneSize, useWindowSize } from "./Splitter";
import { ValueCounts, type ValueCountsTarget } from "./ValueCounts";

const NO_SCHEMA: Record<string, string[]> = {};

/**
 * Columns per table for completion; reloads when the table list changes (DDL reloads it). Completion is a
 * convenience: a failure keeps the previous schema and is only logged, never toasted.
 */
function useCompletionSchema(sessionId: string, database: string | null) {
  const tables = useStore((s) => (database ? s.schema[sessionId]?.tables[database] : undefined));
  const tablesKey = useMemo(() => JSON.stringify(tables ?? null), [tables]);
  const key = database ? JSON.stringify([sessionId, database, tablesKey]) : null;
  const { data } = useApiResult(key, () => api.completionSchema(sessionId, database ?? ""), {
    keepData: true,
    onError: (e) => void api.logError(`Completion schema for ${database}: ${e.message}`).catch(() => {}),
  });
  return data ?? NO_SCHEMA;
}

function RunHint() {
  const statement = useBindingLabel("editor.run");
  const all = useBindingLabel("editor.runAll");
  const parts = [statement && `${statement} statement`, all && `${all} all`].filter(Boolean);
  if (parts.length === 0) return null;
  return <span className="kbd">{parts.join(" · ")}</span>;
}

function ResultsPane({ tab }: { tab: QueryTabState }) {
  // A new run or a whole-run error replaces the failure of the previous run.
  const failure = tab.running || tab.error ? null : tab.response?.failure;
  if (!failure) return <ResultView tab={tab} />;
  return (
    <>
      <ErrorPanel title={`Statement ${failure.index + 1} failed`} message={failure.error.message} code={failure.error.code} />
      <ResultView tab={tab} />
    </>
  );
}

/** Value counts of a result column read from a table; a pick narrows the tab's SQL and runs it. */
function useValueCounts(tab: QueryTabState, result: ResultSet | undefined) {
  const [counts, setCounts] = useState<ValueCountsTarget | null>(null);
  const database = tab.response?.database ?? null;

  const columnMenu = async (col: number, at: { x: number; y: number }): Promise<MenuEntry[]> => {
    const origin = result?.columns[col]?.origin;
    const originDatabase = origin?.database ?? database;
    if (!origin || !originDatabase) return [{ label: "Value counts", disabled: true, title: "This column is not read from a table", action: () => {} }];
    const refusal = await api.narrowRefusal(tab.sql).catch((e) => toAppError(e).message);
    const target = { sessionId: tab.sessionId, database: originDatabase, table: origin.table, column: origin.column, equals: [], ...at };
    return [{ label: "Value counts", disabled: !!refusal, title: refusal ?? undefined, action: () => setCounts(target) }];
  };

  const narrow = async (column: string, value: string | null) => {
    try {
      const sql = await api.narrowQuery(tab.sessionId, tab.sql, column, value);
      useStore.getState().updateQueryTab(tab.id, { sql });
      void useStore.getState().runQuery(tab.id, sql);
    } catch (e) {
      fail(e, "Could not filter the query");
    }
  };

  const popover = counts && <ValueCounts target={counts} onPick={(value) => void narrow(counts.column, value)} onClose={() => setCounts(null)} />;
  return { columnMenu, popover };
}

function ResultView({ tab }: { tab: QueryTabState }) {
  const result = tab.response?.results[tab.activeResult];
  const { columnMenu, popover } = useValueCounts(tab, result);
  if (tab.running) {
    return (
      <div className="empty-state">
        <span><Loader2 size="1.231rem" className="spin" style={{ verticalAlign: -3 }} /> Running…</span>
      </div>
    );
  }
  if (tab.error) return <ErrorPanel message={tab.error.message} code={tab.error.code} />;
  if (!result) return <div className="empty-state">Run a query to see results here.</div>;
  if (result.returnsRows) {
    return (
      <>
        <ResultsGrid
          columns={result.columns}
          rows={result.rows}
          source={{ sessionId: tab.sessionId, resultId: result.id }}
          valueAt={(row, col) => resultValueAt(result, row, col)}
          columnMenu={columnMenu}
        />
        {popover}
      </>
    );
  }
  return (
    <div className="message-panel">
      <CheckCircle2 size="1.538rem" style={{ color: "var(--style-green)" }} />
      <div className="big">{formatCount(result.affectedRows)} {result.affectedRows === 1 ? "row" : "rows"} affected</div>
      {result.lastInsertId != null && <div className="muted">Last insert ID: {result.lastInsertId}</div>}
    </div>
  );
}

/** A changed value's row, opened as an object; only a single-column key makes an address. */
function openSource(tab: QueryTabState, source: Provenance) {
  const s = useStore.getState();
  const connection = s.sessions.find((x) => x.id === tab.sessionId)?.slug;
  const [part, ...rest] = source.row;
  if (!connection || !tab.saved || !part || rest.length > 0) return;
  s.openObject({ connection, database: tab.saved.database, target: { kind: "row", table: source.table, key: part.value }, view: null, compare: null });
}

/** What changed in a saved query's rows since a snapshot ("Changes since snapshot…"). */
function QueryChanges({ tab }: { tab: QueryTabState }) {
  const marks = useStore((s) => s.changeMarks[queryMarksKey(tab.sessionId, tab.id)]);
  if (!marks) return null;
  return (
    <div className="object query-changes">
      <ChangeList marks={marks} dismissCommand="query.dismissChanges" onJump={(source) => openSource(tab, source)} />
    </div>
  );
}

/** Save and Pin, or for a preset query its origin and "Copy to project". */
function SavedActions({ tab }: { tab: QueryTabState }) {
  const { saveQueryTab, pinQueryTab, copyPresetQuery } = useStore.getState();
  const saveKeys = useBindingLabel("query.save");
  const pinKeys = useBindingLabel("query.pin");
  const origin = tab.saved?.origin;
  return (
    <>
      {origin && <span className="o2 small mono" data-stage="query:origin">{savedOriginText(origin)}</span>}
      {origin && isPreset(origin) && (
        <button data-stage="query:copy-to-project" className="btn" onClick={() => void copyPresetQuery(tab.id)}>
          <Copy size="0.923rem" /> Copy to project
        </button>
      )}
      <button data-stage="query:save" className="btn" disabled={!tab.sql.trim()} title={saveKeys ? `Save query (${saveKeys})` : "Save query"} onClick={() => saveQueryTab(tab.id)}>
        <Save size="0.923rem" /> Save
      </button>
      <button data-stage="query:pin" className="btn" disabled={!tab.saved || isPreset(tab.saved.origin)} title={pinKeys ? `Pin to the readout (${pinKeys} in the results)` : "Pin to the readout"} onClick={() => pinQueryTab(tab.id)}>
        <Pin size="0.923rem" /> {pinLabel(tab.saved?.pin)}
      </button>
    </>
  );
}

export function QueryTab({ tab }: { tab: QueryTabState }) {
  const database = useStore((s) => s.sessions.find((x) => x.id === tab.sessionId)?.database ?? null);
  const scheme = useResolvedScheme(useStore((s) => s.settings));
  const { updateQueryTab, runQuery, cancelQuery } = useStore.getState();
  const schema = useCompletionSchema(tab.sessionId, database);
  const viewRef = useRef<EditorView | null>(null);

  const run = (sql: string) => void runQuery(tab.id, sql);
  const isActive = useStore((s) => s.activeTabId === tab.id);
  useEffect(() => {
    if (!isActive) return;
    const onRun = (e: Event) => {
      const which = (e as CustomEvent<string>).detail;
      void runQuery(tab.id, which === "all" || !viewRef.current ? tab.sql : currentStatement(viewRef.current));
    };
    window.addEventListener(EVENTS.runQuery, onRun);
    return () => window.removeEventListener(EVENTS.runQuery, onRun);
  }, [isActive, tab.id, tab.sql, runQuery]);
  const { height } = useWindowSize();
  const editor = usePaneSize("sqlEditorHeight", 140, 48, height - 320);
  const results = tab.response?.results ?? [];

  return (
    <div className="query" style={{ gridTemplateRows: `${editor.size}px auto auto minmax(0, 1fr)` }}>
      <QueryEditor
        value={tab.sql}
        onChange={(sql) => updateQueryTab(tab.id, { sql })}
        schema={schema}
        dark={scheme.dark}
        onView={(v) => (viewRef.current = v)}
      />
      <Splitter axis="y" paneAfter={false} label="Resize editor" {...editor} />
      <div className="action-bar">
        {tab.running ? (
          <button data-stage="query:cancel" className="btn" onClick={() => void cancelQuery(tab.id)}>
            <Square size="0.923rem" /> Cancel
          </button>
        ) : (
          <>
            <button
              data-stage="query:run"
              className="btn btn-primary"
              disabled={!tab.sql.trim()}
              onClick={() => run(viewRef.current ? currentStatement(viewRef.current) : tab.sql)}
            >
              <Play size="0.923rem" /> Run
            </button>
            <button data-stage="query:run-all" className="btn" disabled={!tab.sql.trim()} onClick={() => run(tab.sql)}>
              <PlayCircle size="0.923rem" /> Run all
            </button>
          </>
        )}
        <RunHint />
        <div className="spacer" />
        <SavedActions tab={tab} />
        {results.length > 1 && (
          <div className="segmented" aria-label="Results" data-stage="query:results-picker">
            {results.map((_, i) => (
              <button key={i} aria-pressed={i === tab.activeResult} onClick={() => updateQueryTab(tab.id, { activeResult: i })}>
                {i + 1}
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="results">
        <ResultsPane tab={tab} />
        <QueryChanges tab={tab} />
      </div>
    </div>
  );
}
