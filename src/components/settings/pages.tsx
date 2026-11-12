/** The core settings pages. Modules register their own pages from their install function. */
import { Activity, Info, Keyboard, Palette, Plug, SquareTerminal, Table2 } from "lucide-react";
import { useId, type CSSProperties } from "react";
import { BUNDLED_FONTS, SCHEMES, SETTINGS_LIMITS, TEXT_SIZES, type SchemeId } from "../../api";
import { ROW_PADDING_PX, rowPaddingPx } from "../../lib/theme";
import { registerSettingsPage, type SettingsPageProps, type SettingsRow } from "../../registry/settingsPages";
import { selectActiveSession, selectDatabaseAccent, useStore } from "../../state/store";
import { AccentSwatches } from "../Accent";
import { ConnectionSlugs } from "./ConnectionSlugs";
import { PopUpButton } from "../PopUpButton";
import { Group, NumberField, Row, Segmented, SegmentedButtons, Switch, TextField } from "./controls";
import { shortcutRows } from "../../commands/shortcuts";
import { ShortcutsPage } from "./ShortcutsPage";
import { ABOUT_ROWS, AboutPage, STATUS_ROWS, StatusPage } from "./SystemPages";

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
function SchemeSelect({ label, anchor, dark, value, accent, stage, onChange }: {
  label: string;
  anchor: string;
  dark: boolean;
  value: SchemeId;
  accent: string;
  stage: string;
  onChange: (id: SchemeId) => void;
}) {
  const id = useId();
  return (
    <Row label={label} htmlFor={id} anchor={anchor}>
      <div className="scheme-select">
        <SchemePreview scheme={value} accent={accent} />
        <PopUpButton
          id={id}
          className="input"
          data-stage={stage}
          data-stage-label={label}
          value={value}
          options={[dark, !dark].map((groupDark) => ({
            caption: groupDark ? "Dark" : "Light",
            options: SCHEMES.filter((scheme) => scheme.dark === groupDark).map((scheme) => ({ value: scheme.id, label: scheme.label })),
          }))}
          onChange={(next) => onChange(toSchemeId(next))}
        />
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
    <Row label="Accent" description={description} anchor="appearance-accent">
      <AccentSwatches value={accent} stage="appearance:accent" onChange={onChange} />
    </Row>
  );
}

function AppearancePage({ s, set }: SettingsPageProps) {
  return (
    <>
      <Group title="Look" anchor="appearance-look">
        <Segmented
          label="Theme"
          anchor="appearance-theme"
          description="Which scheme applies; System follows macOS."
          options={["system", "light", "dark"] as const}
          value={s.theme}
          stage="appearance:theme"
          onChange={(theme) => set({ theme })}
        />
        <SchemeSelect
          label="Light scheme"
          anchor="appearance-light-scheme"
          dark={false}
          value={toSchemeId(s.lightScheme)}
          accent={s.accent}
          stage="appearance:light-scheme"
          onChange={(lightScheme) => set({ lightScheme })}
        />
        <SchemeSelect
          label="Dark scheme"
          anchor="appearance-dark-scheme"
          dark
          value={toSchemeId(s.darkScheme)}
          accent={s.accent}
          stage="appearance:dark-scheme"
          onChange={(darkScheme) => set({ darkScheme })}
        />
        <AccentRow accent={s.accent} onChange={(accent) => set({ accent })} />
        <Row label="Row padding" description="Space above and below every row in grids and lists." anchor="appearance-row-padding">
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
      <Group title="Layout" anchor="appearance-layout">
        <Segmented
          label="Panes side by side"
          anchor="appearance-max-panes"
          description="Columns of an object tab shown next to each other at most; fewer when the window is narrow."
          options={MAX_PANES_STEPS}
          value={String(s.maxPanes)}
          stage="appearance:max-panes"
          onChange={(step) => set({ maxPanes: Number(step) })}
        />
      </Group>
      <Group title="Text" anchor="appearance-text">
        <NumberField
          label="UI size"
          anchor="appearance-ui-size"
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
          anchor="appearance-data-size"
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
          anchor="appearance-font-ui"
          description="Any installed family; empty uses the bundled font."
          placeholder={BUNDLED_FONTS.ui}
          value={s.fontUi ?? ""}
          stage="appearance:font-ui"
          onCommit={(name) => set({ fontUi: name.trim() || null })}
        />
        <TextField
          label="Mono font"
          anchor="appearance-font-mono"
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
      <NumberField anchor="queries-row-limit" label="Row limit" description="Rows fetched per query in the SQL editor." value={s.rowLimit} {...SETTINGS_LIMITS.rowLimit} onCommit={(rowLimit) => set({ rowLimit })} />
      <NumberField anchor="queries-timeout" label="Query timeout" description="Queries running longer are stopped." suffix="s" value={s.queryTimeoutSecs} {...SETTINGS_LIMITS.queryTimeoutSecs} onCommit={(queryTimeoutSecs) => set({ queryTimeoutSecs })} />
      <NumberField anchor="queries-live-interval" stage="queries:live-interval" label="Live interval" description="How often a live object reads its row again (w)." suffix="s" value={s.liveIntervalSecs} {...SETTINGS_LIMITS.liveIntervalSecs} onCommit={(liveIntervalSecs) => set({ liveIntervalSecs })} />
      <Switch anchor="queries-live-check" label="Live check in table tabs" description="The active table tab reads the rows it shows again every 4 s and marks those changed in the database." checked={s.tableLiveCheck} onChange={(tableLiveCheck) => set({ tableLiveCheck })} />
      <Switch anchor="queries-confirm-destructive" label="Confirm destructive statements" description="Ask before DROP, TRUNCATE and DELETE/UPDATE without WHERE." checked={s.confirmDestructive} onChange={(confirmDestructive) => set({ confirmDestructive })} />
    </Group>
  );
}

function DisplayPage({ s, set }: SettingsPageProps) {
  return (
    <Group>
      <TextField anchor="display-null-label" label="NULL label" description="Shown in cells that hold NULL." value={s.nullLabel} onCommit={(nullLabel) => set({ nullLabel })} />
      <NumberField anchor="display-max-cell-chars" label="Max characters per cell" description="Longer values are truncated in the grid." value={s.maxCellChars} {...SETTINGS_LIMITS.maxCellChars} onCommit={(maxCellChars) => set({ maxCellChars })} />
      <Switch anchor="display-system-databases" label="Show system databases" description="mysql, sys, information_schema and performance_schema." checked={s.showSystemSchemas} onChange={(showSystemSchemas) => set({ showSystemSchemas })} />
    </Group>
  );
}

function ConnectionsPage({ s, set }: SettingsPageProps) {
  return (
    <>
      <Group title="New connections" anchor="connections-defaults">
        <NumberField anchor="connections-timeout" label="Connect timeout" suffix="s" value={s.connectTimeoutSecs} {...SETTINGS_LIMITS.connectTimeoutSecs} onCommit={(connectTimeoutSecs) => set({ connectTimeoutSecs })} />
        <NumberField anchor="connections-default-port" label="Default port" description="Used when a connection doesn't name one." value={s.defaultPort} {...SETTINGS_LIMITS.defaultPort} onCommit={(defaultPort) => set({ defaultPort })} />
      </Group>
      <ConnectionSlugs />
    </>
  );
}

const APPEARANCE_ROWS: SettingsRow[] = [
  { id: "appearance-look", title: "Look" },
  { id: "appearance-theme", title: "Theme", keywords: ["dark mode", "light mode"] },
  { id: "appearance-light-scheme", title: "Light scheme", keywords: ["colors", "theme"] },
  { id: "appearance-dark-scheme", title: "Dark scheme", keywords: ["colors", "theme"] },
  { id: "appearance-accent", title: "Accent", keywords: ["color"] },
  { id: "appearance-row-padding", title: "Row padding", keywords: ["density", "spacing"] },
  { id: "appearance-layout", title: "Layout" },
  { id: "appearance-max-panes", title: "Panes side by side", keywords: ["columns"] },
  { id: "appearance-text", title: "Text" },
  { id: "appearance-ui-size", title: "UI size", keywords: ["font size", "zoom"] },
  { id: "appearance-data-size", title: "Data size", keywords: ["font size", "zoom"] },
  { id: "appearance-font-ui", title: "UI font", keywords: ["typeface"] },
  { id: "appearance-font-mono", title: "Mono font", keywords: ["typeface", "monospace"] },
];

const QUERIES_ROWS: SettingsRow[] = [
  { id: "queries-row-limit", title: "Row limit" },
  { id: "queries-timeout", title: "Query timeout" },
  { id: "queries-live-interval", title: "Live interval", keywords: ["refresh"] },
  { id: "queries-live-check", title: "Live check in table tabs", keywords: ["refresh", "changes"] },
  { id: "queries-confirm-destructive", title: "Confirm destructive statements", keywords: ["drop", "truncate", "delete"] },
];

const DISPLAY_ROWS: SettingsRow[] = [
  { id: "display-null-label", title: "NULL label" },
  { id: "display-max-cell-chars", title: "Max characters per cell", keywords: ["truncate"] },
  { id: "display-system-databases", title: "Show system databases", keywords: ["information_schema", "mysql", "sys"] },
];

const CONNECTIONS_ROWS: SettingsRow[] = [
  { id: "connections-defaults", title: "New connections" },
  { id: "connections-timeout", title: "Connect timeout" },
  { id: "connections-default-port", title: "Default port", keywords: ["3306"] },
  { id: "connections-addresses", title: "Addresses", keywords: ["slug", "databee://"] },
];

export function registerCoreSettingsPages() {
  registerSettingsPage({ id: "appearance", group: "Interface", label: "Appearance", icon: Palette, order: 10, intro: "How DataBee looks. Changes apply right away.", rows: APPEARANCE_ROWS, Body: AppearancePage });
  registerSettingsPage({ id: "shortcuts", group: "Interface", label: "Shortcuts", icon: Keyboard, order: 20, intro: "Click a shortcut and press the new keys. Backspace unbinds, Escape cancels.", keywords: ["keys", "keyboard"], rows: shortcutRows, Body: ShortcutsPage });
  registerSettingsPage({ id: "queries", group: "Database", label: "Queries", icon: SquareTerminal, order: 10, intro: "Limits and safety checks for the SQL editor and table views.", rows: QUERIES_ROWS, Body: QueriesPage });
  registerSettingsPage({ id: "display", group: "Database", label: "Display", icon: Table2, order: 20, intro: "How values and schemas show up in grids, documents and the sidebar.", rows: DISPLAY_ROWS, Body: DisplayPage });
  registerSettingsPage({ id: "connections", group: "Database", label: "Connections", icon: Plug, order: 30, intro: "Defaults for new connections, and the name each saved connection has in addresses.", rows: CONNECTIONS_ROWS, Body: ConnectionsPage });
  registerSettingsPage({ id: "status", group: "System", label: "Status", icon: Activity, order: 20, intro: "This build, its log and the connections open now.", keywords: ["diagnostics", "logging"], rows: STATUS_ROWS, Body: StatusPage });
  registerSettingsPage({ id: "about", group: "System", label: "About", icon: Info, order: 30, keywords: ["version", "license"], rows: ABOUT_ROWS, Body: AboutPage });
}
