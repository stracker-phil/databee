import { api, type CommandInfo } from "../api";
import catalogJson from "./catalog.gen.json";
import type { Command } from "./registry";

/** Every command's id, title, scope and keys, written from Rust's catalog (`bus/catalog.rs`) by `bun run types`. */
export const CATALOG = catalogJson as CommandInfo[];

/** What the page adds to a page command of the catalog. */
export type Handler = Pick<Command, "id" | "when" | "run">;

/**
 * The catalog's commands, each run by its page handler or, for commands that run in Rust, through `runCommand`.
 * Throws on a handler the catalog lacks and on a page command without a handler.
 */
export function catalogCommands(handlers: Handler[]): Command[] {
  const byId = new Map(handlers.map((handler) => [handler.id, handler]));
  for (const id of byId.keys()) {
    if (!CATALOG.some((info) => info.id === id)) throw new Error(`Command "${id}" is missing from the catalog (bus/catalog.rs)`);
  }
  return CATALOG.map((info) => {
    const handler = byId.get(info.id);
    if (info.runs === "page" && !handler) throw new Error(`Page command "${info.id}" has no handler`);
    if (info.runs === "rust" && handler) throw new Error(`Command "${info.id}" runs in Rust; remove its page handler`);
    return {
      id: info.id,
      title: info.title,
      scope: info.scope,
      defaultBinding: info.defaultBinding ?? undefined,
      menu: info.menuLabel ? true : (info.menuShares ?? undefined),
      fallbackFor: info.fallbackFor ?? undefined,
      skipInText: info.skipInText ? true : undefined,
      when: handler?.when,
      run: handler?.run ?? (() => api.runCommand(info.id)),
    };
  });
}
