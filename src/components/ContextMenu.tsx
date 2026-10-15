import { useEffect, useRef } from "react";

export interface MenuItem {
  label: string;
  action: () => void;
}

export type MenuEntry = MenuItem | "separator";

interface Props {
  x: number;
  y: number;
  items: MenuEntry[];
  onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const left = Math.min(x, window.innerWidth - 200);
  const top = Math.min(y, window.innerHeight - items.length * 26 - 12);
  return (
    <div className="menu" role="menu" ref={ref} style={{ left, top }}>
      {items.map((item, i) =>
        item === "separator" ? (
          <hr key={i} />
        ) : (
          <button
            key={item.label}
            role="menuitem"
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
