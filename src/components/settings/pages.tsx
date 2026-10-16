/** The core settings pages. Modules register their own pages from their install function. */
import { Blocks, FolderOpen, Keyboard, Palette, Plug, ScrollText, SquareTerminal, Table2 } from "lucide-react";
import { ACCENTS, api, LOG_LEVELS, SETTINGS_LIMITS, TEXT_SIZES, type LogLevel } from "../../api";
import { registerSettingsPage, type SettingsPageProps } from "../../registry/settingsPages";
import { capitalize, Group, NumberField, Row, Segmented, Switch, TextField } from "./controls";
import { ShortcutsPage } from "./ShortcutsPage";

const ACCENT_VARS: Record<string, string> = Object.fromEntries(ACCENTS.map((a) => [a, `var(--accent-${a})`]));

function AppearancePage({ s, set }: SettingsPageProps) {
  return (
    <>
      <Group title="Look">
        <Segmented label="Theme" options={["system", "light", "dark"] as const} value={s.theme} onChange={(theme) => set({ theme })} />
        <Row label="Accent" description="Buttons, selection and focus rings.">
          <div className="accent-swatches">
            {ACCENTS.map((a) => (
              <button key={a} type="button" aria-label={`Accent ${a}`} aria-pressed={s.accent === a} style={{ background: ACCENT_VARS[a] }} onClick={() => set({ accent: a })} />
            ))}
          </div>
        </Row>
        <Segmented label="Inspector" description="Where the value inspector and the WordPress detail pane open. Drag the divider to resize." options={["right", "bottom"] as const} value={s.inspectorDock} onChange={(inspectorDock) => set({ inspectorDock })} />
        <Segmented label="Density" description="Row height in grids and lists." options={["comfortable", "compact"] as const} value={s.density} onChange={(density) => set({ density })} />
      </Group>
      <Group title="Text size">
        <NumberField label="UI size" description="Sidebar, tabs and labels." suffix="px" value={s.uiFontSize} min={TEXT_SIZES.ui.min} max={TEXT_SIZES.ui.max} onCommit={(uiFontSize) => set({ uiFontSize })} />
        <NumberField label="Data size" description="Table cells, document values and the SQL editor. ⌘= / ⌘- zoom both." suffix="px" value={s.dataFontSize} min={TEXT_SIZES.data.min} max={TEXT_SIZES.data.max} onCommit={(dataFontSize) => set({ dataFontSize })} />
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
