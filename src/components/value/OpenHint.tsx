import { ChevronRight, ExternalLink } from "lucide-react";
import type { ClickEffect } from "./open";

/**
 * The passive indicator of what a click does, shown while the cell, line or link is hovered: `ChevronRight` opens
 * a column (the side panel), `ExternalLink` moves the whole view or opens a tab. Decorative and never clickable;
 * the click itself acts. `edge` overlays the right edge of a value's cell or line; otherwise it follows the text.
 */
export function OpenHint({ effect, edge = false }: { effect: ClickEffect; edge?: boolean }) {
  const Icon = effect === "drill" ? ChevronRight : ExternalLink;
  return (
    <span className={edge ? "val-acts" : "open-hint"} aria-hidden="true" data-open={effect}>
      <Icon size="0.923rem" />
    </span>
  );
}
