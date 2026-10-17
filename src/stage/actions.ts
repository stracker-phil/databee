/** What the page does to an anchor or a sheet when Rust asks: reveal, click, type, scroll, keys. */
import { useStore } from "../state/store";
import { FIELD, clipsOf, isField, rectOf, windowOf } from "./anchors";
import { centerOf, isInView, visiblePart } from "./rect";
import type { StageKeys, StageScrollTo, StageSheetAction, StageTypeMode, StageWindow } from "./types";

/** A smooth scroll is over once the anchor stops moving, or after this long. */
const SCROLL_SETTLE_MS = 800;
const SCROLL_POLL_MS = 50;
/** Hidden windows don't render frames; answer after this long instead. */
const FRAME_FALLBACK_MS = 50;
/** What a click on a wrapper anchor lands on when nothing of it is under its centre. */
const INTERACTIVE =
  "button, a[href], input, select, textarea, summary, [role='button'], [role='tab'], [role='radio'], [role='switch'], [role='checkbox'], [role='option'], [role='menuitem'], [tabindex]:not([tabindex='-1'])";
const FOCUSABLE =
  "button, a[href], input, select, textarea, summary, [tabindex], [contenteditable='true'], [contenteditable='']";

/** A failure with the HTTP status Rust should answer (500 unless given). */
export class StageFailure extends Error {
  constructor(
    message: string,
    readonly status?: number,
  ) {
    super(message);
  }
}

const pause = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** One rendered frame, so an immediate `describe` sees what the action changed. */
export function nextFrame(): Promise<void> {
  return new Promise<void>((resolve) => {
    const done = () => {
      clearTimeout(timer);
      resolve();
    };
    const timer = setTimeout(done, FRAME_FALLBACK_MS);
    requestAnimationFrame(() => requestAnimationFrame(done));
  });
}

/** Waits until the element stops moving (a smooth scroll ended), at most SCROLL_SETTLE_MS. */
async function settle(element: Element): Promise<void> {
  const started = Date.now();
  let last = "";
  while (Date.now() - started < SCROLL_SETTLE_MS) {
    await pause(SCROLL_POLL_MS);
    const now = rectOf(element);
    const position = `${now.x},${now.y}`;
    if (position === last) return;
    last = position;
  }
}

/** Scrolls the anchor into view as a presenter would (smoothly) and waits until it stops moving. */
export async function reveal(anchor: HTMLElement): Promise<void> {
  const rect = rectOf(anchor);
  const clips = clipsOf(anchor);
  if (isInView(rect, clips)) return;
  // Taller than its scroll area: its top comes into view.
  const tall = rect.height > clips[0].height;
  anchor.scrollIntoView({ behavior: "smooth", block: tall ? "start" : "nearest", inline: "nearest" });
  await settle(anchor);
}

/**
 * Like a real click at the anchor's visible centre: the deepest element there (a wrapper's button,
 * not the wrapper) gets pointerdown, mousedown, focus, pointerup, mouseup and click, so controls
 * that open on press work as well as click handlers. As in the browser, a handled pointerdown
 * skips the mouse press events and the focus.
 */
export function click(anchor: HTMLElement): void {
  const rect = rectOf(anchor);
  const point = centerOf(visiblePart(rect, clipsOf(anchor)) ?? rect);
  const hit = document.elementFromPoint(point.x, point.y);
  const target = hit && anchor.contains(hit) ? hit : (anchor.querySelector(INTERACTIVE) ?? anchor);
  if (target.closest(":disabled, [aria-disabled='true']")) throw new StageFailure("is disabled");
  const init = {
    bubbles: true,
    cancelable: true,
    composed: true,
    clientX: point.x,
    clientY: point.y,
    screenX: window.screenX + point.x,
    screenY: window.screenY + point.y,
    button: 0,
  };
  const pointer = (type: string, buttons: number) =>
    target.dispatchEvent(new PointerEvent(type, { ...init, buttons, pointerId: 1, pointerType: "mouse", isPrimary: true }));
  const mouse = (type: string, buttons: number) => target.dispatchEvent(new MouseEvent(type, { ...init, buttons, detail: 1 }));

  const pointerAllowed = pointer("pointerdown", 1);
  const mouseAllowed = pointerAllowed && mouse("mousedown", 1);
  if (mouseAllowed) {
    const focusable = target.closest(FOCUSABLE);
    if (focusable instanceof HTMLElement) focusable.focus();
  }
  pointer("pointerup", 0);
  if (pointerAllowed) mouse("mouseup", 0);
  mouse("click", 0);
}

/** The text field in or at the anchor. */
function fieldOf(element: HTMLElement): HTMLElement | null {
  return isField(element) ? element : element.querySelector<HTMLElement>(FIELD);
}

/**
 * `replace` sets the whole content, `append` adds at the end of a field or the caret of an
 * editor. Inputs get the value through the native setter (React sees the `input` event); editors
 * get the text typed, as CodeMirror expects.
 */
export function typeInto(anchor: HTMLElement, value: string, mode: StageTypeMode): void {
  const field = fieldOf(anchor);
  if (!field) throw new StageFailure("isn't a text field");
  field.focus();
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
    const next = mode === "append" ? field.value + value : value;
    Reflect.set(Object.getPrototypeOf(field) as object, "value", next, field);
    field.setSelectionRange?.(next.length, next.length);
    field.dispatchEvent(new Event("input", { bubbles: true }));
    return;
  }
  if (mode === "replace") document.execCommand("selectAll");
  document.execCommand("insertText", false, value);
}

function scrolls(element: Element): boolean {
  if (!(element instanceof HTMLElement)) return false;
  const style = getComputedStyle(element);
  const can = (overflow: string) => overflow === "auto" || overflow === "scroll";
  if (can(style.overflowY) && element.scrollHeight > element.clientHeight) return true;
  return can(style.overflowX) && element.scrollWidth > element.clientWidth;
}

/** The scroll container at the anchor, inside it, or around it. */
function scrollerOf(anchor: HTMLElement): HTMLElement | null {
  if (scrolls(anchor)) return anchor;
  for (const inner of anchor.querySelectorAll<HTMLElement>("*")) if (scrolls(inner)) return inner;
  for (let outer = anchor.parentElement; outer; outer = outer.parentElement) if (scrolls(outer)) return outer;
  return null;
}

/** Scrolls the anchor's scroll area to `to`, else by `by` points, and waits until it stops. */
export async function scroll(anchor: HTMLElement, target: { by?: number; to?: StageScrollTo }): Promise<void> {
  const area = scrollerOf(anchor);
  if (!area) throw new StageFailure("isn't scrollable", 409);
  if (target.to === "top") area.scrollTo({ top: 0, behavior: "smooth" });
  else if (target.to === "bottom") area.scrollTo({ top: area.scrollHeight, behavior: "smooth" });
  else area.scrollBy({ top: target.by ?? 0, behavior: "smooth" });
  const started = Date.now();
  let last = -1;
  while (Date.now() - started < SCROLL_SETTLE_MS) {
    await pause(SCROLL_POLL_MS);
    if (area.scrollTop === last) return;
    last = area.scrollTop;
  }
}

/** The window that has the keys: the open sheet on top, else the main window. */
export function focusedWindow(): StageWindow {
  const active = document.activeElement;
  if (active && active !== document.body) return windowOf(active);
  const s = useStore.getState();
  if (s.paletteOpen) return "palette";
  return s.dialog?.type === "settings" ? "settings" : "main";
}

export function press(keys: StageKeys, target: HTMLElement): void {
  const init = {
    key: keys.key,
    code: keys.code,
    metaKey: keys.meta,
    shiftKey: keys.shift,
    altKey: keys.alt,
    ctrlKey: keys.ctrl,
    bubbles: true,
    cancelable: true,
  };
  const handled = !target.dispatchEvent(new KeyboardEvent("keydown", init));
  // Synthetic keys have no default action; Enter in a field submits its form, as typed Enter would.
  const plain = !keys.meta && !keys.ctrl && !keys.alt && !keys.shift;
  if (!handled && plain && keys.key === "Enter" && target instanceof HTMLInputElement) target.form?.requestSubmit();
  target.dispatchEvent(new KeyboardEvent("keyup", init));
}

/** Opens or closes a sheet (Rust's `window` verb for settings and the palette). */
export function sheet(window: StageWindow, action: StageSheetAction, section?: string): void {
  const s = useStore.getState();
  const show = action === "show";
  if (window === "palette") {
    s.setPaletteOpen(show);
    return;
  }
  if (window !== "settings") throw new StageFailure(`“${window}” isn't a sheet`, 404);
  if (show) {
    const current = s.dialog?.type === "settings" ? s.dialog.page : undefined;
    s.openDialog({ type: "settings", page: section ?? current });
    return;
  }
  if (s.dialog?.type === "settings") s.closeDialog();
}
