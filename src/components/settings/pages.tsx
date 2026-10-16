/** The core settings pages. Modules register their own pages from their install function. */
import { Blocks, FolderOpen, Keyboard, Palette, Plug, ScrollText, SquareTerminal, Table2 } from "lucide-react";
import { useId, type CSSProperties } from "react";
import {
  ACCENTS, api, BUNDLED_FONTS, LOG_LEVELS, SCHEMES, SETTINGS_LIMITS, TEXT_SIZES, type LogLevel, type SchemeId,
} from "../../api";
import { ROW_PADDING_PX, rowPaddingPx } from "../../lib/theme";
import { registerSettingsPage, type SettingsPageProps } from "../../registry/settingsPages";
import { capitalize, Group, NumberField, Row, Segmented, SegmentedButtons, Switch, TextField } from "./controls";
import { ShortcutsPage } from "./ShortcutsPage";

// Segmented buttons take string options; the keys of the px table are the only list of steps.
const ROW_PADDING_STEPS: readonly string[] = Object.keys(ROW_PADDING_PX);

/** A stored scheme id narrowed to a known one; unknown ids (hand-edited settings) show the first scheme. */
const toSchemeId = (id: string): SchemeId => SCHEMES.find((scheme) => scheme.id === id)?.id ?? SCHEMES[0].id;

/** Two swatches per scheme: its content background and the chosen accent in that scheme. */
function SchemePreview({ scheme, accent }: { scheme: SchemeId; accent: string }) {
  return (
    <span className="scheme-preview" data-scheme={scheme} aria-hidden="true">
      <span style={{ background: "var(--bg)", color: "var(--text)" }}>Aa</span>
      <span style={{ background: `var(--a-${accent})` }} />
    </span>
  );
}

function SchemeSelect({ label, value, accent, stage, onChange }: {
  label: string;
  value: SchemeId;
  accent: string;
  stage: string;
  onChange: (id: SchemeId) => void;
}) {
  const id = useId();
  return (
    <Row label={label} htmlFor={id}>
      <div className="scheme-select">
        <SchemePreview scheme={value} accent={accent} />
        <select
          id={id}
          className="input"
          data-stage={stage}
          data-stage-label={label}
          value={value}
          onChange={(e) => onChange(toSchemeId(e.target.value))}
        >
          {SCHEMES.map((scheme) => (
            <option key={scheme.id} value={scheme.id}>{scheme.label}</option>
          ))}
        </select>
      </div>
    </Row>
  );
}

/** One sample row at the chosen padding, drawn like every list row (hairline below). */
function RowPaddingPreview({ step }: { step: number }) {
  return (
    <div className="row-padding-preview" style={{ "--pad": `${rowPaddingPx(step)}px` } as CSSProperties} aria-hidden="true">
      <span>option_name</span>
      <span>siteurl</span>
    </div>
  );
}

function AppearancePage({ s, set }: SettingsPageProps) {
  return (
    <>
      <Group title="Look">
        <Segmented
          label="Theme"
          description="Which scheme applies; System follows macOS."
          options={["system", "light", "dark"] as const}
          value={s.theme}
          stage="appearance:theme"
          onChange={(theme) => set({ theme })}
        />
        <SchemeSelect
          label="Light scheme"
          value={toSchemeId(s.lightScheme)}
          accent={s.accent}
          stage="appearance:light-scheme"
          onChange={(lightScheme) => set({ lightScheme })}
        />
        <SchemeSelect
          label="Dark scheme"
          value={toSchemeId(s.darkScheme)}
          accent={s.accent}
          stage="appearance:dark-scheme"
          onChange={(darkScheme) => set({ darkScheme })}
        />
        <Row label="Accent" description="Keys, links, selection and focus.">
          <div className="accent-swatches">
            {ACCENTS.map((a) => (
              <button
                key={a}
                type="button"
                aria-label={`Accent ${a}`}
                aria-pressed={s.accent === a}
                data-stage={`appearance:accent:${a}`}
                data-stage-list="appearance:accent"
                style={{ background: `var(--a-${a})` }}
                onClick={() => set({ accent: a })}
              />
            ))}
          </div>
        </Row>
        <Row label="Row padding" description="Space above and below every row in grids and lists.">
          <div className="row-padding">
            <SegmentedButtons
              label="Row padding"
              options={ROW_PADDING_STEPS}
              value={String(s.rowPadding)}
              stage="appearance:row-padding"
              onChange={(step) => set({ rowPadding: Number(step) })}
            />
            <RowPaddingPreview step={s.rowPadding} />
          </div>
        </Row>
        <Segmented label="Inspector" description="Where the value inspector and the WordPress detail pane open. Drag the divider to resize." options={["right", "bottom"] as const} value={s.inspectorDock} onChange={(inspectorDock) => set({ inspectorDock })} />
      </Group>
      <Group title="Text">
        <NumberField
          label="UI size"
          description="Sidebar, tabs and labels."
          suffix="px"
          value={s.uiFontSize}
          min={TEXT_SIZES.ui.min}
          max={TEXT_SIZES.ui.max}
          stage="appearance:ui-size"
          onCommit={(uiFontSize) => set({ uiFontSize })}
        />
        <NumberField
          label="Data size"
          description="Table cells, document values and the SQL editor. ⌘= / ⌘- zoom both."
          suffix="px"
          value={s.dataFontSize}
          min={TEXT_SIZES.data.min}
          max={TEXT_SIZES.data.max}
          stage="appearance:data-size"
          onCommit={(dataFontSize) => set({ dataFontSize })}
        />
        <TextField
          label="UI font"
          description="Any installed family; empty uses the bundled font."
          placeholder={BUNDLED_FONTS.ui}
          value={s.fontUi ?? ""}
          stage="appearance:font-ui"
          onCommit={(name) => set({ fontUi: name.trim() || null })}
        />
        <TextField
          label="Mono font"
          description="Every database value. Any installed family; empty uses the bundled font."
          placeholder={BUNDLED_FONTS.mono}
          value={s.fontMono ?? ""}
          stage="appearance:font-mono"
          onCommit={(name) => set({ fontMono: name.trim() || null })}
        />
      </Group>
    </>
  );
}

function QueriesPage({ s, set }: SettingsPageProps) {
  return (
    <Group>
      <NumberField label="Row limit" description="Rows fetched per query in the SQL editor." value={s.rowLimit} {...SETTINGS_LIMITS.rowLimit} onCommit={(rowLimit) => set({ rowLimit })} />
      <NumberField label="Query timeout" description="Queries running longer are stopped." suffix="s" value={s.queryTimeoutSecs} {...SETTINGS_LIMITS.queryTimeoutSecs} onCommit={(queryTimeoutSecs) => set({ queryTimeoutSecs })} />
      <Switch label="Confirm destructive statements" description="Ask before DROP, TRUNCATE and DELETE/UPDATE without WHERE." checked={s.confirmDestructive} onChange={(confirmDestructive) => set({ confirmDestructive })} />
    </Group>
  );
}

function DisplayPage({ s, set }: SettingsPageProps) {
  return (
    <Group>
      <TextField label="NULL label" description="Shown in cells that hold NULL." value={s.nullLabel} onCommit={(nullLabel) => set({ nullLabel })} />
      <NumberField label="Max characters per cell" description="Longer values are truncated in the grid." value={s.maxCellChars} {...SETTINGS_LIMITS.maxCellChars} onCommit={(maxCellChars) => set({ maxCellChars })} />
      <Switch label="Show system databases" description="mysql, sys, information_schema and performance_schema." checked={s.showSystemSchemas} onChange={(showSystemSchemas) => set({ showSystemSchemas })} />
    </Group>
  );
}

function ConnectionsPage({ s, set }: SettingsPageProps) {
  return (
    <Group>
      <NumberField label="Connect timeout" suffix="s" value={s.connectTimeoutSecs} {...SETTINGS_LIMITS.connectTimeoutSecs} onCommit={(connectTimeoutSecs) => set({ connectTimeoutSecs })} />
      <NumberField label="Default port" description="Used when a connection doesn't name one." value={s.defaultPort} {...SETTINGS_LIMITS.defaultPort} onCommit={(defaultPort) => set({ defaultPort })} />
    </Group>
  );
}

function WordPressPage({ s, set }: SettingsPageProps) {
  return (
    <Group>
      <Switch label="Gravatar avatars" description="Loads customer avatars from gravatar.com, which sends a hash of each email address." checked={s.gravatar} onChange={(gravatar) => set({ gravatar })} />
    </Group>
  );
}

function LoggingPage({ s, set }: SettingsPageProps) {
  return (
    <Group>
      <Row label="Log level" htmlFor="settings-log-level" description={s.logLevel === "trace" ? "Trace logs include the SQL text of every query." : "Higher levels write more detail."}>
        <select id="settings-log-level" className="input" value={s.logLevel} onChange={(e) => set({ logLevel: e.target.value as LogLevel })}>
          {LOG_LEVELS.map((l) => (
            <option key={l} value={l}>{capitalize(l)}</option>
          ))}
        </select>
      </Row>
      <Row label="Log files" description="The three most recent files are kept.">
        <button type="button" className="btn" onClick={() => void api.openLogFolder()}>
          <FolderOpen size="1.077rem" /> Open log folder
        </button>
      </Row>
    </Group>
  );
}

export function registerCoreSettingsPages() {
  registerSettingsPage({ id: "appearance", group: "Interface", label: "Appearance", icon: Palette, order: 10, intro: "How DataBee looks. Changes apply right away.", Body: AppearancePage });
  registerSettingsPage({ id: "shortcuts", group: "Interface", label: "Shortcuts", icon: Keyboard, order: 20, intro: "Click a shortcut and press the new keys. Backspace unbinds, Escape cancels.", Body: ShortcutsPage });
  registerSettingsPage({ id: "queries", group: "Database", label: "Queries", icon: SquareTerminal, order: 10, intro: "Limits and safety checks for the SQL editor and table views.", Body: QueriesPage });
  registerSettingsPage({ id: "display", group: "Database", label: "Display", icon: Table2, order: 20, intro: "How values and schemas show up in grids, documents and the sidebar.", Body: DisplayPage });
  registerSettingsPage({ id: "connections", group: "Database", label: "Connections", icon: Plug, order: 30, intro: "Defaults for new connections.", Body: ConnectionsPage });
  registerSettingsPage({ id: "wordpress", group: "WordPress", label: "WordPress", icon: Blocks, order: 10, intro: "The WordPress lens for WooCommerce and WordPress databases.", Body: WordPressPage });
  registerSettingsPage({ id: "logging", group: "System", label: "Logging", icon: ScrollText, order: 20, intro: "What DataBee writes to its log file.", Body: LoggingPage });
}
