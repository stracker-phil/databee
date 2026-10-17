import { useEffect, useLayoutEffect, useRef, useState } from "react";

interface MenuItem {
  label: string;
  action: () => void;
  disabled?: boolean;
  /** The tooltip, e.g. why the item is disabled. */
  title?: string;
}

export type MenuEntry = MenuItem | "separator";

interface Props {
  x: number;
  y: number;
  items: MenuEntry[];
  onClose: () => void;
}

const EDGE = 4;

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

  return (
    <div className="menu" role="menu" ref={ref} style={pos} onKeyDown={onKeyDown}>
      {items.map((item, i) =>
        item === "separator" ? (
          <hr key={`separator-${i}`} />
        ) : (
          <button
            key={`${i}-${item.label}`}
            role="menuitem"
            disabled={item.disabled}
            title={item.title}
            onClick={() => {
              onClose();
              item.action();
            }}
          >
            {item.label}
          </button>
        ),
      )}
    </div>
  );
}

function menuButtons(menu: HTMLElement | null): HTMLButtonElement[] {
  return menu ? [...menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")] : [];
}
