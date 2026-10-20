import { useLayoutEffect, useState, type CSSProperties, type MouseEvent } from "react";
import { createPortal } from "react-dom";

interface Target {
  label: HTMLElement;
  row: HTMLElement;
}

/** The alpha of a computed color (the fourth `rgba` component, or after the `/`), else opaque. */
function alphaOf(color: string): number {
  const alpha = /^rgba\(.*,\s*([\d.]+)\)$/.exec(color) ?? /\/\s*([\d.]+)\)$/.exec(color);
  return alpha ? Number(alpha[1]) : 1;
}

/**
 * The row's background as drawn, opaque so the overlay can leave the row's box: the row's and its ancestors'
 * translucent layers over the first opaque one (the window's vibrancy has none, so `--chrome` stands in).
 */
function backgroundOf(row: HTMLElement): string {
  const layers: string[] = [];
  for (let el: HTMLElement | null = row; el; el = el.parentElement) {
    const color = getComputedStyle(el).backgroundColor;
    const alpha = alphaOf(color);
    if (alpha === 0) continue;
    if (alpha === 1) return [...layers, color].join(", ");
    layers.push(`linear-gradient(${color}, ${color})`);
  }
  return [...layers, "var(--chrome)"].join(", ");
}

function styleOf({ label, row }: Target): CSSProperties {
  const text = getComputedStyle(label);
  const rowStyle = getComputedStyle(row);
  const box = label.getBoundingClientRect();
  const rowBox = row.getBoundingClientRect();
  return {
    left: box.left,
    top: rowBox.top,
    height: rowBox.height,
    paddingTop: box.top - rowBox.top,
    paddingRight: rowStyle.paddingRight,
    borderRadius: `0 ${rowStyle.borderTopRightRadius} ${rowStyle.borderBottomRightRadius} 0`,
    font: text.font,
    letterSpacing: text.letterSpacing,
    color: text.color,
    background: backgroundOf(row),
  };
}

/**
 * While a `rowSelector` row is hovered and its `labelSelector` text is cut off, shows the whole text at once:
 * an overlay in the same font, color and background that continues the row past the edge of its scroll box.
 * It reads the row after the render that the same hover lights (`lit`), and again on later class changes.
 */
export function useOverflowReveal(rowSelector: string, labelSelector: string) {
  const [target, setTarget] = useState<Target | null>(null);
  const [style, setStyle] = useState<CSSProperties>({});

  useLayoutEffect(() => {
    if (!target) return;
    setStyle(styleOf(target));
    const observer = new MutationObserver(() => setStyle(styleOf(target)));
    observer.observe(target.row, { attributes: true, attributeFilter: ["class"], subtree: true });
    const hide = () => setTarget(null);
    window.addEventListener("scroll", hide, true);
    window.addEventListener("blur", hide);
    return () => {
      observer.disconnect();
      window.removeEventListener("scroll", hide, true);
      window.removeEventListener("blur", hide);
    };
  }, [target]);

  const onMouseOver = (e: MouseEvent) => {
    const row = e.target instanceof Element ? e.target.closest<HTMLElement>(rowSelector) : null;
    const label = row?.querySelector<HTMLElement>(labelSelector);
    const cutOff = !!row && !!label && label.scrollWidth > label.clientWidth;
    if (!cutOff) setTarget(null);
    else if (target?.label !== label) setTarget({ label, row });
  };
  const onMouseLeave = () => setTarget(null);

  const element =
    target &&
    createPortal(
      <div className="overflow-reveal" aria-hidden="true" style={style}>
        {target.label.textContent}
      </div>,
      document.body,
    );
  return { onMouseOver, onMouseLeave, element };
}
