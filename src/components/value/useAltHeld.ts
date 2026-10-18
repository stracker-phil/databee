import { useSyncExternalStore } from "react";

let held = false;
const listeners = new Set<() => void>();

function set(next: boolean) {
  if (held === next) return;
  held = next;
  listeners.forEach((l) => l());
}

const onDown = (e: KeyboardEvent) => {
  if (e.key === "Alt" && !e.metaKey && !e.ctrlKey && !e.shiftKey) set(true);
};
const onUp = (e: KeyboardEvent) => {
  if (e.key === "Alt") set(false);
};
const release = () => set(false);

function subscribe(listener: () => void) {
  if (listeners.size === 0) {
    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    window.addEventListener("blur", release);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (listeners.size > 0) return;
    window.removeEventListener("keydown", onDown, true);
    window.removeEventListener("keyup", onUp, true);
    window.removeEventListener("blur", release);
    held = false;
  };
}

/** Whether ⌥ is held alone; one set of window listeners shared by every subscriber. */
export function useAltHeld(): boolean {
  return useSyncExternalStore(subscribe, () => held);
}
