import { useEffect, type RefObject } from "react";
import { NO_HOVER, useStore, type ValueHover } from "../../state/store";

/**
 * The nearest value, table link or row: a value (`.val`) names its provenance, anything else with `data-table`
 * links a table, and a row with `data-row-table` stands for a value of its table wherever it has none.
 */
function hoverOf(target: EventTarget | null): ValueHover {
  const found = target instanceof Element ? target.closest<HTMLElement>("[data-src], [data-table], [data-row-table]") : null;
  if (!found) return NO_HOVER;
  const { src, table, rowTable } = found.dataset;
  return { src: src ?? null, table: table ?? rowTable ?? null, linked: table !== undefined && !found.classList.contains("val") };
}

/**
 * Puts the value or table link under the pointer into the store (`hover`), for the status bar and the
 * linked table highlight. One
 * delegated listener for the whole area, read at most once per frame, so grids add no per-cell handlers.
 */
export function useValueHover(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const root = ref.current;
    if (!root) return;
    let target: EventTarget | null = null;
    let frame = 0;
    const flush = () => {
      frame = 0;
      useStore.getState().setHover(hoverOf(target));
    };
    const schedule = (next: EventTarget | null) => {
      target = next;
      if (!frame) frame = requestAnimationFrame(flush);
    };
    const over = (e: MouseEvent) => schedule(e.target);
    const leave = () => schedule(null);
    root.addEventListener("mouseover", over);
    root.addEventListener("mouseleave", leave);
    return () => {
      root.removeEventListener("mouseover", over);
      root.removeEventListener("mouseleave", leave);
      cancelAnimationFrame(frame);
      useStore.getState().setHover(NO_HOVER);
    };
  }, [ref]);
}
