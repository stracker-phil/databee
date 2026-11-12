import type { ReadoutTab } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { useCollapsedGroups } from "../object/useCollapsedGroups";
import { useOpenValue } from "../value/useOpenValue";
import { Anomalies } from "./Anomalies";
import { CreatedSection } from "./CreatedSection";
import { PinnedSection } from "./PinnedSection";
import { RecentSection } from "./RecentSection";
import { StatusHeader } from "./StatusHeader";
import { useReadout } from "./useReadout";
import { WatchedSection } from "./WatchedSection";

export function Readout({ tab }: { tab: ReadoutTab }) {
  const entry = useReadout(tab);
  const { openValue, targetMenu } = useOpenValue(null);
  const collapse = useCollapsedGroups(tab.address);
  const readout = entry?.readout;
  if (entry?.error && !readout) return <ErrorPanel message={entry.error.message} code={entry.error.code} />;
  if (!readout) return <div className="empty-state">Loading…</div>;
  return (
    <div className="readout scroller" data-stage="readout" data-stage-label={`readout · ${tab.database}`}>
      <StatusHeader status={readout.status} onOpen={openValue} />
      <Anomalies part={readout.anomalies} collapse={collapse} />
      <WatchedSection part={readout.watched} tab={tab} collapse={collapse} onOpen={openValue} />
      <CreatedSection part={readout.recentlyCreated} database={tab.database} collapse={collapse} onOpen={openValue} />
      <PinnedSection part={readout.pinned} collapse={collapse} onOpen={openValue} />
      <RecentSection part={readout.recent} collapse={collapse} onOpen={openValue} />
      {targetMenu}
    </div>
  );
}
