/**
 * The anchors Stage sees: elements marked `data-stage="<id>"`, with `data-stage-label`,
 * `data-stage-role`, `data-stage-list`, and `data-stage-window` on a sheet (settings, palette) that
 * Stage sees as a window of its own.
 */
import { isInView, visiblePart } from "./rect";
import type { PageAnchor, StageRect, StageWindow } from "./types";

/** The one label limit of the hook; Rust passes labels through as the page reports them. */
const MAX_LABEL = 80;
export const FIELD = "input:not([type=hidden]):not([type=checkbox]):not([type=radio]), textarea, [contenteditable='true'], [contenteditable='']";

function text(value: string | null | undefined): string {
  return (value ?? "").replace(/\s+/g, " ").trim();
}

export function rectOf(element: Element): StageRect {
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

export function windowOf(element: Element): StageWindow {
  const sheet = element.closest("[data-stage-window]")?.getAttribute("data-stage-window");
  return sheet === "settings" || sheet === "palette" ? sheet : "main";
}

export function isField(element: Element): boolean {
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
  if (value === "true") return true;
  if (value === "false") return false;
  return undefined;
}

/** `role`, `list` and the ARIA states Stage can narrate. */
function detailsOf(element: Element): Pick<PageAnchor, "role" | "list" | "selected" | "checked" | "expanded" | "disabled"> {
  const details: ReturnType<typeof detailsOf> = { role: roleOf(element) };
  const list = element.getAttribute("data-stage-list");
  if (list) details.list = list;
  const current = element.getAttribute("aria-current");
  const isCurrent = current !== null && current !== "" && current !== "false";
  const selected = bool(element.getAttribute("aria-selected")) ?? (isCurrent || undefined);
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
export function clipsOf(element: Element): StageRect[] {
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

/** The first element marked with `id` that has a size. */
export function findAnchor(id: string): HTMLElement | null {
  for (const element of document.querySelectorAll<HTMLElement>(`[data-stage="${CSS.escape(id)}"]`)) {
    if (hasSize(rectOf(element))) return element;
  }
  return null;
}
