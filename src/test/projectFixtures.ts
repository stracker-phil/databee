import type { PresetRow, ProjectRules, Reference, ReferenceRow, ResolvedReference, RuleTable, TableStructure } from "../api";

export const ruleTables: RuleTable[] = [
  { table: "wp_wc_orders", name: "wc_orders" },
  { table: "wp_wc_orders_meta", name: "wc_orders_meta" },
  { table: "wp_wc_order_items", name: "wc_order_items" },
  { table: "wp_users", name: "users" },
];

const COLUMNS: Record<string, string[]> = {
  wp_wc_orders: ["id", "status"],
  wp_wc_orders_meta: ["id", "order_id", "meta_key", "meta_value"],
  wp_wc_order_items: ["order_item_id", "order_id"],
  wp_users: ["ID", "user_login"],
};

const KEYS: Record<string, string[]> = { wp_wc_orders: ["id"], wp_wc_orders_meta: ["id"], wp_wc_order_items: ["order_item_id"], wp_users: ["ID"] };

/** `describeTable` for the tables of `ruleTables`. */
export async function describeRuleTable(_session: string, _database: string, table: string): Promise<TableStructure> {
  return {
    columns: (COLUMNS[table] ?? []).map((name) => ({ name, columnType: "bigint", nullable: false, default: null, key: "", extra: "", comment: "", autoIncrement: false, generated: false })),
    indexes: [],
    primaryKey: KEYS[table] ?? [],
    foreignKeys: [],
    ddl: null,
  };
}

export const orderItemsRule: Reference = { id: "x", from: "wc_order_items.order_id", when: {}, to: "wc_orders.id", to_when: {}, label: null, columns: [] };

const orderItemsResolved: ResolvedReference = {
  from: { table: "wp_wc_order_items", columns: ["order_id"] },
  to: { table: "wp_wc_orders", columns: ["id"] },
  by: { kind: "preset", preset: "woocommerce", id: "x" },
};

/** A preset rule `wp_wc_order_items.order_id → wp_wc_orders.id` with id `x`; override fields per test. */
export function referenceRow(patch: Partial<ReferenceRow> = {}): ReferenceRow {
  return {
    origin: { kind: "preset", preset: "woocommerce" },
    id: "x",
    from: orderItemsResolved.from,
    to: orderItemsResolved.to,
    when: "",
    toWhen: "",
    label: null,
    disabled: false,
    reference: orderItemsResolved,
    rule: orderItemsRule,
    ...patch,
  };
}

export const woocommercePreset: PresetRow = { name: "woocommerce", markerTables: ["wp_wc_orders"], applied: true };

export function projectRules(patch: Partial<ProjectRules> = {}): ProjectRules {
  return {
    project: { connection: "shop", database: "db", prefix: null, network_prefix: null, presets: null, references: [], disabled: [], kinds: [], styles: [], saved: [] },
    prefix: "wp_",
    suggestedPrefix: "wp_",
    presets: [woocommercePreset],
    foreignKeys: 0,
    references: [],
    kinds: [],
    styles: [],
    tables: ruleTables,
    ...patch,
  };
}
