import { useLayoutEffect, useState, type RefObject } from "react";

/** An element's width; a hidden tab measures 0, which keeps the last width so what depends on it stays as it was. */
export function useWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(Infinity);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => el.offsetWidth > 0 && setWidth(el.offsetWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}
