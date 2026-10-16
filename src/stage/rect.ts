/** Rectangle math of the Stage hook (docs/stage.md): visible parts and centers of anchors. */
import type { StageRect } from "./types";

const area = (rect: StageRect) => rect.width * rect.height;

/** Where two rectangles overlap; null when they don't. */
export function intersectRect(a: StageRect, b: StageRect): StageRect | null {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const right = Math.min(a.x + a.width, b.x + b.width);
  const bottom = Math.min(a.y + a.height, b.y + b.height);
  return right > x && bottom > y ? { x, y, width: right - x, height: bottom - y } : null;
}

/** The part of an element its scroll areas and the window (`clips`) let through; null when none. */
export function visiblePart(rect: StageRect, clips: readonly StageRect[]): StageRect | null {
  return clips.reduce<StageRect | null>((part, clip) => (part ? intersectRect(part, clip) : null), rect);
}

/**
 * In view: at least half of the element shows or, when it is larger than the area it scrolls in,
 * it fills at least half of that area. `clips`: the scroll areas around it, innermost first, then
 * the window.
 */
export function isInView(rect: StageRect, clips: readonly StageRect[]): boolean {
  const part = visiblePart(rect, clips);
  if (!part) return false;
  const [inner, ...outer] = clips;
  const room = inner ? visiblePart(inner, outer) : rect;
  return area(part) >= 0.5 * Math.min(area(rect), room ? area(room) : 0);
}

export function centerOf(rect: StageRect): { x: number; y: number } {
  return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
}
