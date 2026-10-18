import { useState, type ReactNode } from "react";
import { api, type PresetRow } from "../../api";
import { fail, selectDatabaseAccent, useStore } from "../../state/store";
import { AccentSwatches } from "../Accent";
import type { ProjectContext } from "./context";

function PrefixField({ ctx }: { ctx: ProjectContext }) {
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
    <span className="head-field">
      <label htmlFor="project-prefix" className="o2">
        prefix
      </label>
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
      <span className={error ? "field-error" : "o3"}>{error ?? (suggestedPrefix ? `suggested: ${suggestedPrefix}` : "no suggestion")}</span>
    </span>
  );
}

/** `presets: null` is auto (every preset whose marker tables exist); a switch turns it into an explicit list. */
function nextPresets(presets: PresetRow[], name: string, on: boolean): string[] {
  const applied = presets.filter((p) => p.applied).map((p) => p.name);
  return on ? [...applied.filter((n) => n !== name), name] : applied.filter((n) => n !== name);
}

function PresetSwitch({ preset, ctx }: { preset: PresetRow; ctx: ProjectContext }) {
  const toggle = async (on: boolean) => {
    const failed = await ctx.change({ op: "setPresets", presets: nextPresets(ctx.rules.presets, preset.name, on) });
    if (failed) fail(failed, `Could not switch ${preset.name}`);
  };
  const markers = preset.markerTables.length ? `marker tables ${preset.markerTables.join(", ")}` : "no marker tables";
  return (
    <label className="head-field" title={markers}>
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
      <span>{preset.name}</span>
    </label>
  );
}

/** The database's own accent, which replaces the app's while it is open; personalization, not a rule. */
function AccentField({ ctx }: { ctx: ProjectContext }) {
  const { tab } = ctx;
  const own = useStore((s) => selectDatabaseAccent(s, tab.address.connection, tab.database)) ?? null;
  const appAccent = useStore((s) => s.settings?.accent);
  const set = (accent: string | null) => void useStore.getState().setAccent(tab.sessionId, tab.database, accent);
  return (
    <span className="head-field" data-stage="project:accent" data-stage-label="Accent">
      <span className="o2">accent</span>
      <AccentSwatches value={own} stage="project:accent" small onChange={set} />
      <button className="act" aria-pressed={!own} data-stage="project:accent:default" onClick={() => set(null)}>
        {appAccent ? `default (app accent: ${appAccent})` : "default"}
      </button>
    </span>
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

/** One row: a switch per preset with auto or "back to auto", the prefix, the accent, export, then the section switch (`children`). */
export function ProjectHeader({ ctx, children }: { ctx: ProjectContext; children?: ReactNode }) {
  const auto = ctx.rules.project.presets === null;
  const toAuto = async () => {
    const failed = await ctx.change({ op: "setPresets", presets: null });
    if (failed) fail(failed, "Could not switch presets back to auto");
  };
  return (
    <section className="project-head" aria-label="Presets and prefix">
      <span className="head-presets">
        <span className="o2">presets</span>
        {ctx.rules.presets.map((preset) => (
          <PresetSwitch key={preset.name} preset={preset} ctx={ctx} />
        ))}
        {auto ? (
          <span className="o3" title="every preset whose marker tables exist">
            auto
          </span>
        ) : (
          <button className="act" data-stage="project:presets-auto" onClick={() => void toAuto()}>
            back to auto
          </button>
        )}
      </span>
      <PrefixField key={ctx.rules.project.prefix ?? ""} ctx={ctx} />
      <AccentField ctx={ctx} />
      <button className="act" data-stage="project:export" data-stage-label="Export as preset" onClick={() => void exportPreset(ctx)}>
        Export as preset…
      </button>
      <span className="spacer" />
      {children}
    </section>
  );
}
