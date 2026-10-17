import { describe, expect, it } from "vitest";
import type { Address, Provenance, ReferenceSource, Value, Zone } from "../api";
import { num, text, value } from "../test/fixtures";
import { address } from "./address";
import { provenanceText, sourceText } from "./provenance";

const row = (table: string, key: string): Address => address("shop", "db", { kind: "row", table, key });
const source = (table = "wp_posts", column = "post_author", id = "5"): Provenance => ({ table, column, row: [{ column: "ID", value: id }] });
const preset: ReferenceSource = { kind: "preset", preset: "woocommerce", id: "order-user" };

const reference = (patch: Partial<Extract<Value["kind"], { kind: "reference" }>> = {}): Value =>
  value({ raw: num("12"), source: source(), kind: { kind: "reference", targets: [row("wp_users", "12")], by: preset, exists: null, ...patch } });

describe("sourceText", () => {
  it("names the table, column and row key", () => {
    expect(sourceText({ table: "wp_wc_orders_meta", column: "meta_value", row: [{ column: "id", value: "9123" }] })).toBe("wp_wc_orders_meta.meta_value · id 9123");
  });

  it("lists every part of a composite key", () => {
    expect(sourceText({ table: "t", column: "c", row: [{ column: "a", value: "1" }, { column: "b", value: "2" }] })).toBe("t.c · a 1, b 2");
  });

  it("leaves the key out when the row has none", () => {
    expect(sourceText({ table: "t", column: "c", row: [] })).toBe("t.c");
  });
});

describe("provenanceText", () => {
  it("is null for a value without source or meaning", () => {
    expect(provenanceText(value())).toBeNull();
  });

  it("is the source for a plain stored value", () => {
    expect(provenanceText(value({ source: source() }))).toBe("wp_posts.post_author · ID 5");
  });

  describe("of a reference", () => {
    it("says where it points and which preset says so", () => {
      expect(provenanceText(reference())).toBe("wp_posts.post_author · ID 5 · → wp_users #12 · preset woocommerce");
    });

    it.each<{ by: ReferenceSource; rule: string }>([
      { by: { kind: "foreignKey", constraint: "fk_author" }, rule: "foreign key fk_author" },
      { by: { kind: "project", id: "p1" }, rule: "project p1" },
      { by: { kind: "sameName", column: "ID", indexed: true }, rule: "inferred · same name · indexed" },
      { by: { kind: "sameName", column: "ID", indexed: false }, rule: "inferred · same name" },
    ])("names the rule $by.kind as '$rule'", ({ by, rule }) => {
      expect(provenanceText(reference({ by }))).toBe(`wp_posts.post_author · ID 5 · → wp_users #12 · ${rule}`);
    });

    it("joins several targets and shows a filtered table target with its filter", () => {
      const filtered = address("shop", "db", { kind: "table", table: "wp_usermeta", filter: [{ column: "user_id", value: "12" }] });
      expect(provenanceText(reference({ targets: [row("wp_users", "12"), filtered] }))).toContain("→ wp_users #12 | wp_usermeta · user_id 12 · preset woocommerce");
    });

    it("says that the target row does not exist when it is dangling", () => {
      const dangling = reference({ exists: false });
      expect(provenanceText({ ...dangling, raw: num("99") })).toBe("wp_posts.post_author · ID 5 · no row #99 in wp_users");
    });
  });

  describe("of a no-reference value", () => {
    it.each([
      { name: "zero", raw: num("0"), shown: "0" },
      { name: "NULL", raw: { t: "null" } as const, shown: "NULL" },
    ])("explains $name as no reference", ({ raw, shown }) => {
      expect(provenanceText(value({ raw, kind: { kind: "noReference" } }))).toBe(`${shown} = no reference`);
    });
  });

  describe("of a timestamp", () => {
    const stamp = (utc: number | null, zone: Zone | null) => value({ raw: text("2026-01-15 10:00:00"), kind: { kind: "timestamp", utc, zone } });

    it("shows UTC and local time", () => {
      const utc = Date.UTC(2026, 0, 15, 10) / 1000;
      expect(provenanceText(stamp(utc, "utc"))).toMatch(/^UTC 2026-01-15 10:00:00 · local \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
    });

    it.each<Zone | null>(["local", null])("says the zone is not stored for %s", (zone) => {
      expect(provenanceText(stamp(null, zone))).toBe("zone not stored");
    });

    it.each<Zone>(["utc", "unix"])("says nothing for a zero or unreadable %s date", (zone) => {
      expect(provenanceText(stamp(null, zone))).toBeNull();
    });
  });

  describe("of a money value", () => {
    const money = (currency: Value | null) => value({ raw: num("49.90"), kind: { kind: "money", currency } });

    it("names the source of the currency", () => {
      const eur = value({ raw: text("EUR"), source: source("wp_wc_orders", "currency", "3") });
      expect(provenanceText(money(eur))).toBe("currency wp_wc_orders.currency · ID 3");
    });

    it("says nothing when the currency has no source", () => {
      expect(provenanceText(money(value({ raw: text("EUR") })))).toBeNull();
    });
  });
});
