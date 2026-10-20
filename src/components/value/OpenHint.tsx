import type { ReactNode } from "react";
import { ChevronRight, ExternalLink } from "lucide-react";
import type { ClickEffect } from "./open";

const EFFECT_ICONS = { drill: ChevronRight, go: ExternalLink } as const;

/**
 * The passive indicator of what a click does: `ChevronRight` opens a column (the side panel), `ExternalLink` moves
 * the whole view or opens a tab. Decorative and never clickable; the click itself acts. It follows a link's text
 * and shows while its parent is hovered; links with a leading icon use `OpenSwap` instead.
 */
export function OpenHint({ effect }: { effect: ClickEffect }) {
  const Icon = EFFECT_ICONS[effect];
  return (
    <span className="open-hint" aria-hidden="true" data-open={effect}>
      <Icon size="0.923rem" />
    </span>
  );
}

/**
 * A link's leading icon (`children`: a table icon, the serialized marker) in a box of 1em that shows the indicator
 * of `effect` in its place while its cell, line or link is hovered (CSS only, `styles/app.css`); just `children`
 * without an effect.
 */
export function OpenSwap({ effect, children }: { effect: ClickEffect | null; children: ReactNode }) {
  if (!effect) return <>{children}</>;
  const Icon = EFFECT_ICONS[effect];
  return (
    <span className="open-swap" aria-hidden="true" data-open={effect}>
      {children}
      <Icon className="open-glyph" size="1em" />
    </span>
  );
}
