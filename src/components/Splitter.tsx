import { useEffect, useRef, useState } from "react";
import type { PaneSize } from "../api";
import { selectActiveSession, useStore } from "../state/store";

const KEY_STEP = 16;

const clamp = (v: number, min: number, max: number) => Math.round(Math.min(Math.max(v, min), Math.max(min, max)));

export function useWindowSize() {
  const [size, setSize] = useState({ width: window.innerWidth, height: window.innerHeight });
  useEffect(() => {
    const on = () => setSize({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  return size;
}

/** A pane size kept per connection: live while dragging, persisted on release. */
export function usePaneSize(pane: PaneSize, fallback: number, min: number, max: number) {
  const session = useStore(selectActiveSession);
  const stored = useStore((s) => (session ? s.uiState[session.identity]?.[pane] : null));
  const [live, setLive] = useState<number | null>(null);
  return {
    size: clamp(live ?? stored ?? fallback, min, max),
    min,
    max,
    onResize: setLive,
    onCommit: (px: number) => {
      setLive(null);
      if (session) useStore.getState().setPaneSize(session.identity, pane, clamp(px, min, max));
    },
  };
}

interface Props {
  /** `x` resizes a width (col-resize), `y` a height (row-resize). */
  axis: "x" | "y";
  /** The pane sits after the handle (right or below), so dragging toward it shrinks it. */
  paneAfter: boolean;
  size: number;
  min: number;
  max: number;
  onResize: (px: number) => void;
  onCommit: (px: number) => void;
  label: string;
  /** A double-click: back to the default sizes. */
  onReset?: () => void;
  /** The handle's Stage anchor. */
  stage?: string;
}

/** Drag handle between two panes; arrow keys resize in steps. */
export function Splitter({ axis, paneAfter, size, min, max, onResize, onCommit, label, onReset, stage }: Props) {
  const sign = paneAfter ? -1 : 1;
  // Removes the window listeners and the body cursor class of a drag in progress.
  const stopDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(), []);

  const start = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    stopDrag.current?.();
    const origin = axis === "x" ? e.clientX : e.clientY;
    const next = (ev: MouseEvent) => clamp(size + sign * ((axis === "x" ? ev.clientX : ev.clientY) - origin), min, max);
    const move = (ev: MouseEvent) => onResize(next(ev));
    const up = (ev: MouseEvent) => {
      stopDrag.current?.();
      onCommit(next(ev));
    };
    stopDrag.current = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove(`resizing-${axis}`);
      stopDrag.current = null;
    };
    document.body.classList.add(`resizing-${axis}`);
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    const keys = axis === "x" ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"];
    const dir = keys.indexOf(e.key);
    if (dir < 0) return;
    e.preventDefault();
    onCommit(clamp(size + sign * (dir === 0 ? -KEY_STEP : KEY_STEP), min, max));
  };
  return (
    <div
      className={`splitter splitter-${axis}`}
      role="separator"
      tabIndex={0}
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      aria-label={label}
      aria-valuenow={size}
      aria-valuemin={min}
      aria-valuemax={max}
      data-stage={stage}
      data-stage-label={stage && label}
      onMouseDown={start}
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
    />
  );
}
