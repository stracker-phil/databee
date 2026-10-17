import { useCallback, useState, type MouseEvent } from "react";
import type { Address, Value } from "../../api";
import { runCommand } from "../../commands";
import { formatAddress } from "../../lib/address";
import { documentText } from "../../lib/document";
import { selectActiveTab, useStore } from "../../state/store";
import { ContextMenu, type MenuEntry } from "../ContextMenu";

const openTarget = (target: Address) => void useStore.getState().openAddress(formatAddress(target), { preview: true });

/**
 * Opens the row a primary key addresses as an object. The key of the object that `owner` shows switches
 * that object between Document and Rows instead (docs/ux.md, Value grammar).
 */
export function openRowObject(address: Address, owner: Address | null, preview: boolean) {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  if (owner && tab?.kind === "object" && documentText(owner) === documentText(address)) return s.toggleObjectView(tab.id);
  s.openObject(address, { preview });
}

/**
 * The click handler for values that act on click (`Value`'s `onOpen`) and the menu for a reference with
 * several targets. A click lands after the mousedown that focused the value, so `value.filter` acts on it.
 */
export function useOpenValue(owner: Address | null) {
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const ownerText = owner ? formatAddress(owner) : null;
  const openValue = useCallback(
    (value: Value, e: MouseEvent) => {
      const kind = value.kind;
      if (kind.kind === "enum") return void runCommand("value.filter");
      if (kind.kind === "primaryKey") return openRowObject(kind.address, owner, true);
      if (kind.kind !== "reference") return;
      if (kind.targets.length === 1) return openTarget(kind.targets[0]);
      setMenu({ x: e.clientX, y: e.clientY, items: kind.targets.map((t) => ({ label: `Open ${formatAddress(t)}`, action: () => openTarget(t) })) });
    },
    // `owner` matters only through its text.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ownerText],
  );
  const targetMenu = menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />;
  return { openValue, targetMenu };
}
