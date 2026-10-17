/**
 * Rows of a scrolling page (object, project, readout). Up to `LONG_LIST` rows render as they are; a longer
 * list mounts only the rows near the viewport of its scroll container, between two spacers that keep its height.
 */
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode, type RefObject } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useRowHeight } from "../lib/theme";

export const LONG_LIST = 200;

/** Viewport the virtualizer assumes until the first layout measurement, so the first frame already has rows. */
const INITIAL_VIEWPORT = { width: 1000, height: 800 };
const OVERSCAN = 20;

interface Props<T> {
  items: readonly T[];
  /** A keyed row; every row of one list has the same height. */
  children: (item: T, index: number) => ReactNode;
  /** Picks the row to scroll into view whenever it changes, e.g. the focused one. */
  revealWhere?: (item: T) => boolean;
}

export function LongList<T>(props: Props<T>) {
  if (props.items.length <= LONG_LIST) return <>{props.items.map((item, i) => props.children(item, i))}</>;
  return <WindowedList {...props} />;
}

function scrollParent(el: HTMLElement): HTMLElement | null {
  for (let node = el.parentElement; node; node = node.parentElement) {
    const { overflowY } = getComputedStyle(node);
    if (overflowY === "auto" || overflowY === "scroll") return node;
  }
  return null;
}

/** Where `el` starts in the scrolled content of `scroller`. */
const offsetIn = (scroller: HTMLElement, el: HTMLElement) => el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;

/** Spans every column when the list sits in a CSS grid. */
const spacer = (height: number): CSSProperties => ({ height, gridColumn: "1 / -1" });

/** Finds the scroll container, the list's offset in it and the height of a rendered row, after every render. */
function useLayout(top: RefObject<HTMLDivElement | null>) {
  const [scroller, setScroller] = useState<HTMLElement | null>(null);
  const [margin, setMargin] = useState(0);
  const [rowHeight, setRowHeight] = useState(0);
  // eslint-disable-next-line react-hooks/exhaustive-deps -- measures after every render; each setter only fires on a change
  useLayoutEffect(() => {
    const el = top.current;
    if (!el) return;
    const parent = scroller ?? scrollParent(el);
    if (parent !== scroller) setScroller(parent);
    if (parent) {
      const offset = offsetIn(parent, el);
      if (Math.abs(offset - margin) >= 1) setMargin(offset);
    }
    const row = el.nextElementSibling;
    const height = row && row.nextElementSibling ? row.getBoundingClientRect().height : 0;
    if (height > 0 && Math.abs(height - rowHeight) >= 0.5) setRowHeight(height);
  });
  return { scroller, margin, rowHeight };
}

function WindowedList<T>({ items, children, revealWhere }: Props<T>) {
  const top = useRef<HTMLDivElement>(null);
  const { scroller, margin, rowHeight } = useLayout(top);
  const fallback = useRowHeight();
  const size = rowHeight || fallback;
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scroller,
    estimateSize: () => size,
    overscan: OVERSCAN,
    initialRect: INITIAL_VIEWPORT,
    scrollMargin: margin,
  });
  useEffect(() => virtualizer.measure(), [size, virtualizer]);
  const reveal = revealWhere ? items.findIndex(revealWhere) : -1;
  useEffect(() => {
    if (reveal >= 0 && scroller) virtualizer.scrollToIndex(reveal, { align: "auto" });
  }, [reveal, scroller, virtualizer]);

  const rows = virtualizer.getVirtualItems();
  const first = rows[0];
  const last = rows[rows.length - 1];
  const total = virtualizer.getTotalSize();
  const before = first ? first.start - margin : 0;
  const after = last ? total - (last.end - margin) : total;
  return (
    <>
      <div ref={top} aria-hidden style={spacer(before)} />
      {rows.map((row) => children(items[row.index], row.index))}
      <div aria-hidden style={spacer(after)} />
    </>
  );
}
