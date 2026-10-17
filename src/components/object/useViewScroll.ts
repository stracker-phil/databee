import { useLayoutEffect, useRef, type UIEvent } from "react";
import type { ObjectView } from "../../lib/document";

const atBottom = (el: HTMLElement) => el.scrollTop >= el.scrollHeight - el.clientHeight - 1;

/**
 * Both views share one scroll container, so the view being shown would inherit the other's offset.
 * A restored offset the view cannot reach yet (its grids are still loading) is applied again as it grows,
 * until the reader scrolls, clicks or presses a key (collapsing a group shrinks the view on purpose).
 * A focused Document value is scrolled into view.
 */
export function useViewScroll(view: ObjectView, ready: boolean) {
  const ref = useRef<HTMLDivElement>(null);
  const positions = useRef<Partial<Record<ObjectView, number>>>({});
  const pending = useRef<number | null>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const target = positions.current[view] ?? 0;
    pending.current = target;
    el.scrollTop = target;
    el.querySelector(".row .focus")?.scrollIntoView?.({ block: "nearest" });
    const observer = new ResizeObserver(() => {
      if (pending.current !== null && el.scrollTop < pending.current) el.scrollTop = pending.current;
    });
    for (const child of el.children) observer.observe(child);
    const settle = () => {
      pending.current = null;
    };
    el.addEventListener("pointerdown", settle);
    el.addEventListener("keydown", settle);
    return () => {
      observer.disconnect();
      el.removeEventListener("pointerdown", settle);
      el.removeEventListener("keydown", settle);
    };
    // `ready` re-attaches the observer once the grids have rendered their children.
  }, [view, ready]);

  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    if (pending.current !== null) {
      const clampedByShortContent = atBottom(el) && el.scrollTop < pending.current;
      if (clampedByShortContent) return;
      pending.current = null;
    }
    positions.current[view] = el.scrollTop;
  };

  return { ref, onScroll };
}
