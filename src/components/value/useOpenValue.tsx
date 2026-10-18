import { useCallback, useContext, useLayoutEffect, useRef, type MouseEvent } from "react";
import type { Address, Value } from "../../api";
import { runCommand } from "../../commands";
import { formatAddress } from "../../lib/address";
import { useContextMenu } from "../ContextMenu";
import { clickMode, ColumnContext, openRowObject, openTarget, showValue, type OpenMode } from "./open";

/**
 * The click handler for values that act on click (`Value`'s `onOpen`) and the menu for a reference with
 * several targets. A click lands after the mousedown that focused the value, so `value.filter` acts on it.
 * Inside a column stack (`ColumnContext`) references and other rows drill into the next column. A reference
 * opens in `mode` (a click: `clickMode`); with several targets the menu comes first.
 */
export function useOpenValue(owner: Address | null) {
  const place = useContext(ColumnContext);
  const { open: openMenu, element: targetMenu } = useContextMenu();
  const ownerRef = useRef(owner);
  useLayoutEffect(() => {
    ownerRef.current = owner;
  });
  const openValue = useCallback(
    (value: Value, e: MouseEvent, mode: OpenMode = clickMode(place)) => {
      const kind = value.kind;
      if (kind.kind === "enum") return void runCommand("value.filter");
      if (kind.kind === "primaryKey") return openRowObject(kind.address, ownerRef.current, true, place);
      if (kind.kind === "serialized") return void showValue(value, place);
      if (kind.kind !== "reference") return;
      const open = (target: Address) => openTarget(target, place, mode, value);
      if (kind.targets.length === 1) return open(kind.targets[0]);
      openMenu({ x: e.clientX, y: e.clientY }, kind.targets.map((t) => ({ label: `Open ${formatAddress(t)}`, action: () => open(t) })));
    },
    [place, openMenu],
  );
  const openRow = useCallback(
    (address: Address, preview: boolean) => openRowObject(address, ownerRef.current, preview, place),
    [place],
  );
  return { openValue, openRow, place, targetMenu };
}
