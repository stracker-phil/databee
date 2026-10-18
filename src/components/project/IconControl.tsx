/** A table's icon rule with its origin, and the picker that stores a project icon (`setIcon`, `removeIcon`). */
import { useState } from "react";
import type { IconRow, ProjectChange, TableIcon as IconName } from "../../api";
import { ruleOriginText } from "../../lib/ruleForms";
import { fail } from "../../state/store";
import { Modal } from "../Modal";
import { ICON_NAMES, IconGlyph } from "../TableIcon";
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
  const set = (name: IconName) => void send({ op: "setIcon", rule: { table: ruleName, icon: name } });
  const own = icon?.origin.kind === "project" ? icon : null;
  return (
    <Modal onClose={onClose} className="palette rule-sheet icon-sheet" label={`Icon for ${table}`}>
      <div className="sheet-head">
        <span className="sheet-title">Icon</span>
        <span className="o3">{table}</span>
      </div>
      <div className="icon-grid" role="group" aria-label="Icons">
        {ICON_NAMES.map((name) => (
          <button key={name} className="icon-choice" aria-label={name} title={name} aria-pressed={icon?.icon === name} onClick={() => set(name)}>
            <IconGlyph name={name} className="icon-choice-glyph" />
          </button>
        ))}
      </div>
      <div className="form-foot">
        <button className="act o2" aria-pressed={icon?.icon === "none"} title="Store `none`: no icon, whatever the presets give" onClick={() => set("none")}>
          None
        </button>
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

/** `[icon] menu · preset wordpress: *meta  icon…`; the picker stores a project icon for the table's rule name. */
export function IconControl({ table, ruleName, icon, ctx }: Props) {
  const [open, setOpen] = useState(false);
  const shown = icon && icon.icon !== "none" ? icon.icon : null;
  return (
    <span className="icon-rule" role="group" aria-label={`icon of ${table}`}>
      <IconGlyph name={shown} />
      <span className="mono">{icon ? icon.icon : "no icon"}</span>
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
