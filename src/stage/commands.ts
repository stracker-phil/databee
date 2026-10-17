/**
 * DataBee's commands as the Stage hook lists them: the registry's (with their shortcut in Stage
 * notation), the conventional ids Stage knows (`app:settings`, `view:palette`), and families that
 * take an argument.
 */
import type { StageCommand } from "./types";
import { activeScopes, allCommands, bindingFor, getCommand, resolveKeydown, runCommand, type KeyLike } from "../commands";
import { selectActiveSession, selectWp, useStore } from "../state/store";
import { settingsSections } from "./state";

const MAX_CHOICES = 30;

/** Stage's conventional ids for registry commands, which every hooked app uses for the same action. */
const ALIASES: Record<string, string> = { "app:settings": "settings.open", "view:palette": "palette.open" };
const STAGE_ID = Object.fromEntries(Object.entries(ALIASES).map(([stage, id]) => [id, stage]));

export const stageId = (id: string) => STAGE_ID[id] ?? id;

const WORDS: Record<string, string> = { Mod: "cmd", Ctrl: "ctrl", Alt: "alt", Shift: "shift" };
const ORDER = ["ctrl", "alt", "shift", "cmd"];

/** `Mod+Shift+K` → `shift+cmd+k`: lowercase words, modifiers ordered ctrl alt shift cmd. */
export function stageShortcut(binding: string | null): string | undefined {
  if (!binding) return undefined;
  const parts = binding.split("+");
  const key = parts.pop()!.toLowerCase();
  const mods = parts.map((m) => WORDS[m] ?? m.toLowerCase()).sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  return [...mods, key].join("+");
}

interface Family {
  id: string;
  help: string;
  arg: string;
  enabled: () => boolean;
  choices: () => { argument: string; title: string }[];
  /** Runs the member; false when the argument isn't one it knows. */
  run: (argument: string) => boolean;
}

function tablesOfCurrentDatabase(): string[] {
  const s = useStore.getState();
  const session = selectActiveSession(s);
  const db = session?.database;
  return db && session ? (s.schema[session.id]?.tables[db] ?? []).map((t) => t.name) : [];
}

const FAMILIES: Family[] = [
  {
    id: "settings:open:",
    help: "Open Settings on a page",
    arg: "<section>",
    enabled: () => true,
    choices: () => settingsSections().map((p) => ({ argument: p.id, title: p.title })),
    run: (page) => {
      if (!settingsSections().some((p) => p.id === page)) return false;
      useStore.getState().openDialog({ type: "settings", page });
      return true;
    },
  },
  {
    id: "table:open:",
    help: "Open a table of the current database",
    arg: "<table>",
    enabled: () => tablesOfCurrentDatabase().length > 0,
    choices: () => tablesOfCurrentDatabase().map((t) => ({ argument: t, title: t })),
    run: (table) => {
      const s = useStore.getState();
      const db = selectActiveSession(s)?.database;
      if (!db || !tablesOfCurrentDatabase().includes(table)) return false;
      s.setMode("db");
      s.openTableTab(db, table);
      return true;
    },
  },
  {
    id: "wp:open:",
    help: "Open a WordPress record, e.g. wp:open:order:4711",
    arg: "<entity>:<id>",
    enabled: () => !!selectWp(useStore.getState()),
    choices: () =>
      useStore
        .getState()
        .recentNav.flatMap((r) => (r.kind === "entity" ? [{ argument: `${r.entity}:${r.id}`, title: r.title }] : [])),
    run: (target) => {
      const [entity, ...rest] = target.split(":");
      const id = rest.join(":");
      const s = useStore.getState();
      if (!entity || !id || !selectWp(s)) return false;
      void s.openEntity(entity, id);
      return true;
    },
  },
];

export function stageCommands(): StageCommand[] {
  const registry = allCommands().map((c) => ({
    id: stageId(c.id),
    help: c.title,
    shortcut: stageShortcut(bindingFor(c.id)),
    enabled: c.when?.() !== false,
  }));
  const families = FAMILIES.map((f) => ({
    id: f.id,
    help: f.help,
    arg: f.arg,
    enabled: f.enabled(),
    choices: f
      .choices()
      .slice(0, MAX_CHOICES)
      .map((c) => ({ id: `${f.id}${c.argument}`, title: c.title })),
  }));
  return [...registry, ...families];
}

/** Runs a command by its Stage id; why it didn't, with the HTTP status, when it can't. */
export function runStageCommand(id: string): { error: string; status: 404 | 409 } | null {
  const cantRun = { error: `“${id}” can't run in the current view`, status: 409 } as const;
  const family = FAMILIES.find((f) => id.startsWith(f.id) && id.length > f.id.length);
  if (family) {
    if (!family.enabled()) return cantRun;
    return family.run(id.slice(family.id.length)) ? null : { error: `Unknown command “${id}”; its choices list what exists now`, status: 404 };
  }
  const registryId = ALIASES[id] ?? id;
  if (!getCommand(registryId)) return { error: `Unknown command “${id}”`, status: 404 };
  return runCommand(registryId) ? null : cantRun;
}

/** The command a key press would run in the current view (like a menu accelerator). */
export function commandForKeys(keys: KeyLike & { target: EventTarget | null }) {
  return resolveKeydown(keys, activeScopes());
}
