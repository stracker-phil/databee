import { useEffect, useRef, useState } from "react";
import type { EditorView } from "@codemirror/view";
import { CheckCircle2, Loader2, Play, PlayCircle, Square } from "lucide-react";
import { api, toAppError } from "../api";
import { EVENTS, useBindingLabel } from "../commands";
import { formatCount } from "../lib/format";
import { useResolvedTheme } from "../lib/theme";
import { useStore, type QueryTab as QueryTabState } from "../state/store";
import { ErrorPanel } from "./ErrorPanel";
import { currentStatement, QueryEditor } from "./QueryEditor";
import { ResultsGrid } from "./ResultsGrid";
import { Splitter, usePaneSize, useWindowSize } from "./Splitter";

/** Columns per table for completion; reloads when the table list changes (DDL reloads it). */
function useCompletionSchema(sessionId: string, database: string | null) {
  const [schema, setSchema] = useState<Record<string, string[]>>({});
  const tables = useStore((s) => (database ? s.schema[sessionId]?.tables[database] : undefined));
  useEffect(() => {
    if (!database) return;
    let alive = true;
    api.completionSchema(sessionId, database).then(
      (s) => alive && setSchema(s),
      // Completion is a convenience: keep the previous schema and log instead of toasting.
      (e) => void api.logError(`Completion schema for ${database}: ${toAppError(e).message}`).catch(() => {}),
    );
    return () => {
      alive = false;
    };
  }, [sessionId, database, tables]);
  return schema;
}

function RunHint() {
  const statement = useBindingLabel("editor.run");
  const all = useBindingLabel("editor.runAll");
  const parts = [statement && `${statement} statement`, all && `${all} all`].filter(Boolean);
  if (parts.length === 0) return null;
  return <span className="kbd">{parts.join(" · ")}</span>;
}

function ResultsPane({ tab }: { tab: QueryTabState }) {
  const result = tab.response?.results[tab.activeResult];
  if (tab.running) {
    return (
      <div className="empty-state">
        <span><Loader2 size="1.231rem" className="spin" style={{ verticalAlign: -3 }} /> Running…</span>
      </div>
    );
  }
  if (tab.error) return <ErrorPanel message={tab.error.message} code={tab.error.code} />;
  if (!result) return <div className="empty-state">Run a query to see results here.</div>;
  if (result.returnsRows) return <ResultsGrid columns={result.columns} rows={result.rows} />;
  return (
    <div className="message-panel">
      <CheckCircle2 size="1.538rem" style={{ color: "var(--success)" }} />
      <div className="big">{formatCount(result.affectedRows)} {result.affectedRows === 1 ? "row" : "rows"} affected</div>
      {result.lastInsertId != null && <div className="muted">Last insert ID: {result.lastInsertId}</div>}
    </div>
  );
}

export function QueryTab({ tab }: { tab: QueryTabState }) {
  const database = useStore((s) => s.sessions.find((x) => x.id === tab.sessionId)?.database ?? null);
  const theme = useResolvedTheme(useStore((s) => s.settings?.theme));
  const { updateQueryTab, runQuery, cancelQuery } = useStore.getState();
  const schema = useCompletionSchema(tab.sessionId, database);
  const viewRef = useRef<EditorView | null>(null);

  const run = (sql: string) => void runQuery(tab.id, sql);
  const isActive = useStore((s) => s.activeTabId === tab.id);
  useEffect(() => {
    if (!isActive) return;
    const onRun = (e: Event) => {
      const which = (e as CustomEvent<string>).detail;
      run(which === "all" || !viewRef.current ? tab.sql : currentStatement(viewRef.current));
    };
    window.addEventListener(EVENTS.runQuery, onRun);
    return () => window.removeEventListener(EVENTS.runQuery, onRun);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isActive, tab.id, tab.sql]);
  const { height } = useWindowSize();
  const editor = usePaneSize("sqlEditorHeight", 140, 48, height - 320);
  const results = tab.response?.results ?? [];

  return (
    <div className="query" style={{ gridTemplateRows: `${editor.size}px auto auto minmax(0, 1fr)` }}>
      <QueryEditor
        value={tab.sql}
        onChange={(sql) => updateQueryTab(tab.id, { sql })}
        schema={schema}
        theme={theme}
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
      </div>
    </div>
  );
}
