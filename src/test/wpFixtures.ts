import type { DocField, EntityDoc, OrderView, WpInfo } from "../api";
import { useStore } from "../state/store";
import { connectedStore, num, session, text } from "./fixtures";
import { NOW } from "./now";

export { NOW };
const NOW_SECS = Math.round(NOW / 1000);

export const wpInfo: WpInfo = {
  context: {
    prefix: "wp_",
    version: "58975",
    sites: [],
    wc: { version: "9.3.0", hpos: "on_with_sync", hasActionScheduler: true },
    site: { name: "Shop", url: "https://shop.test", currency: "EUR", zone: "+00:00", zoneSource: "gmt_offset", theme: "storefront", activePlugins: { source: "active_plugins", plugins: ["woocommerce"] }, networkPlugins: null, uploadsUrl: null, priceDecimals: 2 },
  },
  entities: [
    { name: "order", label: "Order" },
    { name: "user", label: "User" },
  ],
  tableEntities: { wp_wc_orders: { entity: "order", idColumn: "id" } },
};

/** A connected session on a WordPress database, in WordPress mode. */
export function wpStore() {
  connectedStore();
  useStore.setState({ wp: { [`${session.id}/db`]: wpInfo }, mode: "wp" });
}

export function field(name: string, value: string | null, patch: Partial<DocField> = {}): DocField {
  return {
    name,
    label: null,
    value: value == null ? { t: "null" } : /^\d+(\.\d+)?$/.test(value) ? num(value) : text(value),
    decoded: null,
    link: null,
    hint: null,
    sensitive: null,
    autoload: null,
    ...patch,
  };
}

export function doc(patch: Partial<EntityDoc> = {}): EntityDoc {
  return {
    entity: "order",
    label: "Order",
    id: "4711",
    title: "jane@shop.test",
    subtitle: "wc-processing",
    image: null,
    table: "wp_wc_orders",
    idColumn: "id",
    fields: [field("id", "4711"), field("status", "wc-processing")],
    meta: [{ name: "_ppcp*", private: true, entries: [field("_ppcp_paypal_order_id", "66M12696JX411763D")] }],
    metaSource: { table: "wp_wc_orders_meta", fk: "order_id", key: "meta_key" },
    relations: [],
    ...patch,
  };
}

export const orderView: { view: "order" } & OrderView = {
  view: "order",
  doc: doc(),
  kind: "shop_order",
  status: "wc-processing",
  total: "€49.90",
  currency: "EUR",
  created: NOW_SECS - 3600,
  paid: NOW_SECS - 3500,
  completed: null,
  customer: { entity: "user", id: "12", label: "Jane Doe", image: null },
  customerEmail: "jane@shop.test",
  parent: null,
  billing: { name: "Jane Doe", company: null, lines: ["Ring 1", "1010 Vienna"], country: "AT", email: "jane@shop.test", phone: null },
  shipping: { name: "Jane Doe", company: null, lines: ["Marktstraße 1", "6850 Dornbirn"], country: "AT", email: null, phone: null },
  addressNotes: ["City/postcode differs"],
  items: [{ id: "1", name: "Mug", kind: "line_item", quantity: "2", total: "€45.00", product: { entity: "post", id: "123", label: "Mug", image: "https://shop.test/mug.jpg" }, sku: "MUG-1", meta: [] }],
  totals: [
    ["Subtotal", "€45.00"],
    ["Total", "€49.90"],
  ],
  payment: {
    gateway: "ppcp-gateway",
    gatewayTitle: "PayPal",
    transactionId: "61P3809224872131H",
    paypalOrderId: "66M12696JX411763D",
    intent: "CAPTURE",
    mode: "sandbox",
    source: "paypal",
    payerEmail: "jane@shop.test",
    fees: { gross: "€49.90", fee: "€2.04", net: "€47.86", feeRatio: 0.04 },
  },
  attribution: null,
  timeline: [{ at: NOW_SECS - 3600, kind: "created", title: "Order created", detail: null, level: "info", link: null }],
  related: [{ label: "Notes", link: null, count: 1, target: { kind: "timeline" } }],
};
