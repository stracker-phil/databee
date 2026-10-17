import type { KindForm, ReferenceForm, ScopeDraft, StyleForm } from "../../lib/ruleForms";

/** A column as rules name it (`wc_orders.status`); a table outside the prefix keeps its name and Rust refuses it. */
export interface ColumnTarget {
  table: string;
  ruleName: string | null;
  column: string;
}

const opener = (target: ColumnTarget) => `structure of ${target.table}.${target.column}`;

const columnScope = (target: ColumnTarget): ScopeDraft => ({ columns: `${target.ruleName ?? target.table}.${target.column}`, metaKeys: "", inMeta: "" });

export function newReferenceForm(target: ColumnTarget): ReferenceForm {
  return {
    type: "reference",
    opener: opener(target),
    replacing: null,
    ruleId: "",
    base: null,
    source: { table: target.table, column: target.column },
    when: null,
    whenLocked: false,
    target: { table: "", column: "" },
    label: "",
  };
}

export function newKindForm(target: ColumnTarget): KindForm {
  return { type: "kind", opener: opener(target), index: null, scope: columnScope(target), kind: "enum", zone: "unix", currency: "" };
}

export function newStyleForm(target: ColumnTarget): StyleForm {
  return { type: "style", opener: opener(target), replacing: null, scope: columnScope(target), match: "=", value: "", color: "red" };
}
