import type { DocField, Document, WpInfo } from "../api";
import { address } from "../lib/address";
import { useStore } from "../state/store";
import { connectedStore, num, session, text } from "./fixtures";
import { NOW } from "./now";

export { NOW };

export const wpInfo: WpInfo = {
  prefix: "wp_",
  presets: ["wordpress", "woocommerce"],
  entities: ["order", "user"],
  tableEntities: { wp_wc_orders: { entity: "order", idColumn: "id" } },
  legacyEntitiesFile: null,
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

/** The `document` reply for `order <key>`: the row `wp_wc_orders/<key>` with no groups. */
export function orderDocument(key = "4711", patch: Partial<Document> = {}): Document {
  return {
    address: address(session.slug, "db", { kind: "row", table: "wp_wc_orders", key }),
    entity: "order",
    header: [],
    groups: [],
    alternatives: [],
    actions: [],
    ...patch,
  };
}
