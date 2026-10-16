import { useState } from "react";
import { RotateCcw } from "lucide-react";
import { allCommands, bindingFor, bindingFromEvent, bindingLabel, conflicts, type Command } from "../../commands";
import { useStore } from "../../state/store";
import { Group, Row } from "./controls";

const SCOPE_LABEL: Record<Command["scope"], string> = { global: "Everywhere", wp: "WordPress", db: "Database", table: "Table", editor: "Editor", document: "Document" };

function ShortcutField({ value, onChange }: { value: string | null; onChange: (binding: string | null) => void }) {
  const [recording, setRecording] = useState(false);
  return (
    <button
      type="button"
      data-recording={recording || undefined}
      className={`shortcut-field${recording ? " recording" : ""}${value === null && !recording ? " unbound" : ""}`}
      aria-label={recording ? "Press the new shortcut" : "Record shortcut"}
      onClick={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        e.stopPropagation();
        if (e.key === "Escape") return setRecording(false);
        if (e.key === "Backspace" || e.key === "Delete") {
          onChange(null);
          return setRecording(false);
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

export function ShortcutsPage() {
  const bindings = useStore((s) => s.keybindings);
  const save = useStore((s) => s.saveKeybindings);
  const [search, setSearch] = useState("");
  const needle = search.trim().toLowerCase();
  const list = allCommands().filter((c) => !needle || c.title.toLowerCase().includes(needle) || c.id.toLowerCase().includes(needle));
  const clashes = conflicts(bindings);
  const set = (id: string, binding: string | null) => void save({ ...bindings, [id]: binding });
  const reset = (id: string) => {
    const { [id]: _gone, ...rest } = bindings;
    void save(rest);
  };
  return (
    <>
      <input className="input settings-search" aria-label="Search shortcuts" placeholder="Search commands…" value={search} onChange={(e) => setSearch(e.target.value)} />
      {clashes.length > 0 && (
        <div className="shortcut-conflicts" role="alert">
          {clashes.map((c) => (
            <div key={`${c.a.id}:${c.b.id}`}>
              {bindingLabel(c.binding)} is used by both “{c.a.title}” and “{c.b.title}”.
            </div>
          ))}
        </div>
      )}
      <Group>
        {list.length === 0 && <div className="settings-row muted">No matching commands.</div>}
        {list.map((c) => {
          const overridden = Object.prototype.hasOwnProperty.call(bindings, c.id);
          return (
            <Row key={c.id} label={c.title} description={c.menu && overridden ? `${SCOPE_LABEL[c.scope]} · menu shortcut applies after restart` : SCOPE_LABEL[c.scope]}>
              <div className="shortcut-control">
                {overridden && (
                  <button type="button" className="icon-btn tiny" aria-label={`Reset ${c.title}`} title="Reset to default" onClick={() => reset(c.id)}>
                    <RotateCcw size="0.846rem" />
                  </button>
                )}
                <ShortcutField value={bindingFor(c.id, bindings)} onChange={(b) => set(c.id, b)} />
              </div>
            </Row>
          );
        })}
      </Group>
    </>
  );
}
