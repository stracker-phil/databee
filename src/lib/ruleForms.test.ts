import { describe, expect, it } from "vitest";
import type { AppError } from "../api";
import { ruleTables } from "../test/projectFixtures";
import { fieldError, kindRule, referenceRule, styleRule, type KindForm, type ReferenceForm, type StyleForm } from "./ruleForms";

const invalid = (message: string): AppError => ({ kind: "invalid_input", message, code: null, account: null });

describe("fieldError", () => {
  it.each([
    ["reference.from", "source"],
    ["reference.when.meta_key", "when"],
    ["reference.to", "target"],
    ["reference.to_when.status", "target"],
    ["reference.label", "label"],
    ["kind.in[0]", "inMeta"],
    ["style.in[1]", "inMeta"],
    ["kind.columns[0]", "columns"],
    ["style.value", "value"],
  ])("points `%s` at the %s field and keeps only the message", (path, field) => {
    expect(fieldError(invalid(`\`${path}\`: \`wp_users\` has no column \`nope\``))).toEqual({ field, message: "`wp_users` has no column `nope`" });
  });

  it("keeps a multi-line message whole", () => {
    expect(fieldError(invalid("`reference.from`: first\nsecond"))).toEqual({ field: "source", message: "first\nsecond" });
  });

  it.each([
    ["a message without a field", "the database went away"],
    ["a field the form does not have", "`project.prefix`: too long"],
  ])("falls back to the whole form for %s and keeps the full message", (_case, message) => {
    expect(fieldError(invalid(message))).toEqual({ field: "form", message });
  });
});

describe("referenceRule", () => {
  const form = (patch: Partial<ReferenceForm> = {}): ReferenceForm => ({
    type: "reference",
    opener: "r on wp_wc_orders_meta.meta_value",
    replacing: null,
    base: null,
    source: { table: "wp_wc_orders_meta", column: "meta_value" },
    when: { column: "meta_key", value: "_ppcp_paypal_order_id" },
    whenLocked: false,
    target: { table: "wp_wc_orders", column: "id" },
    label: "",
    ...patch,
  });

  it("maps the real tables of both ends to the names rules store", () => {
    expect(referenceRule(form(), ruleTables)).toEqual({
      id: "",
      from: "wc_orders_meta.meta_value",
      when: { meta_key: "_ppcp_paypal_order_id" },
      to: "wc_orders.id",
      to_when: {},
      label: null,
      columns: [],
    });
  });

  it("keeps the name of a table outside the prefix so Rust can refuse it", () => {
    expect(referenceRule(form({ target: { table: "other_table", column: "id" } }), ruleTables).to).toBe("other_table.id");
  });

  it.each([
    ["no when", null],
    ["a when without a column", { column: "", value: "x" }],
  ])("stores an empty when for %s", (_case, when) => {
    expect(referenceRule(form({ when }), ruleTables).when).toEqual({});
  });

  it("trims the label and stores an empty one as null", () => {
    expect(referenceRule(form({ label: "  PayPal orders " }), ruleTables).label).toBe("PayPal orders");
    expect(referenceRule(form({ label: "   " }), ruleTables).label).toBeNull();
  });

  it("keeps the base rule's when, to_when and columns when the form cannot show its when", () => {
    const base = { id: "p1", from: "a.b", when: { x: "1", y: "2" }, to: "c.d", to_when: { z: "3" }, columns: ["k"] };
    const rule = referenceRule(form({ base, whenLocked: true, when: null }), ruleTables);
    expect(rule).toMatchObject({ when: { x: "1", y: "2" }, to_when: { z: "3" }, columns: ["k"] });
  });
});

describe("kindRule", () => {
  const form = (patch: Partial<KindForm> = {}): KindForm => ({
    type: "kind",
    opener: "kind",
    index: null,
    scope: { columns: "wc_orders.status, *.post_status", metaKeys: "_wc_*", inMeta: "" },
    kind: "enum",
    zone: "unix",
    currency: "",
    ...patch,
  });
  const scope = { columns: ["wc_orders.status", "*.post_status"], meta_keys: ["_wc_*"], in: [] };

  it.each([
    ["enum", {}, { enum: true }],
    ["secret", {}, { secret: true }],
    ["number", {}, { number: true }],
    ["timestamp", { zone: "utc" as const }, { timestamp: "utc" }],
    ["money", { currency: "currency, _order_currency" }, { money: { currency: ["currency", "_order_currency"] } }],
  ] as const)("writes the %s kind next to the split scope", (kind, patch, expected) => {
    expect(kindRule(form({ kind, ...patch }))).toEqual({ ...scope, ...expected });
  });

  it("drops empty list entries", () => {
    expect(kindRule(form({ scope: { columns: " , a ,, ", metaKeys: "", inMeta: "wc_orders_meta" } }))).toMatchObject({ columns: ["a"], meta_keys: [], in: ["wc_orders_meta"] });
  });
});

describe("styleRule", () => {
  it("writes an enabled rule with the split scope, match, value and color", () => {
    const form: StyleForm = { type: "style", opener: "style", index: null, scope: { columns: "wc_orders.status", metaKeys: "", inMeta: "" }, match: "LIKE", value: "wc-%", color: "green" };
    expect(styleRule(form)).toEqual({ columns: ["wc_orders.status"], meta_keys: [], in: [], match: "LIKE", value: "wc-%", color: "green", enabled: true });
  });
});
