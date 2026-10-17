import { formatAddress } from "../lib/address";
import { objectColumnAt } from "../lib/columns";
import { selectActiveTab, selectCanDismissMarks, selectChangeTarget, useStore } from "../state/store";
import type { Command } from "./registry";

const targetIs = (kind: "object" | "query") => () => selectChangeTarget(useStore.getState())?.kind === kind;
const canDismiss = () => selectCanDismissMarks(useStore.getState());

/** Asks for another key of the active column's table and opens `?compare=<key>`. */
function compareWith() {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  const column = tab?.kind === "object" ? objectColumnAt(tab.columns, tab.active) : null;
  const target = column?.address.target;
  if (!column || target?.kind !== "row") return;
  s.openDialog({
    type: "prompt",
    title: `Compare ${target.table} #${target.key} with key`,
    value: "",
    onSubmit: (key) => void useStore.getState().openAddress(formatAddress({ ...column.address, view: null, compare: key })),
  });
}

const takeSnapshot = () => useStore.getState().takeSnapshot();
const chooseSnapshot = () => void useStore.getState().chooseSnapshot();
const dismiss = () => useStore.getState().dismissMarks();

export const LIVE_COMMANDS: Command[] = [
  { id: "object.live", title: "Toggle live mode", scope: "object", defaultBinding: "W", when: targetIs("object"), run: () => void useStore.getState().toggleLive() },
  { id: "object.dismissChanges", title: "Dismiss change marks", scope: "object", defaultBinding: "X", when: canDismiss, run: dismiss },
  { id: "object.snapshot", title: "Take a snapshot…", scope: "object", defaultBinding: "S", when: targetIs("object"), run: takeSnapshot },
  { id: "object.sinceSnapshot", title: "Changes since snapshot…", scope: "object", when: targetIs("object"), run: chooseSnapshot },
  { id: "object.compare", title: "Compare with another row…", scope: "object", when: targetIs("object"), run: compareWith },
  { id: "query.snapshot", title: "Snapshot saved query results…", scope: "editor", when: targetIs("query"), run: takeSnapshot },
  { id: "query.sinceSnapshot", title: "Changes since snapshot…", scope: "editor", when: targetIs("query"), run: chooseSnapshot },
  { id: "query.dismissChanges", title: "Dismiss change marks", scope: "editor", when: canDismiss, run: dismiss },
];
