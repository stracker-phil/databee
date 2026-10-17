import { api, type ActionWrite, type Address, type ObjectAction } from "../../api";
import type { Command } from "../../commands";
import { documentText, objectAddress } from "../../lib/document";
import { fail, selectActiveTab, selectDocument, selectSessionBySlug, useStore } from "../../state/store";
import type { MenuEntry } from "../ContextMenu";

function writeText(write: ActionWrite): string {
  switch (write.write) {
    case "set":
      return `${write.table}: ${write.columns.join(", ")}`;
    case "deleteRows":
      return `${write.table}: deletes rows`;
    case "freeSql":
      return `${write.table}: free SQL`;
  }
}

/** What the action writes, step by step, e.g. "wp_wc_orders: date_updated_gmt, status · wp_posts: post_status". */
export const writesSummary = (action: { writes: ActionWrite[] }) => action.writes.map(writeText).join(" · ");

/** Opens the action sheet: params, and the rows and values the action writes, before it is queued. */
function runAction(address: Address, action: ObjectAction) {
  useStore.getState().openDialog({ type: "action", address: objectAddress(address, "document"), action });
}

/** Opens the sheet of table action `name` on a table address (an anomaly's fix). */
export async function runTableAction(address: Address, name: string) {
  const s = useStore.getState();
  const session = selectSessionBySlug(s, address.connection);
  if (!session || address.target.kind !== "table") return;
  try {
    const action = (await api.tableActions(session.id, address.database, address.target.table)).find((a) => a.name === name);
    if (action) s.openDialog({ type: "action", address, action });
    else s.toast("error", `${address.target.table} has no action ${name}`);
  } catch (e) {
    fail(e, `Cannot open ${name}`);
  }
}

const actionTitle = (action: ObjectAction) => `Run ${action.name}${action.undo ? "" : " (no undo)"}`;

export function actionMenu(address: Address, actions: ObjectAction[]): MenuEntry[] {
  if (actions.length === 0) return [{ label: "No actions apply to this row", disabled: true, action: () => {} }];
  return actions.map((action) => ({ label: actionTitle(action), detail: writesSummary(action), action: () => runAction(address, action) }));
}

/** The active object's actions as palette commands (`>`); they exist only while that document is loaded. */
export function objectActionCommands(): Command[] {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  if (tab?.kind !== "object") return [];
  const document = selectDocument(s, tab.sessionId, documentText(tab.address))?.document;
  return (document?.actions ?? []).map((action) => ({
    id: `action.${action.name}`,
    title: actionTitle(action),
    scope: "object",
    run: () => runAction(tab.address, action),
  }));
}
