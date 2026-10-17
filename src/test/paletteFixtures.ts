import type { PaletteHit } from "../api";
import { address } from "../lib/address";
import { num, text, value } from "./fixtures";
import { session } from "./session";

/** A row hit that opens `table/key` of `db`; `source` is `table` unless the hit matched through `table.column`. */
export function paletteHit(source: string, key: string, section = `objects with id ${key}`, table = source.split(".")[0]): PaletteHit {
  return {
    section,
    address: address(session.slug, "db", { kind: "row", table, key }),
    source,
    title: [value({ raw: num(key), kind: { kind: "number" } })],
    detail: [value({ raw: text("wc-processing") })],
  };
}
