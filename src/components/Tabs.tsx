import { FileCode2, FileText, Loader2, Plus, Table2, X } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { useBindingLabel } from "../commands";
import { useStore, type Tab } from "../state/store";
import { formatAddress } from "../lib/address";
import { documentText } from "../lib/document";
import { ObjectPanel } from "./object/ObjectPanel";
import { QueryTab } from "./QueryTab";
import { TableTab } from "./TableTab";

const ARROW_STEPS: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1 };

function tabIcon(tab: Tab) {
  if (tab.kind === "table") return <Table2 size="0.923rem" />;
  if (tab.kind === "object") return <FileText size="0.923rem" />;
  if (tab.running) return <Loader2 size="0.923rem" className="spin" />;
  return <FileCode2 size="0.923rem" />;
}

const PREVIEW_HINT = " · preview (double-click to keep)";

function tabTooltip(tab: Tab): string {
  if (tab.kind === "query") return tab.title;
  if (tab.kind === "object") return `${formatAddress(tab.address)}${tab.preview ? PREVIEW_HINT : ""}`;
  const columns = tab.where.map((w) => ` · ${w.column}: ${w.value ?? "NULL"}`).join("");
  const filter = tab.filter ? ` · ${tab.filter}` : "";
  const preview = tab.preview ? PREVIEW_HINT : "";
  return `${tab.database}.${tab.table}${columns}${filter}${preview}`;
}

function tabStage(tab: Tab): string {
  if (tab.kind === "table") return `tab:table:${tab.database}.${tab.table}`;
  if (tab.kind === "object") return `tab:object:${documentText(tab.address)}`;
  return `tab:query:${tab.id}`;
}

function TabPanel({ tab }: { tab: Tab }) {
  if (tab.kind === "query") return <QueryTab tab={tab} />;
  if (tab.kind === "object") return <ObjectPanel tab={tab} />;
  return <TableTab tab={tab} />;
}

function TabItem({ tab, active }: { tab: Tab; active: boolean }) {
  const { activateTab, closeTab, pinTab } = useStore.getState();
  const preview = tab.kind !== "query" && tab.preview;
  return (
    <div
      role="tab"
      tabIndex={active ? 0 : -1}
      data-stage={tabStage(tab)}
      data-stage-label={tab.title}
      data-stage-list="tabs"
      aria-selected={active}
      className={`tab${active ? " active" : ""}${preview ? " preview" : ""}`}
      onMouseDown={(e) => e.button === 0 && activateTab(tab.id)}
      onDoubleClick={() => pinTab(tab.id)}
      onAuxClick={(e) => e.button === 1 && closeTab(tab.id)}
      title={tabTooltip(tab)}
    >
      {tabIcon(tab)}
      <span className="title">{tab.title}</span>
      <button
        className="icon-btn close"
        aria-label={`Close ${tab.title}`}
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={() => closeTab(tab.id)}
      >
        <X size="0.846rem" />
      </button>
    </div>
  );
}

function focusTab(container: HTMLElement, index: number) {
  container.querySelectorAll<HTMLElement>('[role="tab"]')[index]?.focus();
}

export function TabBar() {
  const sessionId = useStore((s) => s.activeSessionId);
  const tabs = useStore(useShallow((s) => s.tabs.filter((t) => t.sessionId === s.activeSessionId)));
  const activeTabId = useStore((s) => s.activeTabId);
  const newTabKey = useBindingLabel("tabs.new");
  if (!sessionId) return null;

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = ARROW_STEPS[e.key];
    if (!step || !(e.target instanceof HTMLElement) || e.target.getAttribute("role") !== "tab") return;
    e.preventDefault();
    const { cycleTab } = useStore.getState();
    cycleTab(step);
    const activeId = useStore.getState().activeTabId;
    focusTab(e.currentTarget, tabs.findIndex((t) => t.id === activeId));
  };

  return (
    <div className="tabbar" role="tablist" data-stage="tabs" data-stage-label="Tabs" onKeyDown={onKeyDown}>
      {tabs.map((t) => (
        <TabItem key={t.id} tab={t} active={t.id === activeTabId} />
      ))}
      <button
        className="icon-btn new-tab"
        aria-label="New query tab"
        title={newTabKey ? `New query tab ${newTabKey}` : "New query tab"}
        data-stage="tabs:new"
        onClick={() => useStore.getState().openQueryTab()}
      >
        <Plus size="1.077rem" />
      </button>
    </div>
  );
}

/** All tabs of the active session stay mounted so editor history and paging survive switching. */
export function TabPanels() {
  const tabs = useStore(useShallow((s) => s.tabs.filter((t) => t.sessionId === s.activeSessionId)));
  const activeTabId = useStore((s) => s.activeTabId);
  const newTabKey = useBindingLabel("tabs.new");
  if (tabs.length === 0) {
    const newQuery = newTabKey ? `press ${newTabKey}` : "click +";
    return <div className="empty-state">Open a table from the sidebar or {newQuery} for a new query.</div>;
  }
  return (
    <>
      {tabs.map((t) => (
        <div key={t.id} role="tabpanel" className="tab-content" hidden={t.id !== activeTabId}>
          <TabPanel tab={t} />
        </div>
      ))}
    </>
  );
}
