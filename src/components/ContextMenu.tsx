import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

interface MenuItem {
  label: string;
  action: () => void;
  disabled?: boolean;
  /** The tooltip, e.g. why the item is disabled. */
  title?: string;
  /** A secondary line under the label. */
  detail?: string;
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

const EDGE = 4;

/** The app's only context menu: rendered into `document.body`, so it never takes the font of the area it opened from. */
export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const [pos, setPos] = useState({ left: x, top: y });

  // Measured after render, so separators and long labels keep the menu inside the window.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPos({
      left: Math.max(EDGE, Math.min(x, window.innerWidth - width - EDGE)),
      top: Math.max(EDGE, Math.min(y, window.innerHeight - height - EDGE)),
    });
  }, [x, y]);

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    menuButtons(ref.current)[0]?.focus();
    const closeOutside = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onCloseRef.current();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onCloseRef.current();
    window.addEventListener("mousedown", closeOutside);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", closeOutside);
      window.removeEventListener("keydown", onKey);
      const active = document.activeElement;
      if ((!active || active === document.body) && opener?.isConnected) opener.focus();
    };
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const buttons = menuButtons(ref.current);
    if (buttons.length === 0) return;
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const step = e.key === "ArrowDown" ? 1 : -1;
    buttons[(i + step + buttons.length) % buttons.length].focus();
  };

  return createPortal(
    <div
      className="menu"
      role="menu"
      ref={ref}
      style={pos}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      {items.map((item, i) =>
        item === "separator" ? (
          <hr key={`separator-${i}`} />
        ) : (
          <button
            key={`${i}-${item.label}`}
            role="menuitem"
            className={item.detail ? "with-detail" : undefined}
            disabled={item.disabled}
            title={item.title}
            onClick={() => {
              onClose();
              item.action();
            }}
          >
            {item.detail ? (
              <>
                <span>{item.label}</span>
                <span className="menu-detail">{item.detail}</span>
              </>
            ) : (
              item.label
            )}
          </button>
        ),
      )}
    </div>,
    document.body,
  );
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

function menuButtons(menu: HTMLElement | null): HTMLButtonElement[] {
  return menu ? [...menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")] : [];
}
