import { useCallback, useState } from "react";
import { bindingFor, bindingLabel, getCommand } from "../commands/registry";
import { activeScopes } from "../commands/scopes";
import { useStore } from "../state/store";
import { MenuPanel, type PanelEntry } from "./MenuPanel";

export interface MenuItem {
  label: string;
  action: () => void;
  disabled?: boolean;
  /** The tooltip, e.g. why the item is disabled. */
  title?: string;
  /** A secondary line under the label. */
  detail?: string;
  /** The command the item runs; its key shows on the right while the command's scope is active. */
  command?: string;
}

export type MenuEntry = MenuItem | "separator";

/** Where a menu opens: a point, or the pointer event that asked for it (its default menu is suppressed). */
export type MenuAnchor = { x: number; y: number } | { clientX: number; clientY: number; preventDefault: () => void };

interface Props {
  x: number;
  y: number;
  items: MenuEntry[];
  onClose: () => void;
}

/** The app's context menu, drawn by the shared `MenuPanel`. */
export function ContextMenu({ x, y, items, onClose }: Props) {
  const keyOf = useCommandKey();
  const entries: PanelEntry[] = items.map((item) =>
    item === "separator"
      ? item
      : { label: item.label, onChoose: item.action, disabled: item.disabled, title: item.title, detail: item.detail, keyLabel: item.command && !item.detail ? keyOf(item.command) : undefined },
  );
  return <MenuPanel key={`${x},${y}`} entries={entries} placement={{ point: { x, y } }} onClose={onClose} />;
}

/** Opens a context menu built from `MenuEntry` data; render `element` anywhere in the component. */
export function useContextMenu() {
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const open = useCallback((at: MenuAnchor, items: MenuEntry[]) => {
    if ("clientX" in at) at.preventDefault();
    setMenu({ ...("clientX" in at ? { x: at.clientX, y: at.clientY } : at), items });
  }, []);
  const element = menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />;
  return { open, element };
}

/** The key label of a command in an active scope, "" otherwise; follows the user's rebinding. */
function useCommandKey() {
  const custom = useStore((s) => s.keybindings);
  const scopes = activeScopes();
  return (id: string) => {
    const command = getCommand(id);
    return command && scopes.includes(command.scope) ? bindingLabel(bindingFor(id, custom)) : "";
  };
}
