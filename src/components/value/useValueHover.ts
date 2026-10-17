import { useEffect, type RefObject } from "react";
import { NO_HOVER, useStore, type ValueHover } from "../../state/store";

function hoverOf(target: EventTarget | null): ValueHover {
  const value = target instanceof Element ? target.closest<HTMLElement>("[data-src]") : null;
  if (!value) return NO_HOVER;
  return { src: value.dataset.src ?? null, table: value.dataset.table ?? null };
}

/**
 * Puts the provenance of the value under the pointer into the store (`hover`), for the status bar. One
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
