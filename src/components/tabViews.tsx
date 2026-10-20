/** How each tab kind looks: its icon, tooltip, Stage id and panel. What it does in the store is in `state/tabKinds.ts`. */
import type { ReactNode } from "react";
import { FileCode2, FileText, Home, Loader2, SlidersHorizontal } from "lucide-react";
import { useStore, type QueryTab as QueryTabState, type Tab, type TableTab as TableTabState } from "../state/store";
import type { TabOfKind } from "../state/slices/tabs";
import { formatAddress } from "../lib/address";
import { formatCount } from "../lib/format";
import { columnLabel, objectColumnAt } from "../lib/columns";
import { documentText } from "../lib/document";
import { ObjectPanel } from "./object/ObjectPanel";
import { TableIcon } from "./TableIcon";
import { ProjectPanel } from "./project/ProjectPanel";
import { QueryTab } from "./QueryTab";
import { Readout } from "./readout/Readout";
import { TableTab } from "./TableTab";
import { ValueColumnView } from "./value/ValueColumnView";

interface TabView<T extends Tab = Tab> {
  icon: (tab: T) => ReactNode;
  tooltip: (tab: T) => string;
  /** The tab's `data-stage` anchor. */
  stage: (tab: T) => string;
  Panel: (props: { tab: T }) => ReactNode;
  /** Shown after the address in the status bar. */
  addressHint?: (tab: T) => ReactNode;
  /** Shown after the address slot in the status bar, also while it shows a value's provenance. */
  Summary?: (props: { tab: T }) => ReactNode;
}

const ICON_SIZE = "0.923rem";
const PREVIEW_HINT = " · preview (double-click to keep)";
const previewHint = (tab: { preview?: boolean }) => (tab.preview ? PREVIEW_HINT : "");

/** A table or query tab with the value column opened from its grid on the right; the tab keeps its tree position. */
function WithValueColumn({ tab, children }: { tab: QueryTabState | TableTabState; children: ReactNode }) {
  const column = tab.valueColumn;
  return (
    <div className="tab-columns">
      <div className="tab-main">{children}</div>
      {column && (
        <section className="column value-column" aria-label={columnLabel(column)} data-stage="value-column" data-stage-label={columnLabel(column)}>
          <ValueColumnView column={column} place={{ tabId: tab.id, index: null }} onClose={() => useStore.getState().closeRightmostColumn()} />
        </section>
      )}
    </div>
  );
}

function QuerySummary({ tab }: { tab: QueryTabState }) {
  const rowLimit = useStore((s) => s.settings?.rowLimit ?? 0);
  if (!tab.response || tab.error) return null;
  const r = tab.response.results[tab.activeResult];
  return (
    <>
      {r?.returnsRows && (
        <span className={r.truncated ? "warn" : undefined}>
          {r.truncated
            ? `Showing ${formatCount(r.rows.length)} of ${formatCount(r.totalRows)} rows (row limit ${formatCount(rowLimit)})`
            : `${formatCount(r.rows.length)} ${r.rows.length === 1 ? "row" : "rows"}`}
        </span>
      )}
      <span>{formatCount(tab.response.elapsedMs)} ms</span>
      {tab.response.results.length > 1 && <span>{tab.response.results.length} statements</span>}
    </>
  );
}

function tableTooltip(tab: TableTabState): string {
  const columns = tab.where.map((w) => ` · ${w.column}: ${w.value ?? "NULL"}`).join("");
  const filter = tab.filter ? ` · ${tab.filter}` : "";
  return `${tab.database}.${tab.table}${columns}${filter}${previewHint(tab)}`;
}

const TAB_VIEWS: { [K in Tab["kind"]]: TabView<TabOfKind<K>> } = {
  query: {
    icon: (tab) => (tab.running ? <Loader2 size={ICON_SIZE} className="spin" /> : <FileCode2 size={ICON_SIZE} />),
    tooltip: (tab) => tab.title,
    stage: (tab) => `tab:query:${tab.id}`,
    Panel: ({ tab }) => (
      <WithValueColumn tab={tab}>
        <QueryTab tab={tab} />
      </WithValueColumn>
    ),
    Summary: QuerySummary,
  },
  table: {
    icon: (tab) => <TableIcon database={tab.database} table={tab.table} sessionId={tab.sessionId} />,
    tooltip: tableTooltip,
    stage: (tab) => `tab:table:${tab.database}.${tab.table}`,
    Panel: ({ tab }) => (
      <WithValueColumn tab={tab}>
        <TableTab tab={tab} />
      </WithValueColumn>
    ),
    addressHint: (tab) =>
      tab.filter ? (
        <span className="o3" title={`WHERE ${tab.filter}`}>
          · filtered
        </span>
      ) : null,
  },
  object: {
    icon: (tab) => {
      const root = objectColumnAt(tab.columns, 0)?.address ?? tab.address;
      const table = "table" in root.target ? root.target.table : null;
      return <TableIcon database={root.database} table={table} sessionId={tab.sessionId} fallback={<FileText size={ICON_SIZE} />} />;
    },
    tooltip: (tab) => `${formatAddress(tab.address)}${previewHint(tab)}`,
    stage: (tab) => `tab:object:${documentText(objectColumnAt(tab.columns, 0)?.address ?? tab.address)}`,
    Panel: ({ tab }) => <ObjectPanel tab={tab} />,
  },
  project: {
    icon: () => <SlidersHorizontal size={ICON_SIZE} />,
    tooltip: (tab) => formatAddress(tab.address),
    stage: (tab) => `tab:project:${tab.database}`,
    Panel: ({ tab }) => <ProjectPanel tab={tab} />,
  },
  readout: {
    icon: () => <Home size={ICON_SIZE} />,
    tooltip: (tab) => formatAddress(tab.address),
    stage: (tab) => `tab:readout:${tab.database}`,
    Panel: ({ tab }) => <Readout tab={tab} />,
  },
};

export const tabView = <T extends Tab>(tab: T): TabView<T> => TAB_VIEWS[tab.kind] as unknown as TabView<T>;
