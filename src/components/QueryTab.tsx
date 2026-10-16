import { useEffect, useRef, useState } from "react";
import type { EditorView } from "@codemirror/view";
import { AlertTriangle, CheckCircle2, Loader2, Play, PlayCircle, Square } from "lucide-react";
import { api } from "../api";
import { EVENTS } from "../commands";
import { formatCount } from "../lib/format";
import { useResolvedTheme } from "../lib/theme";
import { useStore, type QueryTab as QueryTabState } from "../state/store";
import { currentStatement, QueryEditor } from "./QueryEditor";
import { ResultsGrid } from "./ResultsGrid";
import { Splitter, usePaneSize, useWindowSize } from "./Splitter";

export function ErrorPanel({ message, code }: { message: string; code: string | null }) {
  return (
    <div className="error-panel" role="alert">
      <AlertTriangle size="1.231rem" style={{ flex: "none", marginTop: 1 }} />
      <div>
        {code && <div className="code">Error {code}</div>}
        <div className="msg">{message}</div>
      </div>
    </div>
  );
}

function useCompletionSchema(sessionId: string, database: string | null) {
  const [schema, setSchema] = useState<Record<string, string[]>>({});
  useEffect(() => {
    if (!database) return;
    let alive = true;
    api.completionSchema(sessionId, database).then(
      (s) => alive && setSchema(s),
      () => alive && setSchema({}),
    );
    return () => {
      alive = false;
    };
  }, [sessionId, database]);
  return schema;
}

export function QueryTab({ tab }: { tab: QueryTabState }) {
  const database = useStore((s) => s.sessions.find((x) => x.id === tab.sessionId)?.database ?? null);
  const theme = useResolvedTheme(useStore((s) => s.settings?.theme));
  const { updateTab, runQuery, cancelQuery } = useStore.getState();
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
  const editor = usePaneSize("editorHeight", 140, 48, height - 320);
  const results = tab.response?.results ?? [];
  const result = results[tab.activeResult];

  return (
    <div className="query" style={{ gridTemplateRows: `${editor.size}px auto auto minmax(0, 1fr)` }}>
      <QueryEditor
        value={tab.sql}
        onChange={(sql) => updateTab(tab.id, { sql })}
        schema={schema}
        theme={theme}
        onView={(v) => (viewRef.current = v)}
      />
      <Splitter axis="y" paneAfter={false} label="Resize editor" {...editor} />
      <div className="action-bar">
        {tab.running ? (
          <button className="btn" onClick={() => void cancelQuery(tab.id)}>
            <Square size="0.923rem" /> Cancel
          </button>
        ) : (
          <>
            <button
              className="btn btn-primary"
              disabled={!tab.sql.trim()}
              onClick={() => run(viewRef.current ? currentStatement(viewRef.current) : tab.sql)}
            >
              <Play size="0.923rem" /> Run
            </button>
            <button className="btn" disabled={!tab.sql.trim()} onClick={() => run(tab.sql)}>
              <PlayCircle size="0.923rem" /> Run all
            </button>
          </>
        )}
        <span className="kbd">⌘↵ statement · ⇧⌘↵ all</span>
        <div className="spacer" />
        {results.length > 1 && (
          <div className="segmented" aria-label="Results">
            {results.map((_, i) => (
              <button key={i} aria-pressed={i === tab.activeResult} onClick={() => updateTab(tab.id, { activeResult: i })}>
                {i + 1}
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="results">
        {tab.running ? (
          <div className="empty-state">
            <span><Loader2 size="1.231rem" className="spin" style={{ verticalAlign: -3 }} /> Running…</span>
          </div>
        ) : tab.error ? (
          <ErrorPanel message={tab.error.message} code={tab.error.code} />
        ) : !result ? (
          <div className="empty-state">Run a query to see results here.</div>
        ) : result.returnsRows ? (
          <ResultsGrid columns={result.columns} rows={result.rows} />
        ) : (
          <div className="message-panel">
            <CheckCircle2 size="1.538rem" style={{ color: "var(--success)" }} />
            <div className="big">{formatCount(result.affectedRows)} {result.affectedRows === 1 ? "row" : "rows"} affected</div>
            {result.lastInsertId != null && <div className="muted">Last insert ID: {result.lastInsertId}</div>}
          </div>
        )}
      </div>
    </div>
  );
}
