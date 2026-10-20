/**
 * The ⌥ peek (docs/ux.md, Peek): pressing ⌥ alone peeks the peekable element under the pointer. While ⌥ is held,
 * moving onto another peekable element peeks that one, anything else keeps the peek; releasing ⌥ or the window
 * losing focus ends it. One set of window listeners serves every subscriber, so only one peek shows at a time.
 */
import { useSyncExternalStore } from "react";

/** A changed cell's peek wins over the reference inside it. */
function peekable(el: Element | null): Element | null {
  return el?.closest("[data-db-change]") ?? el?.closest("[data-peek], [data-ref]") ?? null;
}

interface State {
  held: boolean;
  target: Element | null;
}

let state: State = { held: false, target: null };
let hovered: Element | null = null;
const listeners = new Set<() => void>();

function set(next: State) {
  if (next.held === state.held && next.target === state.target) return;
  state = next;
  listeners.forEach((l) => l());
}

const onOver = (e: MouseEvent) => {
  hovered = e.target instanceof Element ? e.target : null;
  const next = state.held ? peekable(hovered) : null;
  if (next) set({ held: true, target: next });
};
const onDown = (e: KeyboardEvent) => {
  if (e.key === "Alt" && !e.repeat && !e.metaKey && !e.ctrlKey && !e.shiftKey) set({ held: true, target: peekable(hovered) });
};
const onUp = (e: KeyboardEvent) => {
  if (e.key === "Alt") set({ held: false, target: null });
};
const release = () => set({ held: false, target: null });

// Installed once, not per subscriber: a peek that mounts after ⌥ went down must still see it held.
if (typeof window !== "undefined") {
  window.addEventListener("mouseover", onOver, true);
  window.addEventListener("keydown", onDown, true);
  window.addEventListener("keyup", onUp, true);
  window.addEventListener("blur", release);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Whether ⌥ is held alone, and the element it peeks (null while nothing peekable was under the pointer). */
export function useAltPeek(): State {
  return useSyncExternalStore(subscribe, () => state);
}
