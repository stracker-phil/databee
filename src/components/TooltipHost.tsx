import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { bindingFor, bindingLabel } from "../commands/registry";
import { useStore } from "../state/store";

/** How long the pointer rests before a tooltip shows. */
const SHOW_DELAY_MS = 450;
/** After one tooltip, the next shows at once for this long (moving along a toolbar). */
const WARM_MS = 600;
const GAP = 6;
const EDGE = 8;
/** How far up from the pointer a cut-off text is looked for (the text sits in a few wrappers at most). */
const CUT_OFF_DEPTH = 4;

interface Tip {
  text: string;
  rect: DOMRect;
}

/** Takes an element's `title` over as `data-tooltip`, so the native tooltip never shows. An element named only
 *  by its title keeps that name as `aria-label`. */
function takeTitle(element: Element) {
  const title = element.getAttribute("title");
  if (!title) return;
  element.removeAttribute("title");
  element.setAttribute("data-tooltip", title);
  if (!element.hasAttribute("aria-label") && !element.textContent?.trim()) element.setAttribute("aria-label", title);
}

/** "Toggle sidebar (⌃⌘S)": a `data-command` control ends its tooltip with the command's binding as it is now. */
function withBinding(element: Element, text: string): string {
  const command = element.getAttribute("data-command");
  const key = command ? bindingLabel(bindingFor(command, useStore.getState().keybindings)) : "";
  return key ? `${text} (${key})` : text;
}

function tooltipText(element: Element): string | null {
  if (cutOff(element)) return element.textContent?.trim() || null;
  takeTitle(element);
  const text = element.getAttribute("data-tooltip");
  return text ? withBinding(element, text) : null;
}

/** Text that doesn't fit: cut with an ellipsis, or clamped to fewer lines. Text that reveals itself on hover
 *  (`data-reveals`, the sidebar's names) is left to that. */
function cutOff(element: Element): boolean {
  if (!(element instanceof HTMLElement) || element.hasAttribute("data-reveals")) return false;
  const style = getComputedStyle(element);
  const clamp = style.getPropertyValue("-webkit-line-clamp");
  if (style.textOverflow === "ellipsis" && element.scrollWidth > element.clientWidth) return true;
  return clamp !== "" && clamp !== "none" && element.scrollHeight > element.clientHeight;
}

function cutOffAround(target: Element): Element | null {
  let element: Element | null = target;
  for (let depth = 0; element && depth < CUT_OFF_DEPTH; depth += 1, element = element.parentElement) {
    if (cutOff(element)) return element;
  }
  return null;
}

const titled = (element: Element) => element.closest("[title], [data-tooltip]");

/** The element whose tooltip shows: a cut-off text shows itself whole, else the nearest title. */
const owner = (target: EventTarget | null) => (target instanceof Element ? (cutOffAround(target) ?? titled(target)) : null);

const HIDING_EVENTS = ["pointerdown", "keydown", "focusout", "wheel"] as const;

/** One tooltip for the whole window: after a short rest, at once while moving between controls, and on keyboard
 *  focus. Any element with a `title` gets it, and any text cut off with an ellipsis shows in full. */
export function TooltipHost() {
  const [tip, setTip] = useState<Tip | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  // Measured before it paints, so the tooltip can keep clear of the window's edges.
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    if (ref.current) setWidth(ref.current.offsetWidth);
  }, [tip]);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let current: Element | null = null;
    let hiddenAt = 0;
    const hide = () => {
      clearTimeout(timer);
      if (current) hiddenAt = Date.now();
      current = null;
      setTip(null);
    };
    const show = (element: Element, delay: number) => {
      const text = tooltipText(element);
      if (!text) return;
      clearTimeout(timer);
      current = element;
      const reveal = () => {
        if (current === element && element.isConnected) setTip({ text, rect: element.getBoundingClientRect() });
      };
      if (delay === 0) reveal();
      else timer = setTimeout(reveal, delay);
    };
    const warm = () => Date.now() - hiddenAt < WARM_MS;
    // Finding cut-off text reads styles and layout, so it waits until the pointer has rested.
    const onOver = (e: PointerEvent) => {
      const target = e.target;
      if (!(target instanceof Element)) return hide();
      const withTitle = target.closest("[title]");
      if (withTitle) takeTitle(withTitle);
      if (current?.contains(target) && titled(target) === titled(current)) return;
      const wasShowing = current !== null;
      hide();
      const reveal = () => {
        const element = owner(target);
        if (element && target.isConnected) show(element, 0);
      };
      if (wasShowing || warm()) reveal();
      else timer = setTimeout(reveal, SHOW_DELAY_MS);
    };
    const onFocus = (e: FocusEvent) => {
      const element = owner(e.target);
      if (element && e.target instanceof Element && e.target.matches(":focus-visible")) show(element, SHOW_DELAY_MS);
    };
    document.addEventListener("pointerover", onOver);
    document.addEventListener("focusin", onFocus);
    for (const event of HIDING_EVENTS) document.addEventListener(event, hide, true);
    window.addEventListener("blur", hide);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("pointerover", onOver);
      document.removeEventListener("focusin", onFocus);
      for (const event of HIDING_EVENTS) document.removeEventListener(event, hide, true);
      window.removeEventListener("blur", hide);
    };
  }, []);

  if (!tip) return null;
  const below = tip.rect.bottom + GAP + 28 < window.innerHeight;
  const half = width / 2;
  const left = Math.min(Math.max(tip.rect.left + tip.rect.width / 2, EDGE + half), window.innerWidth - EDGE - half);
  const style = below ? { left, top: tip.rect.bottom + GAP } : { left, bottom: window.innerHeight - tip.rect.top + GAP };
  return (
    <div ref={ref} className={below ? "tooltip" : "tooltip tooltip--above"} role="tooltip" style={style}>
      {tip.text}
    </div>
  );
}
