/** A table's icon rule with its origin, and the picker that stores a project icon (`setIcon`, `removeIcon`). */
import { useState, type KeyboardEvent } from "react";
import { Search } from "lucide-react";
import type { IconRow, ProjectChange, TableIcon as IconName } from "../../api";
import { ruleOriginText } from "../../lib/ruleForms";
import { fail } from "../../state/store";
import { Modal } from "../Modal";
import { InsetField } from "../Field";
import { DEFAULT_ICON, ICON_NAMES, IconGlyph } from "../TableIcon";
import type { ProjectContext } from "./context";

interface Props {
  /** The real table. */
  table: string;
  /** The name rules write it as; `null` outside the prefixes, where no rule can name it. */
  ruleName: string | null;
  icon: IconRow | null;
  /** `null` until the project's rules are loaded. */
  ctx: ProjectContext | null;
}

function IconPicker({ table, ruleName, icon, ctx, onClose }: Props & { ruleName: string; ctx: ProjectContext; onClose: () => void }) {
  const send = async (change: ProjectChange) => {
    onClose();
    const failed = await ctx.change(change);
    if (failed) fail(failed, "Could not store the icon");
  };
  const [query, setQuery] = useState("");
  const set = (name: IconName) => void send({ op: "setIcon", rule: { table: ruleName, icon: name } });
  const own = icon?.origin.kind === "project" ? icon : null;
  const shown = icon?.icon ?? DEFAULT_ICON;
  const needle = query.trim().toLowerCase();
  const matches = ICON_NAMES.filter((name) => name.includes(needle));
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && matches.length > 0) set(matches[0]);
  };
  return (
    <Modal onClose={onClose} variant="panel" className="palette rule-sheet icon-sheet" label={`Icon for ${table}`}>
      <div className="sheet-head">
        <span className="sheet-title">Icon</span>
        <span className="o3">{table}</span>
      </div>
      <div className="icon-filter">
        <InsetField icon={<Search size="0.846rem" />}>
          <input autoFocus placeholder="Filter icons" aria-label="Filter icons" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={onKeyDown} />
        </InsetField>
      </div>
      <div className="icon-grid" role="group" aria-label="Icons">
        {matches.map((name) => (
          <button key={name} className="icon-choice" aria-label={name} title={name} aria-pressed={shown === name} onClick={() => set(name)}>
            <IconGlyph name={name} className="icon-choice-glyph" />
          </button>
        ))}
        {matches.length === 0 && <span className="o3 icon-none">No icon matches</span>}
      </div>
      <div className="form-foot">
        {own && (
          <button className="act o2" onClick={() => void send({ op: "removeIcon", table: own.pattern })}>
            {own.shipped ? "Reset to preset" : `Remove ${own.pattern}`}
          </button>
        )}
        <span className="spacer" />
        <button className="act o2" onClick={onClose}>
          Cancel <span className="kbd" aria-hidden="true">⎋</span>
        </button>
      </div>
    </Modal>
  );
}

/**
 * `[icon] menu · preset wordpress: *meta  icon…`, or `[icon] table (default)` without a rule; the picker stores a
 * project icon for the table's rule name.
 */
export function IconControl({ table, ruleName, icon, ctx }: Props) {
  const [open, setOpen] = useState(false);
  return (
    <span className="icon-rule" role="group" aria-label={`icon of ${table}`}>
      <IconGlyph name={icon?.icon ?? DEFAULT_ICON} />
      <span className="mono">{icon ? icon.icon : `${DEFAULT_ICON} (default)`}</span>
      {icon && (
        <span className="origin">
          {ruleOriginText(icon.origin)}: <span className="mono">{icon.pattern}</span>
          {icon.shipped && ` · edited in project, was ${icon.shipped.icon}`}
        </span>
      )}
      {ruleName && (
        <button className="link" disabled={!ctx} title="Choose the table's icon (stored in the project)" onClick={() => setOpen(true)}>
          icon…
        </button>
      )}
      {open && ctx && ruleName && <IconPicker table={table} ruleName={ruleName} icon={icon} ctx={ctx} onClose={() => setOpen(false)} />}
    </span>
  );
}
