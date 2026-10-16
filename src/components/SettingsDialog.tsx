import { useEffect, useId, useState, type ReactNode } from "react";
import { Blocks, Copy, FolderOpen, Keyboard, Palette, Plug, Presentation, RefreshCw, RotateCcw, ScrollText, SquareTerminal, Table2, X, type LucideIcon } from "lucide-react";
import { ACCENTS, api, TEXT_SIZES, type Settings, type StageStatus } from "../api";
import { allCommands, bindingFor, bindingFromEvent, bindingLabel, conflicts, type Command } from "../commands";
import { relativeTime } from "../lib/format";
import { useStore } from "../state/store";
import { Modal } from "./Modal";

const SCOPE_LABEL: Record<Command["scope"], string> = { global: "Everywhere", wp: "WordPress", db: "Database", table: "Table", editor: "Editor", document: "Document" };

type Page = "appearance" | "shortcuts" | "queries" | "display" | "connections" | "wordpress" | "presentations" | "logging";

const NAV: { group: string; pages: { id: Page; label: string; icon: LucideIcon }[] }[] = [
  { group: "Interface", pages: [{ id: "appearance", label: "Appearance", icon: Palette }, { id: "shortcuts", label: "Shortcuts", icon: Keyboard }] },
  {
    group: "Database",
    pages: [
      { id: "queries", label: "Queries", icon: SquareTerminal },
      { id: "display", label: "Display", icon: Table2 },
      { id: "connections", label: "Connections", icon: Plug },
    ],
  },
  { group: "WordPress", pages: [{ id: "wordpress", label: "WordPress", icon: Blocks }] },
  {
    group: "System",
    pages: [
      { id: "presentations", label: "Presentations", icon: Presentation },
      { id: "logging", label: "Logging", icon: ScrollText },
    ],
  },
];

/** Settings pages in display order; the Stage hook lists them as the settings window's sections. */
export const SETTINGS_PAGES = NAV.flatMap((g) => g.pages.map(({ id, label }) => ({ id, title: label })));

const isPage = (id: string | undefined): id is Page => SETTINGS_PAGES.some((p) => p.id === id);

const PAGE_LABEL = Object.fromEntries(NAV.flatMap((g) => g.pages.map((p) => [p.id, p.label]))) as Record<Page, string>;

const ACCENT_VARS: Record<string, string> = Object.fromEntries(ACCENTS.map((a) => [a, `var(--accent-${a})`]));

const capitalize = (s: string) => s[0].toUpperCase() + s.slice(1);

function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="settings-group">
      {title && <h3>{title}</h3>}
      <div className="settings-card">{children}</div>
    </section>
  );
}

/** One card row: label and description on the left, the control on the right (or below when `stacked`). */
function Row({ label, description, htmlFor, stacked, children }: { label: string; description?: ReactNode; htmlFor?: string; stacked?: boolean; children: ReactNode }) {
  return (
    <div className={`settings-row${stacked ? " stacked" : ""}`}>
      <div className="settings-row-text">
        {htmlFor ? <label htmlFor={htmlFor}>{label}</label> : <div className="settings-row-label">{label}</div>}
        {description && <div className="settings-row-desc">{description}</div>}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}

function Switch({ label, description, checked, onChange }: { label: string; description?: ReactNode; checked: boolean; onChange: (v: boolean) => void }) {
  const id = useId();
  return (
    <Row label={label} description={description} htmlFor={id}>
      <input id={id} className="switch" type="checkbox" role="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </Row>
  );
}

function Segmented<T extends string>({ label, description, options, value, onChange }: { label: string; description?: ReactNode; options: readonly T[]; value: T; onChange: (v: T) => void }) {
  return (
    <Row label={label} description={description}>
      <div className="segmented" role="group" aria-label={label}>
        {options.map((o) => (
          <button key={o} type="button" aria-pressed={value === o} onClick={() => onChange(o)}>
            {capitalize(o)}
          </button>
        ))}
      </div>
    </Row>
  );
}

/** Text input that commits on blur or Enter, so half-typed values never reach the backend.
 *  Numbers within [min, max] commit as they change, so spinner clicks preview right away. */
function TextField({ label, description, value, onCommit, type = "text", min, max, suffix }: {
  label: string;
  description?: ReactNode;
  value: string;
  onCommit: (v: string) => void;
  type?: "text" | "number";
  min?: number;
  max?: number;
  suffix?: string;
}) {
  const id = useId();
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const commit = () => text !== value && onCommit(text);
  return (
    <Row label={label} description={description} htmlFor={id}>
      <div className="settings-input">
        <input
          id={id}
          className={`input${type === "number" ? " number" : ""}`}
          type={type}
          min={min}
          max={max}
          value={text}
          onChange={(e) => {
            const next = e.target.value;
            setText(next);
            const n = Number(next);
            if (type === "number" && next.trim() !== "" && n >= (min ?? -Infinity) && n <= (max ?? Infinity) && next !== value) onCommit(next);
          }}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && commit()}
        />
        {suffix && <span className="muted">{suffix}</span>}
      </div>
    </Row>
  );
}

function NumberField({ value, onCommit, ...rest }: { label: string; description?: ReactNode; value: number; onCommit: (v: number) => void; min: number; max: number; suffix?: string }) {
  return (
    <TextField
      {...rest}
      type="number"
      value={String(value)}
      onCommit={(t) => {
        const n = Number(t);
        if (t.trim() !== "" && Number.isFinite(n)) onCommit(n);
      }}
    />
  );
}

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

function ShortcutsPage() {
  const bindings = useStore((s) => s.keybindings);
  const save = useStore((s) => s.saveKeybindings);
  const [search, setSearch] = useState("");
  const needle = search.trim().toLowerCase();
  const list = allCommands().filter((c) => !needle || c.title.toLowerCase().includes(needle) || c.id.includes(needle));
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

const INTRO: Record<Page, string> = {
  appearance: "How DataBee looks. Changes apply right away.",
  shortcuts: "Click a shortcut and press the new keys. Backspace unbinds, Escape cancels.",
  queries: "Limits and safety checks for the SQL editor and table views.",
  display: "How values and schemas show up in grids, documents and the sidebar.",
  connections: "Defaults for new connections.",
  wordpress: "The WordPress lens for WooCommerce and WordPress databases.",
  presentations: "Lets a presenter app such as Stage highlight parts of DataBee and click through it for a demo. Only apps on this Mac can connect.",
  logging: "What DataBee writes to its log file.",
};

/** §10 of the Stage protocol: the hook's switch, status, port, token and register command. */
function PresentationsPage({ s, set }: { s: Settings; set: (patch: Partial<Settings>) => void }) {
  const [status, setStatus] = useState<StageStatus | null>(null);
  const [revealed, setRevealed] = useState(false);
  const { toast, newStageToken } = useStore.getState();
  useEffect(() => {
    let alive = true;
    const load = () => api.stageStatus().then((next) => alive && setStatus(next), () => alive && setStatus(null));
    void load();
    // Keeps "Last presenter request" current while the page is open.
    const timer = setInterval(load, 5000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [s.stage.enabled, s.stage.port, s.stage.token]);
  const stage = (patch: Partial<Settings["stage"]>) => set({ stage: { ...s.stage, ...patch } });
  return (
    <Group>
      <Switch label="Allow presentations" description="Runs a local API on 127.0.0.1; requests from web pages are refused." checked={s.stage.enabled} onChange={(enabled) => stage({ enabled })} />
      <Row label="Status" description={status?.running ? status.url : undefined}>
        {status?.error ? (
          <span className="pill tone-orange">{status.error}</span>
        ) : status?.running ? (
          <span className="pill tone-green">Ready</span>
        ) : (
          <span className="badge">Off</span>
        )}
      </Row>
      <NumberField label="Port" description="27184 unless another app needs it. Register again after a change." value={s.stage.port} min={1024} max={65535} onCommit={(port) => stage({ port })} />
      <Row label="Token" description="A new token disconnects presenters until DataBee is registered again.">
        <button type="button" className="btn mono" title={revealed ? "Hide" : "Show the token"} onClick={() => setRevealed((r) => !r)}>
          {revealed ? s.stage.token : "••••••••••••"}
        </button>
        <button type="button" className="icon-btn" aria-label="Copy token" title="Copy token" onClick={() => void navigator.clipboard.writeText(s.stage.token).then(() => toast("success", "Copied token"))}>
          <Copy size="1rem" />
        </button>
        <button type="button" className="btn" onClick={() => void newStageToken()}>
          <RefreshCw size="1rem" /> New Token
        </button>
      </Row>
      {status && (
        <Row label="Register with Stage" description={<code className="mono small selectable">{status.registerCommand}</code>}>
          <button type="button" className="btn" onClick={() => void navigator.clipboard.writeText(status.registerCommand).then(() => toast("success", "Copied the register command"))}>
            <Copy size="1rem" /> Copy
          </button>
        </Row>
      )}
      <Row label="Last presenter request">
        <span className="muted">{status?.lastRequest ? relativeTime(status.lastRequest) : "None yet"}</span>
      </Row>
    </Group>
  );
}

function PageBody({ page, s, set }: { page: Page; s: Settings; set: (patch: Partial<Settings>) => void }) {
  switch (page) {
    case "appearance":
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
    case "shortcuts":
      return <ShortcutsPage />;
    case "queries":
      return (
        <Group>
          <NumberField label="Row limit" description="Rows fetched per query in the SQL editor." value={s.rowLimit} min={10} max={100000} onCommit={(rowLimit) => set({ rowLimit })} />
          <NumberField label="Query timeout" description="Queries running longer are stopped." suffix="s" value={s.queryTimeoutSecs} min={1} max={3600} onCommit={(queryTimeoutSecs) => set({ queryTimeoutSecs })} />
          <Switch label="Confirm destructive statements" description="Ask before DROP, TRUNCATE and DELETE/UPDATE without WHERE." checked={s.confirmDestructive} onChange={(confirmDestructive) => set({ confirmDestructive })} />
        </Group>
      );
    case "display":
      return (
        <Group>
          <TextField label="NULL label" description="Shown in cells that hold NULL." value={s.nullLabel} onCommit={(nullLabel) => set({ nullLabel })} />
          <NumberField label="Max characters per cell" description="Longer values are truncated in the grid." value={s.maxCellChars} min={20} max={100000} onCommit={(maxCellChars) => set({ maxCellChars })} />
          <Switch label="Show system databases" description="mysql, sys, information_schema and performance_schema." checked={s.showSystemSchemas} onChange={(showSystemSchemas) => set({ showSystemSchemas })} />
        </Group>
      );
    case "connections":
      return (
        <Group>
          <NumberField label="Connect timeout" suffix="s" value={s.connectTimeoutSecs} min={1} max={120} onCommit={(connectTimeoutSecs) => set({ connectTimeoutSecs })} />
          <NumberField label="Default port" description="Used when a connection doesn't name one." value={s.defaultPort} min={1} max={65535} onCommit={(defaultPort) => set({ defaultPort })} />
        </Group>
      );
    case "wordpress":
      return (
        <Group>
          <Switch label="Gravatar avatars" description="Loads customer avatars from gravatar.com, which sends a hash of each email address." checked={s.gravatar} onChange={(gravatar) => set({ gravatar })} />
        </Group>
      );
    case "presentations":
      return <PresentationsPage s={s} set={set} />;
    case "logging":
      return (
        <Group>
          <Row label="Log level" htmlFor="settings-log-level" description={s.logLevel === "trace" ? "Trace logs include the SQL text of every query." : "Higher levels write more detail."}>
            <select id="settings-log-level" className="input" value={s.logLevel} onChange={(e) => set({ logLevel: e.target.value as Settings["logLevel"] })}>
              {(["error", "warn", "info", "debug", "trace"] as const).map((l) => (
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
}

/** macOS-style settings: sidebar of pages, grouped cards, every change applies immediately. */
export function SettingsDialog() {
  const settings = useStore((s) => s.settings);
  const save = useStore((s) => s.saveSettings);
  const close = useStore((s) => s.closeDialog);
  const shown = useStore((s) => (s.dialog?.type === "settings" ? s.dialog.page : undefined));
  const page: Page = isPage(shown) ? shown : "appearance";
  const setPage = (id: Page) => useStore.getState().openDialog({ type: "settings", page: id });
  if (!settings) return null;

  const set = (patch: Partial<Settings>) => void save({ ...useStore.getState().settings!, ...patch }, { quiet: true });

  return (
    <Modal onClose={close} className="settings-window" labelledBy="settings-title" stageWindow="settings">
      <nav className="settings-nav" aria-label="Settings pages" data-stage="settings-nav" data-stage-label="Settings pages">
        {NAV.map((g) => (
          <div key={g.group} className="settings-nav-group">
            <div className="settings-nav-heading">{g.group}</div>
            {g.pages.map(({ id, label, icon: Icon }) => (
              <button key={id} data-stage={`settings-nav:${id}`} data-stage-list="settings-nav" type="button" aria-current={page === id ? "page" : undefined} onClick={() => setPage(id)}>
                <Icon size="1.077rem" /> {label}
              </button>
            ))}
          </div>
        ))}
      </nav>
      <div className="settings-pane" data-stage={`settings:${page}`} data-stage-label={`Settings: ${PAGE_LABEL[page]}`}>
        <header>
          <h2 id="settings-title">{PAGE_LABEL[page]}</h2>
          <button data-stage="settings:close" type="button" className="icon-btn" aria-label="Close settings" title="Close (Esc)" onClick={close}>
            <X size="1.077rem" />
          </button>
        </header>
        <div className="settings-body">
          <p className="settings-intro">{INTRO[page]}</p>
          <PageBody page={page} s={settings} set={set} />
        </div>
      </div>
    </Modal>
  );
}
