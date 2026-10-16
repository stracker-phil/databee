/**
 * The page's side of the Stage hook (src-tauri/src/stage, Stage protocol 1 §9): it reports the
 * elements marked `data-stage="<id>"` with their rectangles, and reveals, clicks, types into,
 * scrolls or sends keys to them, runs commands and opens sheets when Rust asks. Nothing here runs
 * unless Rust asks.
 *
 * Markup: `data-stage` (id), `data-stage-label`, `data-stage-role`, `data-stage-list`, and
 * `data-stage-window` on a sheet (settings, palette) that Stage sees as a window of its own.
 */
import { api, type PageAnchor, type StageActReply, type StageActRequest, type StageKeys, type StageRect, type StageWindow } from "../api";
import { centerOf, isInView, visiblePart } from "../lib/stage";
import { useStore } from "../state/store";
import { commandForKeys, runStageCommand, stageCommands, stageId } from "./commands";
import { stageState, stageSurfaces, stageUi, stageView } from "./state";

const PAGE = "main";
const MAX_LABEL = 80;
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
const FIELD = "input:not([type=hidden]):not([type=checkbox]):not([type=radio]), textarea, [contenteditable='true'], [contenteditable='']";

const pause = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** One rendered frame, so an immediate `describe` sees what the action changed. */
const nextFrame = () =>
  new Promise<void>((resolve) => {
    const done = () => (clearTimeout(timer), resolve());
    const timer = setTimeout(done, FRAME_FALLBACK_MS);
    requestAnimationFrame(() => requestAnimationFrame(done));
  });

/** A failure with the HTTP status Rust should answer (500 unless given). */
class StageFailure extends Error {
  constructor(
    message: string,
    readonly status?: number,
  ) {
    super(message);
  }
}

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

function windowOf(element: Element): StageWindow {
  const sheet = element.closest("[data-stage-window]")?.getAttribute("data-stage-window");
  return sheet === "settings" || sheet === "palette" ? sheet : "main";
}

function isField(element: Element): boolean {
  return element.matches(FIELD) || (element instanceof HTMLElement && element.isContentEditable);
}

/** `data-stage-role`, else what the element is: a field, toggle, tab, list row, link or button. */
function roleOf(element: Element): string {
  const explicit = element.getAttribute("data-stage-role");
  if (explicit) return explicit;
  const role = element.getAttribute("role");
  if (isField(element)) return "field";
  if (element.matches("input[type=checkbox], input[type=radio], [aria-pressed]") || role === "switch" || role === "checkbox") return "toggle";
  if (role === "tab") return "tab";
  if (element.hasAttribute("data-stage-list")) return "item";
  if (element.matches("a[href]")) return "link";
  if (element.matches("button") || role === "button" || role === "option") return "button";
  return "region";
}

function bool(value: string | null): boolean | undefined {
  return value === "true" ? true : value === "false" ? false : undefined;
}

/** `role`, `list` and the ARIA states Stage can narrate. */
function detailsOf(element: Element): Pick<PageAnchor, "role" | "list" | "selected" | "checked" | "expanded" | "disabled"> {
  const details: ReturnType<typeof detailsOf> = { role: roleOf(element) };
  const list = element.getAttribute("data-stage-list");
  if (list) details.list = list;
  const current = element.getAttribute("aria-current");
  const selected = bool(element.getAttribute("aria-selected")) ?? (current && current !== "false" ? true : undefined);
  if (selected !== undefined) details.selected = selected;
  const checked =
    element instanceof HTMLInputElement && (element.type === "checkbox" || element.type === "radio")
      ? element.checked
      : (bool(element.getAttribute("aria-checked")) ?? bool(element.getAttribute("aria-pressed")));
  if (checked !== undefined) details.checked = checked;
  const expanded = bool(element.getAttribute("aria-expanded"));
  if (expanded !== undefined) details.expanded = expanded;
  if (element.matches(":disabled") || element.getAttribute("aria-disabled") === "true") details.disabled = true;
  return details;
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
    const clips = clipsOf(element);
    anchors.push({
      id,
      label: labelOf(element),
      window: windowOf(element),
      rect,
      inView: isInView(rect, clips),
      visible: visiblePart(rect, clips),
      ...detailsOf(element),
    });
  }
  return anchors;
}

function findAnchor(id: string): HTMLElement | null {
  for (const element of document.querySelectorAll<HTMLElement>(`[data-stage="${CSS.escape(id)}"]`)) {
    if (hasSize(rectOf(element))) return element;
  }
  return null;
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
async function reveal(anchor: HTMLElement): Promise<void> {
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
function click(anchor: HTMLElement): void {
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
  return isField(element) ? element : element.querySelector<HTMLElement>(FIELD);
}

/**
 * `replace` sets the whole content, `append` adds at the end of a field or the caret of an
 * editor. Inputs get the value through the native setter (React sees the `input` event); editors
 * get the text typed, as CodeMirror expects.
 */
function typeInto(anchor: HTMLElement, value: string, mode: "replace" | "append"): void {
  const field = fieldOf(anchor);
  if (!field) throw new StageFailure("isn't a text field");
  field.focus();
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
    const next = mode === "append" ? field.value + value : value;
    const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(field), "value")?.set;
    if (setter) setter.call(field, next);
    else field.value = next;
    field.setSelectionRange?.(next.length, next.length);
    field.dispatchEvent(new Event("input", { bubbles: true }));
    return;
  }
  if (mode === "replace") document.execCommand("selectAll");
  document.execCommand("insertText", false, value);
}

/** The scroll container at the anchor, inside it, or around it. */
function scrollerOf(anchor: HTMLElement): HTMLElement | null {
  const scrolls = (el: Element): boolean => {
    if (!(el instanceof HTMLElement)) return false;
    const style = getComputedStyle(el);
    const can = (overflow: string) => overflow === "auto" || overflow === "scroll";
    return (can(style.overflowY) && el.scrollHeight > el.clientHeight) || (can(style.overflowX) && el.scrollWidth > el.clientWidth);
  };
  if (scrolls(anchor)) return anchor;
  for (const inner of anchor.querySelectorAll<HTMLElement>("*")) if (scrolls(inner)) return inner;
  for (let outer = anchor.parentElement; outer; outer = outer.parentElement) if (scrolls(outer)) return outer;
  return null;
}

async function scroll(anchor: HTMLElement, request: StageActRequest): Promise<void> {
  const area = scrollerOf(anchor);
  if (!area) throw new StageFailure("isn't scrollable", 409);
  if (request.to === "top") area.scrollTo({ top: 0, behavior: "smooth" });
  else if (request.to === "bottom") area.scrollTo({ top: area.scrollHeight, behavior: "smooth" });
  else area.scrollBy({ top: request.by ?? 0, behavior: "smooth" });
  const started = Date.now();
  let last = -1;
  while (Date.now() - started < SCROLL_SETTLE_MS) {
    await pause(SCROLL_POLL_MS);
    if (area.scrollTop === last) return;
    last = area.scrollTop;
  }
}

/** The window that has the keys: the open sheet on top, else the main window. */
function focusedWindow(): StageWindow {
  const active = document.activeElement;
  if (active && active !== document.body) return windowOf(active);
  const s = useStore.getState();
  return s.paletteOpen ? "palette" : s.dialog?.type === "settings" ? "settings" : "main";
}

function press(keys: StageKeys, target: HTMLElement): void {
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
function sheet(request: StageActRequest): void {
  const s = useStore.getState();
  const show = request.action === "show";
  if (request.window === "palette") {
    s.setPaletteOpen(show);
  } else if (request.window === "settings") {
    if (show) s.openDialog({ type: "settings", page: request.section ?? (s.dialog?.type === "settings" ? s.dialog.page : undefined) });
    else if (s.dialog?.type === "settings") s.closeDialog();
  } else {
    throw new StageFailure(`“${request.window}” isn't a sheet`, 404);
  }
}

export async function perform(request: StageActRequest): Promise<StageActReply> {
  const base = { request: request.request, page: PAGE };
  try {
    switch (request.kind) {
      case "window":
        sheet(request);
        await nextFrame();
        return { ...base, handled: true, ok: true };
      case "command": {
        const failure = runStageCommand(request.command ?? "");
        await nextFrame();
        return failure ? { ...base, handled: true, ok: false, ...failure } : { ...base, handled: true, ok: true };
      }
      case "keys": {
        if (!request.keys) return { ...base, handled: false, ok: false };
        const k = request.keys;
        const target = document.activeElement instanceof HTMLElement ? document.activeElement : document.body;
        // A command's shortcut runs it, as its menu accelerator or key handler would.
        const command = commandForKeys({ key: k.key, metaKey: k.meta, ctrlKey: k.ctrl, altKey: k.alt, shiftKey: k.shift, target });
        if (command) {
          void command.run();
          await nextFrame();
          return { ...base, handled: true, ok: true, command: stageId(command.id) };
        }
        if (!request.focused && !document.hasFocus()) return { ...base, handled: false, ok: false };
        const window = focusedWindow();
        press(k, target);
        await nextFrame();
        return { ...base, handled: true, ok: true, window };
      }
    }
    const anchor = request.anchor ? findAnchor(request.anchor) : null;
    if (!anchor) return { ...base, handled: false, ok: false };
    const window = windowOf(anchor);
    if (request.kind === "scroll") {
      await scroll(anchor, request);
    } else {
      await reveal(anchor);
    }
    // Measured before acting: a click may close or replace what it was on.
    const rect = rectOf(anchor);
    const visible = visiblePart(rect, clipsOf(anchor));
    if (request.kind === "click") click(anchor);
    else if (request.kind === "type") typeInto(anchor, request.text ?? "", request.mode ?? "replace");
    if (request.kind === "click" || request.kind === "type") await nextFrame();
    return { ...base, handled: true, ok: true, window, rect, visible };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const status = error instanceof StageFailure ? error.status : undefined;
    return { ...base, handled: true, ok: false, error: request.anchor ? `“${request.anchor}” ${message}` : message, status };
  }
}

/** Call once, when the app starts; returns the listeners to remove. */
export function installStageHooks() {
  return [
    api.onStageCollect((request) => void api.stageAnchors(request, collectAnchors()).catch(() => undefined)),
    api.onStageInfo(
      (request) =>
        void api
          .stageInfo({ request, page: PAGE, state: stageState(), view: stageView(), ui: stageUi(), surfaces: stageSurfaces(), commands: stageCommands() })
          .catch(() => undefined),
    ),
    api.onStageAct(
      (request) =>
        void perform(request)
          .then((reply) => api.stageDone(reply))
          .catch(() => undefined),
    ),
  ];
}
