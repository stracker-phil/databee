/** `?`: the bound commands of the scopes that were active, most specific scope first, from the registry. */
import { allCommands, bindingFor, bindingLabel, scopeLabel, specificity, type Overrides } from "../commands";
import { useStore } from "../state/store";
import { KeyCap } from "./Field";
import { Modal } from "./Modal";

interface Group {
  scope: string;
  label: string;
  rows: { id: string; title: string; keys: string }[];
}

function shortcutGroups(scopes: string[], custom: Overrides): Group[] {
  const ordered = [...scopes].sort((a, b) => specificity(b) - specificity(a));
  return ordered
    .map((scope) => ({
      scope,
      label: scopeLabel(scope),
      rows: allCommands()
        .filter((c) => c.scope === scope)
        .map((c) => ({ id: c.id, title: c.title, keys: bindingLabel(bindingFor(c.id, custom)) }))
        .filter((row) => row.keys),
    }))
    .filter((group) => group.rows.length > 0);
}

export function ShortcutsDialog({ scopes }: { scopes: string[] }) {
  const close = useStore((s) => s.closeDialog);
  const custom = useStore((s) => s.keybindings);
  return (
    <Modal onClose={close} className="shortcuts" labelledBy="shortcuts-title">
      <h2 id="shortcuts-title">Keyboard shortcuts</h2>
      {shortcutGroups(scopes, custom).map((group) => (
        <section key={group.scope} aria-label={group.label}>
          <h3>{group.label}</h3>
          <dl>
            {group.rows.map((row) => (
              <div key={row.id} className="shortcut-row">
                <dt>{row.title}</dt>
                <dd>
                  <KeyCap keys={row.keys} />
                </dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
      <div className="actions">
        <button data-stage="shortcuts:close" className="btn" onClick={close}>Close</button>
      </div>
    </Modal>
  );
}
