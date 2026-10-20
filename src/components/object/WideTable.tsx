import { useMemo, type CSSProperties, type ReactNode } from "react";
import { markScrolled } from "../gridHooks";

interface Props {
  /** Column widths in `ch`; the last column also takes the rest of the pane. */
  widths: string[];
  /** The first column holds the rows' primary key: it stays in view while the table scrolls sideways. */
  pinned?: boolean;
  /** A `.row.hdr` header, then the `.row`s. */
  children: ReactNode;
}

function columnVars(widths: string[]): CSSProperties {
  const last = widths.length - 1;
  return {
    "--cols": widths.map((w, i) => (i === last ? `minmax(${w}, 1fr)` : w)).join(" "),
    "--cols-sum": `calc(${widths.join(" + ")})`,
    "--col-count": widths.length,
    "--first-col": widths[0],
  } as CSSProperties;
}

/** Rows wider than the pane scroll sideways in their own box, header included; the pane only scrolls down. */
export function WideTable({ widths, pinned = false, children }: Props) {
  const style = useMemo(() => columnVars(widths), [widths]);
  return (
    <div className={pinned ? "wide-table pinned" : "wide-table"} style={style} onScroll={pinned ? markScrolled : undefined}>
      {children}
    </div>
  );
}
