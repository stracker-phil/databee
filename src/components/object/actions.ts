/** Rule actions of an object (docs/presets.md, actions): planned through `plan_action` into the pending tray. */
import type { ActionParam, Address, ObjectAction } from "../../api";
import type { Command } from "../../commands";
import { documentText, objectAddress } from "../../lib/document";
import { selectActiveTab, selectDocument, useStore } from "../../state/store";
import type { MenuEntry } from "../ContextMenu";

const MAX_CHOICES = 8;

function paramTitle(action: ObjectAction, param: ActionParam): string {
  const shown = param.choices.slice(0, MAX_CHOICES).join(", ");
  const more = param.choices.length > MAX_CHOICES ? ", …" : "";
  const choices = shown ? ` (${shown}${more}${param.free ? ", or any value" : ""})` : "";
  return `${action.name}: ${param.name}${choices}`;
}

/** Asks for each required parameter in turn, then calls `done`; optional ones are left to their default. */
function askParams(action: ObjectAction, params: Record<string, string>, index: number, done: (params: Record<string, string>) => void) {
  const param = action.params[index];
  if (!param) return done(params);
  if (param.optional) return askParams(action, params, index + 1, done);
  useStore.getState().openDialog({
    type: "prompt",
    title: paramTitle(action, param),
    value: param.default ?? param.choices[0] ?? "",
    onSubmit: (value) => askParams(action, { ...params, [param.name]: value }, index + 1, done),
  });
}

export function runAction(address: Address, action: ObjectAction) {
  askParams(action, {}, 0, (params) => {
    void useStore.getState().queueAction(objectAddress(address, "document"), action.name, params);
  });
}

const actionTitle = (action: ObjectAction) => `Run ${action.name}${action.undo ? "" : " (no undo)"}`;

export function actionMenu(address: Address, actions: ObjectAction[]): MenuEntry[] {
  if (actions.length === 0) return [{ label: "No actions apply to this row", disabled: true, action: () => {} }];
  return actions.map((action) => ({ label: actionTitle(action), action: () => runAction(address, action) }));
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
