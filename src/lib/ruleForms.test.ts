import { describe, expect, it } from "vitest";
import type { AppError } from "../api";
import { orderItemsRule, referenceRow, ruleTables } from "../test/projectFixtures";
import { fieldError, kindRule, referenceFormFromRow, referenceRule, ruleOriginText, styleRule, type KindForm, type ReferenceForm, type StyleForm } from "./ruleForms";

const invalid = (message: string): AppError => ({ kind: "invalid_input", message, code: null, account: null });

const invalidField = (path: string, message: string): AppError =>
  ({ kind: "invalid_input", message: `\`${path}\`: ${message}`, code: null, account: null, field: { path, message } });

describe("fieldError", () => {
  it.each([
    ["reference.from", "source"],
    ["reference.when.meta_key", "when"],
    ["reference.to", "target"],
    ["reference.to_when.status", "target"],
    ["reference.label", "label"],
    ["reference.id", "label"],
    ["kind.in[0]", "inMeta"],
    ["style.in[1]", "inMeta"],
    ["kind.columns[0]", "columns"],
    ["style.columns[2]", "columns"],
    ["columns", "columns"],
    ["style.value", "value"],
  ])("points `%s` at the %s field and returns the field message", (path, field) => {
    expect(fieldError(invalidField(path, "`wp_users` has no column `nope`"))).toEqual({ field, message: "`wp_users` has no column `nope`" });
  });

  it("keeps a multi-line message whole", () => {
    expect(fieldError(invalidField("reference.from", "first\nsecond"))).toEqual({ field: "source", message: "first\nsecond" });
  });

  it("falls back to the whole form for an error without a field and keeps the full message", () => {
    expect(fieldError(invalid("the database went away"))).toEqual({ field: "form", message: "the database went away" });
  });

  it.each(["project.prefix", "presets[0]"])("falls back to the whole form for the unknown field path `%s` and keeps the full message", (path) => {
    const error = invalidField(path, "too long");
    expect(fieldError(error)).toEqual({ field: "form", message: error.message });
  });
});

describe("referenceRule", () => {
  const form = (patch: Partial<ReferenceForm> = {}): ReferenceForm => ({
    type: "reference",
    opener: "r on wp_wc_orders_meta.meta_value",
    replacing: null,
    ruleId: "",
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

  it("stores the rule under the form's rule id", () => {
    expect(referenceRule(form({ ruleId: "x" }), ruleTables).id).toBe("x");
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

describe("referenceFormFromRow", () => {
  const inferred = referenceRow({ origin: { kind: "inferred", indexed: true }, id: "inferred-1", rule: { ...orderItemsRule, id: "" } });

  it.each([
    ["a project row", referenceRow({ origin: { kind: "project" }, id: "p1" }), { replacing: "p1", ruleId: "" }],
    ["an edited preset row", referenceRow({ id: "p1", edited: true }), { replacing: "p1", ruleId: "" }],
    ["an edited inferred row", { ...inferred, edited: true }, { replacing: "inferred-1", ruleId: "" }],
    ["an unedited preset row", referenceRow(), { replacing: null, ruleId: "x" }],
    ["an unedited inferred row", inferred, { replacing: null, ruleId: "" }],
  ])("opens %s with the right replacing and rule id", (_case, row, expected) => {
    expect(referenceFormFromRow(row, "edit")).toMatchObject(expected);
  });

  it("opens nothing for a row without a rule", () => {
    expect(referenceFormFromRow(referenceRow({ rule: null }), "edit")).toBeNull();
  });
});

describe("ruleOriginText", () => {
  it.each([
    [{ kind: "project" }, "project"],
    [{ kind: "preset", preset: "woocommerce" }, "preset woocommerce"],
    [{ kind: "foreignKey", constraint: "fk_user" }, "foreign key fk_user"],
    [{ kind: "inferred", indexed: true }, "inferred · same name · indexed"],
    [{ kind: "inferred", indexed: false }, "inferred · same name · not indexed"],
  ] as const)("writes %j as %s", (origin, text) => {
    expect(ruleOriginText(origin)).toBe(text);
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
    const form: StyleForm = { type: "style", opener: "style", replacing: null, scope: { columns: "wc_orders.status", metaKeys: "", inMeta: "" }, match: "LIKE", value: "wc-%", color: "green" };
    expect(styleRule(form)).toEqual({ columns: ["wc_orders.status"], meta_keys: [], in: [], match: "LIKE", value: "wc-%", color: "green", enabled: true });
  });
});
