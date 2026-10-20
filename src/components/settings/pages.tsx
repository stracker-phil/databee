/** The core settings pages. Modules register their own pages from their install function. */
import { FolderOpen, Keyboard, Palette, Plug, ScrollText, SquareTerminal, Table2 } from "lucide-react";
import { useId, type CSSProperties } from "react";
import {
  api, BUNDLED_FONTS, LOG_LEVELS, SCHEMES, SETTINGS_LIMITS, TEXT_SIZES, type LogLevel, type SchemeId,
} from "../../api";
import { ROW_PADDING_PX, rowPaddingPx } from "../../lib/theme";
import { registerSettingsPage, type SettingsPageProps } from "../../registry/settingsPages";
import { selectActiveSession, selectDatabaseAccent, useStore } from "../../state/store";
import { AccentSwatches } from "../Accent";
import { ConnectionSlugs } from "./ConnectionSlugs";
import { capitalize, Group, NumberField, Row, Segmented, SegmentedButtons, Switch, TextField } from "./controls";
import { ShortcutsPage } from "./ShortcutsPage";

// Segmented buttons take string options; the keys of the px table are the only list of steps.
const ROW_PADDING_STEPS: readonly string[] = Object.keys(ROW_PADDING_PX);
const { min: MIN_PANES, max: MAX_PANES } = SETTINGS_LIMITS.maxPanes;
const MAX_PANES_STEPS = Array.from({ length: MAX_PANES - MIN_PANES + 1 }, (_, i) => String(MIN_PANES + i));

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

/** The schemes of the slot's own kind come first. */
function SchemeSelect({ label, dark, value, accent, stage, onChange }: {
  label: string;
  dark: boolean;
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
          {[dark, !dark].map((groupDark) => (
            <optgroup key={String(groupDark)} label={groupDark ? "Dark" : "Light"}>
              {SCHEMES.filter((scheme) => scheme.dark === groupDark).map((scheme) => (
                <option key={scheme.id} value={scheme.id}>{scheme.label}</option>
              ))}
            </optgroup>
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

/** The app's accent, the default of every database; says so when the open database has its own. */
function AccentRow({ accent, onChange }: { accent: string; onChange: (accent: string) => void }) {
  const session = useStore(selectActiveSession);
  const own = useStore((st) => (session ? selectDatabaseAccent(st, session.slug, session.database) : undefined));
  const description = "Keys, links, selection and focus. The default accent; a database can override it on its Project page."
    + (own && session?.database ? ` ${session.database} uses its own: ${own}.` : "");
  return (
    <Row label="Accent" description={description}>
      <AccentSwatches value={accent} stage="appearance:accent" onChange={onChange} />
    </Row>
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
          dark={false}
          value={toSchemeId(s.lightScheme)}
          accent={s.accent}
          stage="appearance:light-scheme"
          onChange={(lightScheme) => set({ lightScheme })}
        />
        <SchemeSelect
          label="Dark scheme"
          dark
          value={toSchemeId(s.darkScheme)}
          accent={s.accent}
          stage="appearance:dark-scheme"
          onChange={(darkScheme) => set({ darkScheme })}
        />
        <AccentRow accent={s.accent} onChange={(accent) => set({ accent })} />
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
      </Group>
      <Group title="Layout">
        <Segmented
          label="Panes side by side"
          description="Columns of an object tab shown next to each other at most; fewer when the window is narrow."
          options={MAX_PANES_STEPS}
          value={String(s.maxPanes)}
          stage="appearance:max-panes"
          onChange={(step) => set({ maxPanes: Number(step) })}
        />
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
      <NumberField stage="queries:live-interval" label="Live interval" description="How often a live object reads its row again (w)." suffix="s" value={s.liveIntervalSecs} {...SETTINGS_LIMITS.liveIntervalSecs} onCommit={(liveIntervalSecs) => set({ liveIntervalSecs })} />
      <Switch label="Live check in table tabs" description="The active table tab reads the rows it shows again every 4 s and marks those changed in the database." checked={s.tableLiveCheck} onChange={(tableLiveCheck) => set({ tableLiveCheck })} />
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
    <>
      <Group>
        <NumberField label="Connect timeout" suffix="s" value={s.connectTimeoutSecs} {...SETTINGS_LIMITS.connectTimeoutSecs} onCommit={(connectTimeoutSecs) => set({ connectTimeoutSecs })} />
        <NumberField label="Default port" description="Used when a connection doesn't name one." value={s.defaultPort} {...SETTINGS_LIMITS.defaultPort} onCommit={(defaultPort) => set({ defaultPort })} />
      </Group>
      <ConnectionSlugs />
    </>
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
  registerSettingsPage({ id: "connections", group: "Database", label: "Connections", icon: Plug, order: 30, intro: "Defaults for new connections, and the name each saved connection has in addresses.", Body: ConnectionsPage });
  registerSettingsPage({ id: "logging", group: "System", label: "Logging", icon: ScrollText, order: 20, intro: "What DataBee writes to its log file.", Body: LoggingPage });
}
