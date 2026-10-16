import { FileCode2, Loader2, Plus, Table2, X } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { useStore } from "../state/store";
import { QueryTab } from "./QueryTab";
import { TableTab } from "./TableTab";

export function TabBar() {
  const sessionId = useStore((s) => s.activeSessionId);
  const tabs = useStore(useShallow((s) => s.tabs.filter((t) => t.sessionId === s.activeSessionId)));
  const activeTabId = useStore((s) => s.activeTabId);
  const { activateTab, closeTab, openQueryTab, pinTab } = useStore.getState();
  if (!sessionId) return null;
  return (
    <div className="tabbar" role="tablist" data-stage="tabs" data-stage-label="Tabs">
      {tabs.map((t) => (
        <div
          key={t.id}
          role="tab"
          data-stage={t.kind === "table" ? `tab:table:${t.database}.${t.table}` : `tab:query:${t.id}`}
          data-stage-label={t.title}
          data-stage-list="tabs"
          aria-selected={t.id === activeTabId}
          className={`tab${t.id === activeTabId ? " active" : ""}${t.kind === "table" && t.preview ? " preview" : ""}`}
          onMouseDown={(e) => e.button === 0 && activateTab(t.id)}
          onDoubleClick={() => pinTab(t.id)}
          onAuxClick={(e) => e.button === 1 && closeTab(t.id)}
          title={t.kind === "table" ? `${t.database}.${t.table}${t.filter ? ` WHERE ${t.filter}` : ""}${t.preview ? " · preview (double-click to keep)" : ""}` : t.title}
        >
          {t.kind === "query" && t.running ? (
            <Loader2 size="0.923rem" className="spin" />
          ) : t.kind === "query" ? (
            <FileCode2 size="0.923rem" />
          ) : (
            <Table2 size="0.923rem" />
          )}
          <span className="title">{t.title}</span>
          <button
            className="icon-btn close"
            aria-label={`Close ${t.title}`}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => closeTab(t.id)}
          >
            <X size="0.846rem" />
          </button>
        </div>
      ))}
      <button className="icon-btn" style={{ alignSelf: "center", marginLeft: 4 }} aria-label="New query tab" data-stage="tabs:new" onClick={() => openQueryTab()}>
        <Plus size="1.077rem" />
      </button>
    </div>
  );
}

/** All tabs of the active session stay mounted so editor history and paging survive switching. */
export function TabPanels() {
  const tabs = useStore(useShallow((s) => s.tabs.filter((t) => t.sessionId === s.activeSessionId)));
  const activeTabId = useStore((s) => s.activeTabId);
  if (tabs.length === 0) {
    return <div className="empty-state">Open a table from the sidebar or press ⌘T for a new query.</div>;
  }
  return (
    <>
      {tabs.map((t) => (
        <div key={t.id} role="tabpanel" className="tab-content" hidden={t.id !== activeTabId} style={t.id === activeTabId ? undefined : { display: "none" }}>
          {t.kind === "query" ? <QueryTab tab={t} /> : <TableTab tab={t} />}
        </div>
      ))}
    </>
  );
}
