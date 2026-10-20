import { act, render, screen } from "@testing-library/react";
import { vi } from "vitest";
import App from "../App";
import type { api, Document, Group, Value } from "../api";
import { address } from "../lib/address";
import { useStore } from "../state/store";
import { browseOrderTables } from "./objectFixtures";
import { connectedStore, num, settings, text, value } from "./fixtures";

const rowOf = (table: string, key: string) => address("shop", "db", { kind: "row", table, key });

export const order = rowOf("wp_wc_orders", "4711");
export const user = rowOf("wp_users", "12");

const source = (table: string, column: string, key: string, keyColumn: string) => ({ table, column, row: [{ column: keyColumn, value: key }] });

function documentOf(owner: typeof order, entity: string, keyColumn: string, fields: [string, Value][]): Document {
  const table = owner.target.kind === "row" ? owner.target.table : "";
  const key = owner.target.kind === "row" ? owner.target.key : "";
  const own: Group = {
    id: "own",
    table,
    via: { kind: "own" },
    shape: "fields",
    columns: fields.map(([name]) => name),
    total: 1,
    rows: [{ key: [{ column: keyColumn, value: key }], fields }],
    capped: false,
    rowsAddress: null,
    error: null,
  };
  return { address: owner, entity, header: { values: [] }, groups: [own], alternatives: [], actions: [] };
}

/** The reference `wp_wc_orders.customer_id = 12` that points at `wp_users #12`. */
const customerRef: Value = value({
  raw: num("12"),
  kind: { kind: "reference", targets: [user], by: { kind: "preset", preset: "woocommerce", id: "customer" }, exists: true },
  source: source("wp_wc_orders", "customer_id", "4711", "id"),
});

/** The set of orders `billing_email = ada@example.com`, which the order 4711 itself belongs to. */
export const emailSet = address("shop", "db", { kind: "table", table: "wp_wc_orders", filter: [{ column: "billing_email", value: "ada@example.com" }] });

/** `wp_wc_orders.billing_email` of the order 4711, a reference to every order with the same email. */
export const emailRef: Value = value({
  raw: text("ada@example.com"),
  kind: { kind: "reference", targets: [emailSet], by: { kind: "preset", preset: "woocommerce", id: "billing-email" }, exists: true },
  source: source("wp_wc_orders", "billing_email", "4711", "id"),
});

export const orderDocument = documentOf(order, "order", "id", [
  ["id", value({ raw: num("4711"), kind: { kind: "primaryKey", address: order }, source: source("wp_wc_orders", "id", "4711", "id") })],
  ["status", value({ raw: text("wc-processing"), source: source("wp_wc_orders", "status", "4711", "id") })],
  ["customer_id", customerRef],
]);

export const userDocument = documentOf(user, "user", "ID", [
  ["ID", value({ raw: num("12"), kind: { kind: "primaryKey", address: user }, source: source("wp_users", "ID", "12", "ID") })],
  ["user_login", value({ raw: text("ada"), source: source("wp_users", "user_login", "12", "ID") })],
]);

/** `document` answers the order 4711 and the user 12; `browseTable` the Rows view of the order. */
export function stubDocuments(mocked: typeof api) {
  vi.mocked(mocked.document).mockImplementation(async (_s, addressText) => (addressText.includes("/wp_users/") ? userDocument : orderDocument));
  vi.mocked(mocked.browseTable).mockImplementation(browseOrderTables);
}

/** Every element measures `width` px wide; returns the function that restores the default. */
export function stubOffsetWidth(width: number): () => void {
  const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth")!;
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => width });
  return () => Object.defineProperty(HTMLElement.prototype, "offsetWidth", original);
}

/** A connected store without tabs whose settings (also those the app loads) allow `maxPanes` panes side by side. */
export function connectWithMaxPanes(mocked: typeof api, maxPanes: number) {
  vi.mocked(mocked.getSettings).mockResolvedValue(settings({ maxPanes }));
  connectedStore({ tabs: [], settings: { maxPanes } });
}

/** Renders the app and opens the order 4711 as an object tab. */
export async function openOrder() {
  render(<App />);
  await screen.findByRole("tree");
  act(() => useStore.getState().openObject(order));
  await screen.findByRole("region", { name: "Object header" });
}
