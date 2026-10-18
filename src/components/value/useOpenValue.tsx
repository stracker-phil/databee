import { useCallback, useContext, useLayoutEffect, useMemo, useRef, type MouseEvent } from "react";
import type { Address, Value } from "../../api";
import { runCommand } from "../../commands";
import { formatAddress } from "../../lib/address";
import { documentText } from "../../lib/document";
import { useContextMenu } from "../ContextMenu";
import { clickEffect, ColumnContext, openRowObject, openTarget, showValue } from "./open";
import type { OpenValue } from "./Value";

/**
 * The click handler for values that act on click (`Value`'s `onOpen`) and the menu for a reference with
 * several targets. A click lands after the mousedown that focused the value, so `value.filter` acts on it.
 * Inside a column stack (`ColumnContext`) references and other rows drill into the next column. The handler's
 * `effect` says what a click does (`clickEffect`), for the indicator a value shows on hover.
 */
export function useOpenValue(owner: Address | null) {
  const place = useContext(ColumnContext);
  const { open: openMenu, element: targetMenu } = useContextMenu();
  const ownerRef = useRef(owner);
  useLayoutEffect(() => {
    ownerRef.current = owner;
  });
  const ownerText = owner ? documentText(owner) : null;
  const open = useCallback(
    (value: Value, e: MouseEvent) => {
      const kind = value.kind;
      if (kind.kind === "enum") return void runCommand("value.filter");
      if (kind.kind === "primaryKey") return openRowObject(kind.address, ownerRef.current, true, place);
      if (kind.kind === "serialized") return void showValue(value, place);
      if (kind.kind !== "reference") return;
      if (kind.targets.length === 1) return openTarget(kind.targets[0], place);
      openMenu({ x: e.clientX, y: e.clientY }, kind.targets.map((t) => ({ label: `Open ${formatAddress(t)}`, action: () => openTarget(t, place) })));
    },
    [place, openMenu],
  );
  const openValue: OpenValue = useMemo(() => {
    return Object.assign((value: Value, e: MouseEvent) => open(value, e), {
      effect: (value: Value, density: "grid" | "full") => clickEffect(value, density, owner, place),
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `ownerText` stands for `owner`, which may be a new object on every render
  }, [open, ownerText, place]);
  const openRow = useCallback(
    (address: Address, preview: boolean) => openRowObject(address, ownerRef.current, preview, place),
    [place],
  );
  return { openValue, openRow, place, targetMenu };
}
