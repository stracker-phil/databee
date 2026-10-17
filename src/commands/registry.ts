import { isAncestor, scopeDef, specificity } from "./scopes";

export type Scope = string;

export interface Command {
  id: string;
  title: string;
  scope: Scope;
  defaultBinding?: string;
  /** Enabled-state check; disabled commands neither fire nor show in the palette. */
  when?: () => boolean;
  run: () => void | Promise<void>;
  /** Also bound as a native menu accelerator; rebinding rebuilds the menu. */
  menu?: boolean;
  /** Has no key of its own: it takes this command's binding and runs while that one is disabled, so the two never conflict. */
  fallbackFor?: string;
}

export type Overrides = Record<string, string | null>;

const MODIFIERS = ["Mod", "Ctrl", "Alt", "Shift"] as const;
const KEY_LABELS: Record<string, string> = {
  Enter: "↵",
  Escape: "⎋",
  Tab: "⇥",
  Backspace: "⌫",
  Delete: "⌦",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Space: "␣",
};
const MOD_LABELS: Record<(typeof MODIFIERS)[number], string> = { Mod: "⌘", Ctrl: "⌃", Alt: "⌥", Shift: "⇧" };

const commands = new Map<string, Command>();
let overrides: Overrides = {};

/** Throws on a command whose scope is not registered, which catches scope typos at install time. */
export function registerCommands(list: Command[]) {
  for (const c of list) {
    if (!scopeDef(c.scope)) throw new Error(`Command "${c.id}" has unknown scope "${c.scope}"`);
  }
  for (const c of list) commands.set(c.id, c);
}

export function clearCommands() {
  commands.clear();
  overrides = {};
}

export function allCommands(): Command[] {
  return [...commands.values()];
}

export function getCommand(id: string): Command | undefined {
  return commands.get(id);
}

export function setOverrides(next: Overrides) {
  overrides = { ...next };
}

/** The effective binding: user override, else the default, else none. */
export function bindingFor(id: string, custom: Overrides = overrides): string | null {
  const fallbackFor = commands.get(id)?.fallbackFor;
  if (fallbackFor) return bindingFor(fallbackFor, custom);
  if (Object.prototype.hasOwnProperty.call(custom, id)) return custom[id] === null ? null : normalizeBinding(custom[id]);
  const def = commands.get(id)?.defaultBinding;
  return def ? normalizeBinding(def) : null;
}

function canonicalKey(key: string): string | null {
  if (key === " ") return "Space";
  if (key.length === 1) return key.toUpperCase();
  if (["Meta", "Control", "Alt", "Shift", "CapsLock", "Fn", "Dead", "Unidentified"].includes(key)) return null;
  return key;
}

/** Splits on `+`; a trailing `++` (or a bare `+`) is the `+` key, as in `Mod+Shift++`. */
function splitBinding(binding: string): string[] {
  if (binding === "+") return ["+"];
  if (binding.endsWith("++")) return [...binding.slice(0, -2).split("+"), "+"];
  return binding.split("+");
}

/** Normalizes `shift+mod+k` to `Mod+Shift+K`; returns null for an unparsable string. */
function normalizeBinding(binding: string): string | null {
  const parts = splitBinding(binding).map((p) => p.trim());
  if (parts.length === 0) return null;
  const mods = new Set<string>();
  let key: string | null = null;
  for (const part of parts) {
    const lower = part.toLowerCase();
    const mod = MODIFIERS.find((m) => m.toLowerCase() === lower || (m === "Mod" && (lower === "cmd" || lower === "meta" || lower === "cmdorctrl")));
    if (mod) mods.add(mod);
    else if (part && key === null) key = canonicalKey(part);
    else return null;
  }
  if (!key) return null;
  return [...MODIFIERS.filter((m) => mods.has(m)), key].join("+");
}

export interface KeyLike {
  key: string;
  /** Physical key (`KeyK`, `Digit1`, `Minus`); used when `key` is a composed or non-Latin character. */
  code?: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

const PUNCTUATION_CODES: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

function keyFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  return PUNCTUATION_CODES[code] ?? null;
}

/** A single character outside printable ASCII (digits and punctuation are not Latin script) and Latin letters. */
function isNonLatinChar(key: string) {
  return key.length === 1 && !/^[\x20-\x7e]$|^\p{Script=Latin}$/u.test(key);
}

/**
 * ⌥ composes characters (⌥K is `˚`) and non-Latin layouts type other scripts, so those fall back to the
 * physical key. Plain Latin characters keep `e.key`, so punctuation follows the user's layout (⌘- on a
 * German keyboard stays `-`).
 */
function eventKey(e: KeyLike): string | null {
  const physical = e.code ? keyFromCode(e.code) : null;
  if (physical && (e.altKey || e.key === "Dead" || e.key === "Unidentified" || isNonLatinChar(e.key))) return physical;
  return canonicalKey(e.key);
}

/** The binding a keyboard event represents, or null for a bare modifier press. */
export function bindingFromEvent(e: KeyLike, mac = true): string | null {
  const key = eventKey(e);
  if (!key) return null;
  const mods: string[] = [];
  if (mac ? e.metaKey : e.ctrlKey) mods.push("Mod");
  if (mac && e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  return [...mods, key].join("+");
}

/** `Mod+Shift+K` → `⌘⇧K`. */
export function bindingLabel(binding: string | null): string {
  if (!binding) return "";
  const parts = splitBinding(binding);
  const key = parts[parts.length - 1];
  const mods = parts.slice(0, -1) as (typeof MODIFIERS)[number][];
  return mods.map((m) => MOD_LABELS[m] ?? m).join("") + (KEY_LABELS[key] ?? key);
}

export function isTextInput(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag === "INPUT") {
    const type = (target as HTMLInputElement).type;
    return !["checkbox", "radio", "button", "submit", "range"].includes(type);
  }
  return false;
}

/** A shortcut field in Settings is recording: its keys, Escape included, are the new binding. */
export function isRecordingTarget(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest("[data-recording]");
}

function scopesOverlap(a: Scope, b: Scope): boolean {
  return a === b || isAncestor(a, b) || isAncestor(b, a);
}

/**
 * Finds the command for a keydown. Inside text inputs only `Mod`/`Ctrl` combos fire
 * (and Escape), so typing never triggers plain-key commands.
 */
export function resolveKeydown(e: KeyLike & { target?: EventTarget | null }, activeScopes: Scope[], mac = true): Command | null {
  const binding = bindingFromEvent(e, mac);
  if (!binding) return null;
  if (isTextInput(e.target ?? null) && !binding.includes("Mod+") && !binding.includes("Ctrl+") && binding !== "Escape") return null;
  const matches = allCommands()
    .filter((c) => bindingFor(c.id) === binding && activeScopes.includes(c.scope) && (c.when?.() ?? true))
    .sort((a, b) => specificity(b.scope) - specificity(a.scope));
  return matches[0] ?? null;
}

interface Conflict {
  binding: string;
  a: Command;
  b: Command;
}

export function conflicts(custom: Overrides = overrides): Conflict[] {
  const list = allCommands();
  const out: Conflict[] = [];
  for (let i = 0; i < list.length; i++) {
    const ba = bindingFor(list[i].id, custom);
    if (!ba) continue;
    for (let j = i + 1; j < list.length; j++) {
      const fallback = list[i].fallbackFor === list[j].id || list[j].fallbackFor === list[i].id;
      if (!fallback && bindingFor(list[j].id, custom) === ba && scopesOverlap(list[i].scope, list[j].scope)) {
        out.push({ binding: ba, a: list[i], b: list[j] });
      }
    }
  }
  return out;
}
