import { useState } from "react";
import { api, type PresetRow } from "../../api";
import { fail, useStore } from "../../state/store";
import type { ProjectContext } from "./context";

function PrefixRow({ ctx }: { ctx: ProjectContext }) {
  const { project, prefix, suggestedPrefix } = ctx.rules;
  const [text, setText] = useState(project.prefix ?? "");
  const [error, setError] = useState<string | null>(null);
  const commit = async () => {
    const next = text.trim() || null;
    if (next === project.prefix) return;
    const failed = await ctx.change({ op: "setPrefix", prefix: next });
    setError(failed?.message ?? null);
  };
  return (
    <div className="row prefix-row">
      <label htmlFor="project-prefix">prefix</label>
      <input
        id="project-prefix"
        className="input mono"
        data-stage="project:prefix"
        data-stage-label="Prefix"
        value={text}
        placeholder={prefix || "none"}
        aria-invalid={!!error}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => void commit()}
        onKeyDown={(e) => e.key === "Enter" && void commit()}
      />
      <span className="o3">{error ?? (suggestedPrefix ? `suggested: ${suggestedPrefix}` : "no suggestion")}</span>
    </div>
  );
}

/** `presets: null` is auto (every preset whose marker tables exist); a switch turns it into an explicit list. */
function nextPresets(presets: PresetRow[], name: string, on: boolean): string[] {
  const applied = presets.filter((p) => p.applied).map((p) => p.name);
  return on ? [...applied.filter((n) => n !== name), name] : applied.filter((n) => n !== name);
}

function PresetLine({ preset, ctx }: { preset: PresetRow; ctx: ProjectContext }) {
  const toggle = async (on: boolean) => {
    const failed = await ctx.change({ op: "setPresets", presets: nextPresets(ctx.rules.presets, preset.name, on) });
    if (failed) fail(failed, `Could not switch ${preset.name}`);
  };
  return (
    <div className="row preset-row">
      <span>{preset.name}</span>
      <span className="o3">{preset.markerTables.length ? `marker ${preset.markerTables.join(", ")}` : "no marker tables"}</span>
      <input
        className="switch"
        type="checkbox"
        role="switch"
        aria-label={`Preset ${preset.name}`}
        data-stage={`project:preset:${preset.name}`}
        data-stage-label={`Preset ${preset.name}`}
        data-stage-list="presets"
        checked={preset.applied}
        onChange={(e) => void toggle(e.target.checked)}
      />
    </div>
  );
}

async function exportPreset(ctx: ProjectContext) {
  const { tab } = ctx;
  try {
    const path = await api.chooseSavePath(`${tab.address.connection}-${tab.database}.toml`, "toml");
    if (!path) return;
    await api.projectExportTo(tab.sessionId, tab.database, path);
    useStore.getState().toast("success", `Exported the project to ${path}`);
  } catch (e) {
    fail(e, "Could not export the project");
  }
}

export function PresetsSection({ ctx }: { ctx: ProjectContext }) {
  const auto = ctx.rules.project.presets === null;
  const toAuto = async () => {
    const failed = await ctx.change({ op: "setPresets", presets: null });
    if (failed) fail(failed, "Could not switch presets back to auto");
  };
  return (
    <section aria-label="Presets and prefix">
      <div className="sh">
        <span className="tn sans">presets and prefix</span>
        <span className="via">{auto ? "auto: every preset whose marker tables exist" : "chosen by hand"}</span>
        <span className="spacer" />
        {!auto && (
          <button className="act" data-stage="project:presets-auto" onClick={() => void toAuto()}>
            back to auto
          </button>
        )}
        <button className="act" data-stage="project:export" data-stage-label="Export as preset" onClick={() => void exportPreset(ctx)}>
          Export as preset…
        </button>
      </div>
      <PrefixRow key={ctx.rules.project.prefix ?? ""} ctx={ctx} />
      {ctx.rules.presets.map((preset) => (
        <PresetLine key={preset.name} preset={preset} ctx={ctx} />
      ))}
    </section>
  );
}
