import { useState } from "react";
import { RotateCcw } from "lucide-react";
import type { Keybindings } from "../../api";
import { bindingFor, bindingFromEvent, bindingLabel, type Command } from "../../commands";
import {
  defaultBinding, groupedCommands, holderOf, shortcutGroupAnchor, shortcutGroupTitle, shortcutRowAnchor, withBinding,
} from "../../commands/shortcuts";
import { useStore } from "../../state/store";
import { Modal } from "../Modal";
import { Group, Row } from "./controls";

function ShortcutField({ value, onChange }: { value: string | null; onChange: (binding: string | null) => void }) {
  const [recording, setRecording] = useState(false);
  return (
    <button
      type="button"
      data-recording={recording || undefined}
      className={`shortcut-field${recording ? " recording" : ""}${value === null && !recording ? " unbound" : ""}`}
      aria-label={recording ? "Press the new shortcut" : "Record shortcut"}
      onClick={(e) => {
        e.currentTarget.focus();
        setRecording(true);
      }}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        e.stopPropagation();
        if (e.key === "Escape") {
          setRecording(false);
          return;
        }
        if (e.key === "Backspace" || e.key === "Delete") {
          onChange(null);
          setRecording(false);
          return;
        }
        const binding = bindingFromEvent(e);
        if (!binding) return;
        onChange(binding);
        setRecording(false);
      }}
    >
      {recording ? "Press keys…" : value ? bindingLabel(value) : "unbound"}
    </button>
  );
}

interface Takeover {
  command: Command;
  holder: Command;
  binding: string;
}

/** Asks before a key moves from the command that has it; Cancel and Esc keep both as they are. */
function ReassignSheet({ takeover, onReassign, onCancel }: { takeover: Takeover; onReassign: () => void; onCancel: () => void }) {
  const key = bindingLabel(takeover.binding);
  return (
    <Modal onClose={onCancel} labelledBy="reassign-title">
      <h2 id="reassign-title">{key} is used by “{takeover.holder.title}”</h2>
      <p className="muted" style={{ margin: 0 }}>
        Give it to “{takeover.command.title}” instead? “{takeover.holder.title}” is left without a shortcut.
      </p>
      <div className="actions">
        <button type="button" className="btn" onClick={onCancel}>Cancel</button>
        <button type="button" className="btn btn-primary" autoFocus onClick={onReassign}>Reassign</button>
      </div>
    </Modal>
  );
}

function ShortcutRow({ command, bindings, onAssign }: { command: Command; bindings: Keybindings; onAssign: (command: Command, binding: string | null) => void }) {
  const current = bindingFor(command.id, bindings);
  const changed = current !== defaultBinding(command.id);
  return (
    <Row label={command.title} anchor={shortcutRowAnchor(command.id)} description={command.menu && changed ? "The menu shows the new shortcut after a restart." : undefined}>
      <div className="shortcut-control">
        {changed ? (
          <button type="button" className="icon-btn tiny" aria-label={`Reset ${command.title}`} data-tooltip="Reset to default" onClick={() => onAssign(command, defaultBinding(command.id))}>
            <RotateCcw size="0.846rem" />
          </button>
        ) : (
          <span className="icon-btn tiny slot-hidden" aria-hidden="true" />
        )}
        <ShortcutField value={current} onChange={(binding) => onAssign(command, binding)} />
      </div>
    </Row>
  );
}

/** Each command's shortcut, grouped by where it works; one key per command (a taken key moves after asking). */
export function ShortcutsPage() {
  const bindings = useStore((s) => s.keybindings);
  const save = useStore((s) => s.saveKeybindings);
  const [takeover, setTakeover] = useState<Takeover | null>(null);

  const assign = (command: Command, binding: string | null) => {
    const holder = binding ? holderOf(bindings, command.id, binding) : undefined;
    if (binding && holder) {
      setTakeover({ command, holder, binding });
      return;
    }
    void save(withBinding(bindings, command.id, binding));
  };
  const reassign = ({ command, holder, binding }: Takeover) => {
    setTakeover(null);
    void save(withBinding(withBinding(bindings, holder.id, null), command.id, binding));
  };

  return (
    <>
      {groupedCommands().map(({ scope, commands }) => (
        <Group key={scope} title={shortcutGroupTitle(scope)} anchor={shortcutGroupAnchor(scope)}>
          {commands.map((command) => (
            <ShortcutRow key={command.id} command={command} bindings={bindings} onAssign={assign} />
          ))}
        </Group>
      ))}
      {takeover && <ReassignSheet takeover={takeover} onReassign={() => reassign(takeover)} onCancel={() => setTakeover(null)} />}
    </>
  );
}
