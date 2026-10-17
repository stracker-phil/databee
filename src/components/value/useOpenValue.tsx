import { useCallback, useContext, useLayoutEffect, useRef, useState, type MouseEvent } from "react";
import type { Address, Value } from "../../api";
import { runCommand } from "../../commands";
import { formatAddress } from "../../lib/address";
import { ContextMenu, type MenuEntry } from "../ContextMenu";
import { ColumnContext, openRowObject, openTarget, showValue } from "./open";

/**
 * The click handler for values that act on click (`Value`'s `onOpen`) and the menu for a reference with
 * several targets. A click lands after the mousedown that focused the value, so `value.filter` acts on it.
 * Inside a column stack (`ColumnContext`) references and other rows drill into the next column.
 */
export function useOpenValue(owner: Address | null) {
  const place = useContext(ColumnContext);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const ownerRef = useRef(owner);
  useLayoutEffect(() => {
    ownerRef.current = owner;
  });
  const openValue = useCallback(
    (value: Value, e: MouseEvent) => {
      const kind = value.kind;
      if (kind.kind === "enum") return void runCommand("value.filter");
      if (kind.kind === "primaryKey") return openRowObject(kind.address, ownerRef.current, true, place);
      if (kind.kind === "serialized") return void showValue(value, place);
      if (kind.kind !== "reference") return;
      if (kind.targets.length === 1) return openTarget(kind.targets[0], place);
      setMenu({ x: e.clientX, y: e.clientY, items: kind.targets.map((t) => ({ label: `Open ${formatAddress(t)}`, action: () => openTarget(t, place) })) });
    },
    [place],
  );
  const openRow = useCallback(
    (address: Address, preview: boolean) => openRowObject(address, ownerRef.current, preview, place),
    [place],
  );
  const targetMenu = menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />;
  return { openValue, openRow, place, targetMenu };
}
