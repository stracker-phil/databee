/**
 * The page's side of the Stage hook (src-tauri/src/stage): it reports the elements marked
 * `data-stage="<id>"` with their rectangles, and scrolls to, clicks, types into, sends keys to
 * them or runs a command when Rust asks (Stage drives this through the local HTTP API).
 * Nothing here runs unless Rust asks. 
 */
import { api, type PageAnchor, type StageActReply, type StageActRequest, type StageKeys, type StageRect } from "../api";
import { centerOf, isInView, visiblePart } from "../lib/stage";
import { stageCommands, stageState, stageView } from "./state";

const PAGE = "main";
const MAX_LABEL = 80;
/** A smooth scroll is over once the anchor stops moving, or after this long. */
const SCROLL_SETTLE_MS = 800;
const SCROLL_POLL_MS = 50;
/** What a click on a wrapper anchor lands on when nothing of it is under its centre. */
const INTERACTIVE =
  "button, a[href], input, select, textarea, summary, [role='button'], [role='tab'], [role='radio'], [role='switch'], [role='checkbox'], [role='option'], [role='menuitem'], [tabindex]:not([tabindex='-1'])";
const FOCUSABLE =
  "button, a[href], input, select, textarea, summary, [tabindex], [contenteditable='true'], [contenteditable='']";

const pause = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function text(value: string | null | undefined): string {
  return (value ?? "").replace(/\s+/g, " ").trim();
}

function rectOf(element: Element): StageRect {
  const rect = element.getBoundingClientRect();
  return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
}

/** `data-stage-label` (on the element or inside it), aria-label, title, or the text. */
function labelOf(element: Element): string {
  const inner = element.querySelector<HTMLElement>("[data-stage-label]");
  const label =
    text(element.getAttribute("data-stage-label")) ||
    text(inner?.getAttribute("data-stage-label")) ||
    text(element.getAttribute("aria-label")) ||
    text(element.getAttribute("title")) ||
    text(element.textContent);
  return label.length > MAX_LABEL ? `${label.slice(0, MAX_LABEL - 1)}…` : label;
}

function hasSize(rect: StageRect): boolean {
  return rect.width > 0 && rect.height > 0;
}

/** The scroll areas around an element (innermost first), then the window: what can hide it. */
function clipsOf(element: Element): StageRect[] {
  const clips: StageRect[] = [];
  for (
    let parent = element.parentElement;
    parent && parent !== document.body && parent !== document.documentElement;
    parent = parent.parentElement
  ) {
    const style = getComputedStyle(parent);
    if (style.overflowX === "visible" && style.overflowY === "visible") continue;
    if (parent.clientWidth === 0 && parent.clientHeight === 0) continue;
    const box = parent.getBoundingClientRect();
    // The padding box: without borders and scroll bars.
    clips.push({ x: box.x + parent.clientLeft, y: box.y + parent.clientTop, width: parent.clientWidth, height: parent.clientHeight });
  }
  clips.push({ x: 0, y: 0, width: window.innerWidth, height: window.innerHeight });
  return clips;
}

/** Every marked element with a size, the first one per id. */
export function collectAnchors(root: Document | HTMLElement = document): PageAnchor[] {
  const seen = new Set<string>();
  const anchors: PageAnchor[] = [];
  for (const element of root.querySelectorAll<HTMLElement>("[data-stage]")) {
    const id = element.dataset.stage?.trim();
    if (!id || seen.has(id)) continue;
    const rect = rectOf(element);
    if (!hasSize(rect)) continue;
    seen.add(id);
    anchors.push({ id, label: labelOf(element), rect, inView: isInView(rect, clipsOf(element)) });
  }
  return anchors;
}

function findAnchor(id: string): HTMLElement | null {
  for (const element of document.querySelectorAll<HTMLElement>(`[data-stage="${CSS.escape(id)}"]`)) {
    if (hasSize(rectOf(element))) return element;
  }
  return null;
}

/** Scrolls the anchor into view as a presenter would (smoothly) and waits until it stops moving. */
async function reveal(anchor: HTMLElement): Promise<void> {
  const rect = rectOf(anchor);
  const clips = clipsOf(anchor);
  if (isInView(rect, clips)) return;
  // Taller than its scroll area: its top comes into view.
  const tall = rect.height > clips[0].height;
  anchor.scrollIntoView({ behavior: "smooth", block: tall ? "start" : "nearest", inline: "nearest" });
  const started = Date.now();
  let last = "";
  while (Date.now() - started < SCROLL_SETTLE_MS) {
    await pause(SCROLL_POLL_MS);
    const now = rectOf(anchor);
    const position = `${now.x},${now.y}`;
    if (position === last) return;
    last = position;
  }
}

/**
 * Like a real click at the anchor's visible centre: the deepest element there (a wrapper's button,
 * not the wrapper) gets pointerdown, mousedown, focus, pointerup, mouseup and click, so controls
 * that open on press work as well as click handlers. As in the browser, a handled pointerdown
 * skips the mouse press events and the focus.
 */
function click(anchor: HTMLElement): void {
  const rect = rectOf(anchor);
  const point = centerOf(visiblePart(rect, clipsOf(anchor)) ?? rect);
  const hit = document.elementFromPoint(point.x, point.y);
  const target = hit && anchor.contains(hit) ? hit : (anchor.querySelector(INTERACTIVE) ?? anchor);
  if (target.closest(":disabled, [aria-disabled='true']")) throw new Error("is disabled");
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

  const pointerDown = pointer("pointerdown", 1);
  const mouseDown = pointerDown && mouse("mousedown", 1);
  if (mouseDown) {
    const focusable = target.closest(FOCUSABLE);
    if (focusable instanceof HTMLElement) focusable.focus();
  }
  pointer("pointerup", 0);
  if (pointerDown) mouse("mouseup", 0);
  mouse("click", 0);
}

/** The text field in or at the anchor. */
function fieldOf(element: HTMLElement): HTMLElement | null {
  if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement || element.isContentEditable) return element;
  return element.querySelector<HTMLElement>("input:not([type=hidden]), textarea, [contenteditable='true'], [contenteditable='']");
}

/** Inputs get the whole text (React sees an `input` event); editors get it typed at the caret. */
function typeInto(anchor: HTMLElement, value: string): void {
  const field = fieldOf(anchor);
  if (!field) throw new Error("isn't a text field");
  field.focus();
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
    const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(field), "value")?.set;
    if (setter) setter.call(field, value);
    else field.value = value;
    field.dispatchEvent(new Event("input", { bubbles: true }));
    return;
  }
  document.execCommand("insertText", false, value);
}

function press(keys: StageKeys): void {
  const target = document.activeElement instanceof HTMLElement ? document.activeElement : document.body;
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

export async function perform(request: StageActRequest, run: (id: string) => boolean): Promise<StageActReply> {
  const base = { request: request.request, page: PAGE };
  try {
    if (request.kind === "command") {
      const ran = !!request.command && run(request.command);
      return ran ? { ...base, handled: true, ok: true } : { ...base, handled: true, ok: false, error: `“${request.command}” isn't available now` };
    }
    if (request.kind === "keys") {
      if (!document.hasFocus() || !request.keys) return { ...base, handled: false, ok: false };
      press(request.keys);
      return { ...base, handled: true, ok: true };
    }
    const anchor = request.anchor ? findAnchor(request.anchor) : null;
    if (!anchor) return { ...base, handled: false, ok: false };
    await reveal(anchor);
    // Measured before acting: a click may close or replace what it was on.
    const rect = rectOf(anchor);
    if (request.kind === "click") click(anchor);
    else if (request.kind === "type") typeInto(anchor, request.text ?? "");
    return { ...base, handled: true, ok: true, rect };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { ...base, handled: true, ok: false, error: `“${request.anchor}” ${message}` };
  }
}

/** Call once, when the app starts; returns the listeners to remove. */
export function installStageHooks(run: (id: string) => boolean) {
  return [
    api.onStageCollect((request) => void api.stageAnchors(request, collectAnchors()).catch(() => undefined)),
    api.onStageInfo(
      (request) =>
        void api
          .stageInfo({ request, page: PAGE, state: stageState(), view: stageView(), commands: stageCommands() })
          .catch(() => undefined),
    ),
    api.onStageAct(
      (request) =>
        void perform(request, run)
          .then((reply) => api.stageDone(reply))
          .catch(() => undefined),
    ),
  ];
}
