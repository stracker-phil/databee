import type { MenuItem } from "../components/ContextMenu";
import { getCommand } from "./registry";

/** A context menu item that runs a command and shows its key; `label` replaces the command's title. */
export function commandItem(id: string, label?: string): MenuItem {
  const command = getCommand(id);
  if (!command) throw new Error(`Unknown command "${id}"`);
  return { label: label ?? command.title, command: id, disabled: command.when?.() === false, action: () => void command.run() };
}
