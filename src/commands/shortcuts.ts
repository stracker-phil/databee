/** Shortcut editing rules for Settings → Shortcuts: groups by where a key works, one key per command. */
import type { Keybindings } from "../api";
import type { SettingsRow } from "../registry/settingsPages";
import { allCommands, bindingFor, conflicts, type Command } from "./registry";
import { scopeLabel } from "./scopes";

/** Where each scope's keys work, in the order the groups show. Module scopes follow under their own label. */
const SCOPE_GROUPS: [scope: string, title: string][] = [
  ["global", "Everywhere"],
  ["db", "With a database open"],
  ["table", "In a table tab"],
  ["editor", "In the SQL editor"],
  ["object", "In an object tab"],
];

export const shortcutGroupTitle = (scope: string) => SCOPE_GROUPS.find(([id]) => id === scope)?.[1] ?? scopeLabel(scope);
export const shortcutGroupAnchor = (scope: string) => `shortcuts-in-${scope}`;
export const shortcutRowAnchor = (id: string) => `shortcut:${id}`;

/** The commands with a key of their own (a fallback takes another's), grouped by scope in `SCOPE_GROUPS` order. */
export function groupedCommands(): { scope: string; commands: Command[] }[] {
  const order = (scope: string) => {
    const index = SCOPE_GROUPS.findIndex(([id]) => id === scope);
    return index === -1 ? SCOPE_GROUPS.length : index;
  };
  const groups = new Map<string, Command[]>();
  for (const command of allCommands()) {
    if (command.fallbackFor) continue;
    groups.set(command.scope, [...(groups.get(command.scope) ?? []), command]);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => order(a) - order(b) || a.localeCompare(b))
    .map(([scope, commands]) => ({ scope, commands }));
}

/** The Settings search's rows for the Shortcuts page: each group and each command. */
export function shortcutRows(): SettingsRow[] {
  return groupedCommands().flatMap(({ scope, commands }) => [
    { id: shortcutGroupAnchor(scope), title: shortcutGroupTitle(scope) },
    ...commands.map((c) => ({ id: shortcutRowAnchor(c.id), title: c.title, keywords: [c.id] })),
  ]);
}

export const defaultBinding = (id: string) => bindingFor(id, {});

/** The command that loses `binding` when `id` takes it, if any (same key in an overlapping scope). */
export function holderOf(bindings: Keybindings, id: string, binding: string): Command | undefined {
  const clash = conflicts({ ...bindings, [id]: binding }).find((c) => c.binding === binding && (c.a.id === id || c.b.id === id));
  if (!clash) return undefined;
  return clash.a.id === id ? clash.b : clash.a;
}

/** `bindings` with `id` set to `binding`; the default binding drops the override, so nothing is left to reset. */
export function withBinding(bindings: Keybindings, id: string, binding: string | null): Keybindings {
  const { [id]: _old, ...rest } = bindings;
  return binding === defaultBinding(id) ? rest : { ...rest, [id]: binding };
}
