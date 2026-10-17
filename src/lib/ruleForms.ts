/**
 * The Project page's rule forms (reference, kind, value style): their state, how a value or a listed rule
 * fills them, and the rule they save. Table names in the reference form are real ones; `RuleTable`s from Rust
 * map them to the names rules store. Kind and style scopes are typed as rules store them (globs allowed).
 */
import type { AppError, KindRow, KindRule, Provenance, Reference, ReferenceRow, RuleDraft, RuleOrigin, RuleTable, StyleColor, StyleMatch, StyleRow, StyleRule, Zone } from "../api";

export interface ColumnPick {
  table: string;
  column: string;
}

export interface WhenDraft {
  column: string;
  value: string;
}

export interface ReferenceForm {
  type: "reference";
  /** What opened the form, e.g. `r on wp_wc_orders_meta.meta_value · meta_id 9`. */
  opener: string;
  replacing: string | null;
  /** The stored rule being edited or promoted; what the form does not show is kept from it. */
  base: Reference | null;
  source: ColumnPick;
  when: WhenDraft | null;
  /** The base rule's `when` is more than one `=`, so the form shows it but keeps it as it is. */
  whenLocked: boolean;
  target: ColumnPick;
  label: string;
}

/** Comma-separated lists, as rules write them: `wc_orders.status`, `_wc_*`, `wc_orders_meta`. */
export interface ScopeDraft {
  columns: string;
  metaKeys: string;
  inMeta: string;
}

export const KIND_CHOICES = ["enum", "money", "timestamp", "secret", "number"] as const;
export type KindChoice = (typeof KIND_CHOICES)[number];
export const ZONES: Zone[] = ["unix", "utc", "local"];
export const STYLE_MATCHES: StyleMatch[] = ["=", "LIKE", "REGEXP"];
export const STYLE_COLORS: StyleColor[] = ["red", "green", "yellow", "blue", "purple"];

export interface KindForm {
  type: "kind";
  opener: string;
  index: number | null;
  scope: ScopeDraft;
  kind: KindChoice;
  zone: Zone;
  currency: string;
}

export interface StyleForm {
  type: "style";
  opener: string;
  index: number | null;
  scope: ScopeDraft;
  match: StyleMatch;
  value: string;
  color: StyleColor;
}

export type ProjectForm = ReferenceForm | KindForm | StyleForm;

/** The form fields Rust's errors point at (`AppError.field`). */
export type FormField = "source" | "when" | "target" | "label" | "columns" | "metaKeys" | "inMeta" | "value" | "form";

export interface FieldError {
  field: FormField;
  message: string;
}

const list = (text: string) =>
  text
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);

const joined = (items: string[] | undefined) => (items ?? []).join(", ");

export function openerText(command: string, source: Provenance): string {
  const key = source.row.map((part) => `${part.column} ${part.value}`).join(", ");
  return `${command} on ${source.table}.${source.column}${key ? ` · ${key}` : ""}`;
}

function scopeOf(draft: RuleDraft): ScopeDraft {
  if (draft.meta) return { columns: "", metaKeys: draft.meta.key, inMeta: draft.meta.table };
  return { columns: draft.source, metaKeys: "", inMeta: "" };
}

export function referenceFormFor(source: Provenance, draft: RuleDraft): ReferenceForm {
  const when = draft.meta ? { column: draft.meta.keyColumn, value: draft.meta.key } : null;
  return {
    type: "reference",
    opener: openerText("r", source),
    replacing: null,
    base: null,
    source: { table: source.table, column: source.column },
    when,
    whenLocked: false,
    target: { table: "", column: "" },
    label: "",
  };
}

export function kindFormFor(source: Provenance, draft: RuleDraft): KindForm {
  return { type: "kind", opener: openerText("kind", source), index: null, scope: scopeOf(draft), kind: "enum", zone: "unix", currency: "" };
}

export function styleFormFor(source: Provenance, draft: RuleDraft, raw: string): StyleForm {
  return { type: "style", opener: openerText("style", source), index: null, scope: scopeOf(draft), match: "=", value: raw, color: "red" };
}

/** A single `column = 'value'`, the only `when` the form edits. */
function simpleWhen(when: Reference["when"]): WhenDraft | null {
  const entries = Object.entries(when ?? {});
  if (entries.length !== 1) return null;
  const [[column, match]] = entries;
  return typeof match === "string" ? { column, value: match } : null;
}

/** Edits a project row (`replacing` its id) or promotes an inferred one (a new project rule). */
export function referenceFormFromRow(row: ReferenceRow, opener: string): ReferenceForm | null {
  if (!row.rule) return null;
  const when = simpleWhen(row.rule.when);
  const hasWhen = Object.keys(row.rule.when ?? {}).length > 0;
  return {
    type: "reference",
    opener,
    replacing: row.origin.kind === "project" ? row.id : null,
    base: row.rule,
    source: { table: row.from.table, column: row.from.columns[0] ?? "" },
    when,
    whenLocked: hasWhen && !when,
    target: { table: row.to.table, column: row.to.columns[0] ?? "" },
    label: row.rule.label ?? "",
  };
}

function kindChoice(rule: KindRule): KindChoice {
  if (rule.money) return "money";
  if (rule.timestamp) return "timestamp";
  if (rule.secret) return "secret";
  if (rule.number) return "number";
  return "enum";
}

function scopeFromRule(rule: { columns?: string[]; meta_keys?: string[]; in?: string[] }): ScopeDraft {
  return { columns: joined(rule.columns), metaKeys: joined(rule.meta_keys), inMeta: joined(rule.in) };
}

export function kindFormFromRow(row: KindRow): KindForm {
  const rule = row.rule;
  return {
    type: "kind",
    opener: "edit",
    index: row.index,
    scope: scopeFromRule(rule),
    kind: kindChoice(rule),
    zone: rule.timestamp ?? "unix",
    currency: joined(rule.money?.currency),
  };
}

export function styleFormFromRow(row: StyleRow): StyleForm {
  const rule = row.rule;
  return { type: "style", opener: "edit", index: row.index, scope: scopeFromRule(rule), match: rule.match, value: rule.value, color: rule.color };
}

/** The name rules store for a real table; a table outside the prefix keeps its name and Rust refuses it. */
export function ruleTableName(tables: RuleTable[], table: string): string {
  return tables.find((t) => t.table === table)?.name ?? table;
}

const ruleColumn = (tables: RuleTable[], pick: ColumnPick) => `${ruleTableName(tables, pick.table)}.${pick.column}`;

export const referenceComplete = (form: ReferenceForm) => !!(form.source.table && form.source.column && form.target.table && form.target.column);

export function referenceRule(form: ReferenceForm, tables: RuleTable[]): Reference {
  const base = form.base;
  const when = form.whenLocked ? (base?.when ?? {}) : form.when && form.when.column ? { [form.when.column]: form.when.value } : {};
  return {
    id: "",
    from: ruleColumn(tables, form.source),
    when,
    to: ruleColumn(tables, form.target),
    to_when: base?.to_when ?? {},
    label: form.label.trim() || null,
    columns: base?.columns ?? [],
  };
}

function scopeRule(scope: ScopeDraft) {
  return { columns: list(scope.columns), meta_keys: list(scope.metaKeys), in: list(scope.inMeta) };
}

export function kindRule(form: KindForm): KindRule {
  const scope = scopeRule(form.scope);
  if (form.kind === "money") return { ...scope, money: { currency: list(form.currency) } };
  if (form.kind === "timestamp") return { ...scope, timestamp: form.zone };
  if (form.kind === "secret") return { ...scope, secret: true };
  if (form.kind === "number") return { ...scope, number: true };
  return { ...scope, enum: true };
}

export function styleRule(form: StyleForm): StyleRule {
  return { ...scopeRule(form.scope), match: form.match, value: form.value, color: form.color, enabled: true };
}

const FORM_FIELDS: Record<string, FormField> = {
  "reference.from": "source",
  "reference.when": "when",
  "reference.to": "target",
  "reference.to_when": "target",
  "reference.label": "label",
  "reference.id": "label",
  columns: "columns",
  "kind.columns": "columns",
  "style.columns": "columns",
  "kind.in": "inMeta",
  "style.in": "inMeta",
  "style.value": "value",
};

/** A rule field's path (`reference.when.meta_key`, `kind.in[0]`) up to the field the form shows. */
const formPath = (path: string) =>
  path
    .split(".")
    .slice(0, 2)
    .map((part) => part.split("[")[0])
    .join(".");

/** The form field Rust's error points at; anything else belongs to the whole form. */
export function fieldError({ field, message }: AppError): FieldError {
  const formField = field && FORM_FIELDS[formPath(field.path)];
  if (field && formField) return { field: formField, message: field.message };
  return { field: "form", message };
}

export function originText(origin: RuleOrigin): string {
  switch (origin.kind) {
    case "project":
      return "project";
    case "preset":
      return origin.preset;
    case "foreignKey":
      return `foreign key ${origin.constraint}`;
    case "inferred":
      return "inferred";
  }
}

/** `wc_orders.status, meta keys _wc_* in wc_orders_meta`, as the rule stores it. */
export function scopeText(rule: { columns?: string[]; meta_keys?: string[]; in?: string[] }): string {
  const parts: string[] = [];
  if (rule.columns?.length) parts.push(joined(rule.columns));
  if (rule.meta_keys?.length) parts.push(`meta keys ${joined(rule.meta_keys)}${rule.in?.length ? ` in ${joined(rule.in)}` : ""}`);
  return parts.join(" · ");
}

export function kindText(rule: KindRule): string {
  const kind = kindChoice(rule);
  if (kind === "money") return rule.money?.currency?.length ? `money · currency from ${joined(rule.money.currency)}` : "money";
  if (kind === "timestamp") return `timestamp · ${rule.timestamp}`;
  return kind;
}

export function styleMatchText(rule: StyleRule): string {
  return `${rule.match} '${rule.value}'`;
}
