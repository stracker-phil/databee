import { useStore } from "../state/store";

const quoted = (text: string) => `"${text.replace(/["\\]/g, "\\$&")}"`;

/**
 * The linked table highlight in the panes while `hover` is a table link: that table's rows (pinned key cells
 * included), bands and links are lit, and every reference into it from other rows looks hovered; for a sidebar row everything
 * else is dimmed too. Selectors by `data-table`, `data-row-table` and `data-lead-table`, so no component
 * subscribes to the hover.
 */
export function TableFocus() {
  const hover = useStore((s) => s.hover);
  if (!hover.linked || !hover.table) return null;
  const t = quoted(hover.table);
  const lit = `
.main [data-row-table=${t}] { background: var(--lit-solid); --pin-bg: var(--lit-solid); }
.main .sh[data-table=${t}] { background: var(--lit-solid); box-shadow: inset 3px 0 0 var(--accent); }
.main .sh[data-table=${t}] .tn, .main [data-table=${t}]:not(.val, .sh) { color: var(--accent); }
.main .grid-cell:has(> .val[data-lead-table=${t}]):not([data-row-table=${t}] > *) { background: var(--cell); }
.main .val[data-lead-table=${t}]:not([data-row-table=${t}] .val, .grid-cell > .val) { background: var(--cell); border-radius: 3px; padding: 0 3px; margin: 0 -3px; }`;
  const dim = `
.main :is([data-row-table], .readout .row, .attention):not([data-row-table=${t}]) > :not(:has(.val[data-lead-table=${t}], [data-table=${t}]:not(.val))),
.main .sh[data-table]:not([data-table=${t}]),
.main .entity-chip:not(:has([data-table=${t}])) { opacity: var(--dimmed); }`;
  return <style data-testid="table-focus">{hover.sidebar ? lit + dim : lit}</style>;
}
