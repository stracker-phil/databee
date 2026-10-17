import type { Address, BrowsePage, Cell, Document, Group, GroupLink, GroupRow, ObjectAction, Provenance, ReferenceSource, Value, ValueKind } from "../api";
import { address } from "../lib/address";
import { selectActiveTab, useStore } from "../state/store";
import { browsePage, columnMeta, num, resultSet, text, value } from "./fixtures";
import { session } from "./session";

const rowOf = (table: string, key: string): Address => address(session.slug, "db", { kind: "row", table, key });
const rowsOf = (table: string, column: string, v: string): Address => address(session.slug, "db", { kind: "table", table, filter: [{ column, value: v }] });

export const orderAddress = rowOf("wp_wc_orders", "4711");

export const source = (table: string, column: string, key: string, keyColumn = "id"): Provenance => ({ table, column, row: [{ column: keyColumn, value: key }] });

/** The `meta_value` of the `_paid_via` meta row, which the Document shows as `bacs`. */
export const paidViaSource = source("wp_wc_orders_meta", "meta_value", "91");

function stored(table: string, column: string, key: string, raw: Cell, kind: ValueKind = { kind: "text" }): Value {
  return value({ raw, kind, source: source(table, column, key) });
}

function group(patch: Partial<Group> & Pick<Group, "id" | "table" | "via">): Group {
  return { shape: "rows", columns: [], total: patch.rows?.length ?? 0, rows: [], capped: false, rowsAddress: null, error: null, ...patch };
}

/** An incoming link from `table.column` to the order. */
export const incomingLink = (table: string, column: string, label: string | null, by: ReferenceSource = { kind: "preset", preset: "woocommerce", id: `${table}-order` }): GroupLink => ({
  kind: "incoming",
  label,
  reference: { from: { table, columns: [column] }, to: { table: "wp_wc_orders", columns: ["id"] }, by },
});

const metaRow = (id: string, key: string, raw: string): GroupRow => ({
  key: [{ column: "id", value: id }],
  fields: [
    ["meta_key", stored("wp_wc_orders_meta", "meta_key", id, text(key))],
    ["meta_value", stored("wp_wc_orders_meta", "meta_value", id, text(raw))],
  ],
});

const addressRow = (id: string, type: string, city: string): GroupRow => ({
  key: [{ column: "id", value: id }],
  fields: [
    ["address_type", stored("wp_wc_order_addresses", "address_type", id, text(type))],
    ["city", stored("wp_wc_order_addresses", "city", id, text(city))],
  ],
});

const itemRow = (id: string, name: string): GroupRow => ({
  key: [{ column: "order_item_id", value: id }],
  fields: [
    ["order_item_id", value({ raw: num(id), kind: { kind: "number" }, source: source("wp_wc_order_items", "order_item_id", id, "order_item_id") })],
    ["order_item_name", value({ raw: text(name), source: source("wp_wc_order_items", "order_item_name", id, "order_item_id") })],
  ],
});

/** The groups of the order 4711 document, in the order Rust sends them. */
export const orderGroups = {
  own: group({
    id: "own",
    table: "wp_wc_orders",
    via: { kind: "own" },
    shape: "fields",
    columns: ["id", "status", "total_amount"],
    rows: [
      {
        key: [{ column: "id", value: "4711" }],
        fields: [
          ["id", stored("wp_wc_orders", "id", "4711", num("4711"), { kind: "primaryKey", address: orderAddress })],
          ["status", stored("wp_wc_orders", "status", "4711", text("wc-processing"))],
          ["total_amount", stored("wp_wc_orders", "total_amount", "4711", num("119.00"), { kind: "number" })],
        ],
      },
    ],
    rowsAddress: rowsOf("wp_wc_orders", "id", "4711"),
  }),
  meta: group({
    id: "meta",
    table: "wp_wc_orders_meta",
    via: { kind: "meta", fk: "order_id", key: "meta_key", value: "meta_value" },
    shape: "meta",
    columns: ["meta_key", "meta_value"],
    rows: [metaRow("91", "_paid_via", "bacs"), metaRow("92", "_gift_note", "Happy birthday")],
    rowsAddress: rowsOf("wp_wc_orders_meta", "order_id", "4711"),
  }),
  addresses: group({
    id: "addresses",
    table: "wp_wc_order_addresses",
    via: incomingLink("wp_wc_order_addresses", "order_id", "Addresses"),
    columns: ["address_type", "city"],
    rows: [addressRow("1", "billing", "Berlin"), addressRow("2", "shipping", "Hamburg")],
    rowsAddress: rowsOf("wp_wc_order_addresses", "order_id", "4711"),
  }),
  refunds: group({
    id: "refunds",
    table: "wp_wc_orders",
    via: incomingLink("wp_wc_orders", "parent_order_id", "Refunds"),
    columns: ["id", "status"],
    rows: [
      {
        key: [{ column: "id", value: "4712" }],
        fields: [
          ["id", stored("wp_wc_orders", "id", "4712", num("4712"), { kind: "primaryKey", address: rowOf("wp_wc_orders", "4712") })],
          ["status", stored("wp_wc_orders", "status", "4712", text("wc-refunded"))],
        ],
      },
    ],
    rowsAddress: rowsOf("wp_wc_orders", "parent_order_id", "4711"),
  }),
  items: group({
    id: "items",
    table: "wp_wc_order_items",
    via: incomingLink("wp_wc_order_items", "order_id", "Line items"),
    columns: ["order_item_id", "order_item_name"],
    rows: [itemRow("11", "Blue mug"), itemRow("12", "Red mug")],
    total: 30,
    capped: true,
    rowsAddress: rowsOf("wp_wc_order_items", "order_id", "4711"),
  }),
  coupons: group({
    id: "coupons",
    table: "wp_wc_order_coupon_lookup",
    via: incomingLink("wp_wc_order_coupon_lookup", "order_id", "Coupons"),
    columns: ["coupon_id"],
    rowsAddress: rowsOf("wp_wc_order_coupon_lookup", "order_id", "4711"),
  }),
  events: group({
    id: "events",
    table: "",
    via: { kind: "events" },
    columns: ["time"],
    rows: [{ key: null, fields: [["date_created_gmt", stored("wp_wc_orders", "date_created_gmt", "4711", text("2025-01-02 10:00:00"), { kind: "timestamp", utc: null, zone: "utc" })]] }],
  }),
};

export const markCompleted: ObjectAction = { name: "mark-completed", preset: "woocommerce", kind: "update", undo: true, params: [] };
export const purgeNotes: ObjectAction = { name: "purge-notes", preset: "woocommerce", kind: "delete", undo: false, params: [] };
export const setStatus: ObjectAction = {
  name: "set-status",
  preset: "woocommerce",
  kind: "update",
  undo: true,
  params: [{ name: "status", type: "text", choices: ["wc-completed", "wc-cancelled"], free: false, default: null, optional: false, error: null }],
};

/** The `document` reply for order 4711: own, meta, three incoming groups (one capped, one empty) and events. */
export function objectDocument(patch: Partial<Document> = {}): Document {
  return {
    address: orderAddress,
    entity: "order",
    header: [],
    groups: Object.values(orderGroups),
    alternatives: [],
    actions: [markCompleted, purgeNotes],
    ...patch,
  };
}

const keyOf = (id: string) => [{ column: "id", value: id }];

const orderPage: BrowsePage = browsePage({
  table: "wp_wc_orders",
  result: resultSet({
    columns: [columnMeta("id", "BIGINT"), columnMeta("status", "VARCHAR"), columnMeta("total_amount", "DECIMAL")],
    rows: [[num("4711"), text("wc-processing"), num("119.00")]],
    totalRows: 1,
  }),
  keys: [keyOf("4711")],
});

const metaPage: BrowsePage = browsePage({
  table: "wp_wc_orders_meta",
  result: resultSet({
    columns: [columnMeta("id", "BIGINT"), columnMeta("order_id", "BIGINT"), columnMeta("meta_key", "VARCHAR"), columnMeta("meta_value", "TEXT")],
    rows: [
      [num("91"), num("4711"), text("_paid_via"), text("bacs")],
      [num("92"), num("4711"), text("_gift_note"), text("Happy birthday")],
    ],
    totalRows: 2,
  }),
  keys: [keyOf("91"), keyOf("92")],
});

/** `browseTable` for the Rows view: real pages for the order and its meta, no rows for the other groups. */
export async function browseOrderTables(_session: string, req: { table: string }): Promise<BrowsePage> {
  if (req.table === "wp_wc_orders") return orderPage;
  if (req.table === "wp_wc_orders_meta") return metaPage;
  return browsePage({ table: req.table, result: resultSet({ columns: [columnMeta("id", "BIGINT")], rows: [], totalRows: 0 }) });
}

/** The active tab, which must be an object tab, with the `view` and `focus` of its active (object) column. */
export function activeObject() {
  const tab = selectActiveTab(useStore.getState());
  if (tab?.kind !== "object") throw new Error("the active tab is not an object tab");
  const column = tab.columns[tab.active];
  if (column.kind !== "object") throw new Error("the active column is not an object column");
  return { ...tab, view: column.view, focus: column.focus };
}
