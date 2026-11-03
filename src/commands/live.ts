import { formatAddress } from "../lib/address";
import { objectColumnAt } from "../lib/columns";
import { selectActiveTab, selectCanDismissMarks, selectChangeTarget, useStore } from "../state/store";
import type { Handler } from "./catalog";

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

export const LIVE_COMMANDS: Handler[] = [
  { id: "object.live", when: targetIs("object"), run: () => void useStore.getState().toggleLive() },
  { id: "object.dismissChanges", when: canDismiss, run: dismiss },
  { id: "object.snapshot", when: targetIs("object"), run: takeSnapshot },
  { id: "object.sinceSnapshot", when: targetIs("object"), run: chooseSnapshot },
  { id: "object.compare", when: targetIs("object"), run: compareWith },
  { id: "query.snapshot", when: targetIs("query"), run: takeSnapshot },
  { id: "query.sinceSnapshot", when: targetIs("query"), run: chooseSnapshot },
  { id: "query.dismissChanges", when: canDismiss, run: dismiss },
];
