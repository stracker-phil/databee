import { Check } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";

export interface PanelItem {
  label: string;
  onChoose: () => void;
  disabled?: boolean;
  title?: string;
  /** A secondary line under the label. */
  detail?: string;
  /** A shortcut shown on the right, outside the accessible name. */
  keyLabel?: string;
  /** Set on every row of a choice (`menuitemradio`); the chosen row shows a check. */
  checked?: boolean;
}

/** A caption starts a group that runs to the next caption or separator. */
export type PanelEntry = PanelItem | "separator" | { caption: string };

interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** A point (a right-click), or under a button, at least as wide as it. */
export type PanelPlacement = { point: { x: number; y: number } } | { below: Box };

interface Props {
  entries: PanelEntry[];
  placement: PanelPlacement;
  label?: string;
  onClose: () => void;
  /** Takes the focus back on close; defaults to the element focused when the panel opened. */
  opener?: HTMLElement | null;
  /** Presses on it leave the panel to its own click handler, so a second press on a pop-up button closes. */
  owner?: HTMLElement | null;
  mono?: boolean;
}

const EDGE = 4;
const GAP = 4;

const isItem = (entry: PanelEntry): entry is PanelItem => entry !== "separator" && !("caption" in entry);

/** The one menu panel behind every context menu and pop-up button: portaled to `document.body`, so it never takes
 *  the font of the area it opened from. */
export function MenuPanel({ entries, placement, label, onClose, opener: givenOpener, owner, mono }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const [opener] = useState(() => givenOpener ?? (document.activeElement as HTMLElement | null));
  // Placed once per opening; the opener passes a new placement object on every render.
  const [opened] = useState(placement);
  const [style, setStyle] = useState<CSSProperties>(() => startOf(opened));
  const choice = entries.some((e) => isItem(e) && e.checked !== undefined);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setStyle(place(opened, el.getBoundingClientRect()));
  }, [opened]);

  useEffect(() => {
    const items = enabledItems(ref.current);
    (items.find((b) => b.getAttribute("aria-checked") === "true") ?? items[0])?.focus({ preventScroll: true });
    const close = () => onCloseRef.current();
    const outside = (target: EventTarget | null) => !(target instanceof Node && (ref.current?.contains(target) || owner?.contains(target)));
    const onPress = (e: MouseEvent) => outside(e.target) && close();
    const onScroll = (e: Event) => !ref.current?.contains(e.target as Node) && close();
    window.addEventListener("mousedown", onPress);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("mousedown", onPress);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      const active = document.activeElement;
      if ((!active || active === document.body) && opener?.isConnected) opener.focus();
    };
  }, [opener, owner]);

  const onKeyDown = (e: KeyboardEvent) => {
    const items = enabledItems(ref.current);
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const focus = (i: number) => items[i]?.focus({ preventScroll: false });
    switch (e.key) {
      case "ArrowDown":
        focus(at === -1 ? 0 : (at + 1) % items.length);
        break;
      case "ArrowUp":
        focus(at === -1 ? items.length - 1 : (at - 1 + items.length) % items.length);
        break;
      case "Home":
        focus(0);
        break;
      case "End":
        focus(items.length - 1);
        break;
      case "Escape":
        onCloseRef.current();
        opener?.focus();
        break;
      case "Enter":
      case " ":
        // The focused row's own button activation chooses it.
        e.stopPropagation();
        return;
      case "Tab":
        break;
      default: {
        if (e.key.length !== 1 || e.metaKey || e.ctrlKey || e.altKey) return;
        const letter = e.key.toLowerCase();
        const order = [...items.slice(at + 1), ...items.slice(0, at + 1)];
        order.find((b) => b.dataset.label?.toLowerCase().startsWith(letter))?.focus();
      }
    }
    e.preventDefault();
    e.stopPropagation();
  };

  return createPortal(
    <div
      className={mono ? "menu mono" : "menu"}
      role="menu"
      aria-label={label}
      tabIndex={-1}
      data-own-keys
      ref={ref}
      style={style}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      {sectionsOf(entries).map((section, i) => {
        const rows = section.entries.map(([entry, index]) =>
          isItem(entry) ? (
            <Row key={`${index}-${entry.label}`} item={entry} choice={choice} onClose={onClose} />
          ) : (
            <hr key={`separator-${index}`} />
          ),
        );
        return section.caption === undefined ? (
          rows
        ) : (
          <div key={`group-${i}`} role="group" aria-label={section.caption}>
            <div className="menu-caption" aria-hidden>
              {section.caption}
            </div>
            {rows}
          </div>
        );
      })}
    </div>,
    document.body,
  );
}

function Row({ item, choice, onClose }: { item: PanelItem; choice: boolean; onClose: () => void }) {
  let content: ReactNode = item.label;
  if (item.detail) {
    content = (
      <span className="menu-label">
        <span>{item.label}</span>
        <span className="menu-detail">{item.detail}</span>
      </span>
    );
  }
  return (
    <button
      type="button"
      role={item.checked === undefined ? "menuitem" : "menuitemradio"}
      aria-checked={item.checked}
      className={item.detail ? "with-detail" : undefined}
      disabled={item.disabled}
      data-tooltip={item.title}
      data-label={item.label}
      onMouseEnter={(e) => !item.disabled && e.currentTarget.focus({ preventScroll: true })}
      onClick={() => {
        onClose();
        item.onChoose();
      }}
    >
      {choice && <span className="menu-check">{item.checked && <Check size="0.923rem" strokeWidth={2.4} aria-hidden />}</span>}
      {content}
      {item.keyLabel && (
        <span className="menu-key" aria-hidden>
          {item.keyLabel}
        </span>
      )}
    </button>
  );
}

function sectionsOf(entries: PanelEntry[]): { caption?: string; entries: [PanelEntry, number][] }[] {
  const sections: { caption?: string; entries: [PanelEntry, number][] }[] = [{ entries: [] }];
  entries.forEach((entry, index) => {
    const last = sections[sections.length - 1];
    if (entry !== "separator" && "caption" in entry) sections.push({ caption: entry.caption, entries: [] });
    else if (entry === "separator" && last.caption !== undefined) sections.push({ entries: [[entry, index]] });
    else last.entries.push([entry, index]);
  });
  return sections.filter((s) => s.caption !== undefined || s.entries.length > 0);
}

function enabledItems(menu: HTMLElement | null): HTMLButtonElement[] {
  return menu ? [...menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")] : [];
}

function startOf(placement: PanelPlacement): CSSProperties {
  if ("point" in placement) return { left: placement.point.x, top: placement.point.y };
  const box = placement.below;
  return { left: box.left, top: box.bottom + GAP, minWidth: box.right - box.left };
}

/** At its place when it fits, flipped above a button when it doesn't, then kept inside the window. */
function place(placement: PanelPlacement, size: { width: number; height: number }): CSSProperties {
  const room = { width: window.innerWidth, height: window.innerHeight };
  const fitsDown = (top: number) => top + size.height <= room.height - EDGE;
  const start = startOf(placement);
  let top = start.top as number;
  if ("below" in placement && !fitsDown(top)) top = placement.below.top - GAP - size.height;
  return {
    ...start,
    left: Math.max(EDGE, Math.min(start.left as number, room.width - size.width - EDGE)),
    top: Math.max(EDGE, Math.min(top, room.height - size.height - EDGE)),
  };
}
