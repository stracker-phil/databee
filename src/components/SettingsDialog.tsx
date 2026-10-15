import { useState } from "react";
import { FolderOpen } from "lucide-react";
import { ACCENTS, api, type Settings } from "../api";
import { useStore } from "../state/store";
import { Modal } from "./Modal";

const ACCENT_VARS: Record<string, string> = Object.fromEntries(ACCENTS.map((a) => [a, `var(--accent-${a})`]));

function NumberField({ label, value, onChange, min, max, suffix }: { label: string; value: number; onChange: (v: number) => void; min: number; max: number; suffix?: string }) {
  return (
    <label className="field">
      <span>{label}{suffix ? ` (${suffix})` : ""}</span>
      <input
        className="input"
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}

export function SettingsDialog() {
  const current = useStore((s) => s.settings);
  const save = useStore((s) => s.saveSettings);
  const close = useStore((s) => s.closeDialog);
  const [draft, setDraft] = useState<Settings | null>(current);
  if (!draft) return null;

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => setDraft({ ...draft, [key]: value });

  return (
    <Modal onClose={close} className="wide" labelledBy="settings-title">
      <h2 id="settings-title">Settings</h2>

      <section>
        <h3>Appearance</h3>
        <div className="field-row">
          <div className="field">
            <span>Theme</span>
            <div className="segmented">
              {(["system", "light", "dark"] as const).map((t) => (
                <button key={t} type="button" aria-pressed={draft.theme === t} onClick={() => set("theme", t)}>
                  {t[0].toUpperCase() + t.slice(1)}
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <span>Density</span>
            <div className="segmented">
              {(["comfortable", "compact"] as const).map((d) => (
                <button key={d} type="button" aria-pressed={draft.density === d} onClick={() => set("density", d)}>
                  {d[0].toUpperCase() + d.slice(1)}
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="field-row">
          <div className="field">
            <span>Accent</span>
            <div className="accent-swatches">
              {ACCENTS.map((a) => (
                <button
                  key={a}
                  type="button"
                  aria-label={`Accent ${a}`}
                  aria-pressed={draft.accent === a}
                  style={{ background: ACCENT_VARS[a] }}
                  onClick={() => set("accent", a)}
                />
              ))}
            </div>
          </div>
        </div>
        <div className="field-row">
          <NumberField label="UI size" suffix="px" value={draft.uiFontSize} min={10} max={24} onChange={(v) => set("uiFontSize", v)} />
          <NumberField label="Data size" suffix="px" value={draft.dataFontSize} min={9} max={28} onChange={(v) => set("dataFontSize", v)} />
        </div>
        <div className="hint">UI size scales sidebar, tabs and labels; data size scales table cells, document values and the SQL editor. ⌘= / ⌘- zoom both.</div>
      </section>

      <section>
        <h3>Queries</h3>
        <div className="field-row">
          <NumberField label="Row limit" value={draft.rowLimit} min={10} max={100000} onChange={(v) => set("rowLimit", v)} />
          <NumberField label="Query timeout" suffix="s" value={draft.queryTimeoutSecs} min={1} max={3600} onChange={(v) => set("queryTimeoutSecs", v)} />
        </div>
        <label className="check">
          <input type="checkbox" checked={draft.confirmDestructive} onChange={(e) => set("confirmDestructive", e.target.checked)} />
          Ask before DROP, TRUNCATE and DELETE/UPDATE without WHERE
        </label>
      </section>

      <section>
        <h3>Display</h3>
        <div className="field-row">
          <label className="field">
            <span>NULL label</span>
            <input className="input" value={draft.nullLabel} onChange={(e) => set("nullLabel", e.target.value)} />
          </label>
          <NumberField label="Max characters per cell" value={draft.maxCellChars} min={20} max={100000} onChange={(v) => set("maxCellChars", v)} />
        </div>
        <label className="check">
          <input type="checkbox" checked={draft.showSystemSchemas} onChange={(e) => set("showSystemSchemas", e.target.checked)} />
          Show system databases (mysql, sys, …)
        </label>
      </section>

      <section>
        <h3>Connections & logging</h3>
        <div className="field-row">
          <NumberField label="Connect timeout" suffix="s" value={draft.connectTimeoutSecs} min={1} max={120} onChange={(v) => set("connectTimeoutSecs", v)} />
          <NumberField label="Default port" value={draft.defaultPort} min={1} max={65535} onChange={(v) => set("defaultPort", v)} />
        </div>
        <div className="field-row" style={{ alignItems: "flex-end" }}>
          <label className="field">
            <span>Log level</span>
            <select className="input" value={draft.logLevel} onChange={(e) => set("logLevel", e.target.value as Settings["logLevel"])}>
              {(["error", "warn", "info", "debug", "trace"] as const).map((l) => (
                <option key={l} value={l}>{l}</option>
              ))}
            </select>
          </label>
          <button type="button" className="btn" onClick={() => void api.openLogFolder()}>
            <FolderOpen size="1.077rem" /> Open log folder
          </button>
        </div>
        {draft.logLevel === "trace" && <div className="hint">Trace logs include the SQL text of every query.</div>}
      </section>

      <div className="actions">
        <button type="button" className="btn" onClick={close}>Cancel</button>
        <button
          type="button"
          className="btn btn-primary"
          onClick={async () => {
            await save(draft);
            close();
          }}
        >
          Save
        </button>
      </div>
    </Modal>
  );
}
