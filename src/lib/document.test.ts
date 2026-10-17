import { describe, expect, it } from "vitest";
import type { Group } from "../api";
import { address } from "./address";
import { countText, documentText, findValue, focusOrder, groupAnchors, moreText, objectAddress, objectTitle, originText, provenanceKey, relationText } from "./document";
import { incomingLink, objectDocument, orderAddress, orderGroups, paidViaSource, source } from "../test/objectFixtures";

const { own, meta, addresses, refunds, items, events, coupons } = orderGroups;

const viaOf = (via: Group["via"]): Group => ({ ...addresses, via });

describe("objectAddress", () => {
  it("names the Rows view with a parameter and the document with none", () => {
    const row = { ...orderAddress, compare: "4712" };
    expect(objectAddress(row, "rows")).toMatchObject({ view: "rows", compare: null });
    expect(objectAddress({ ...row, view: "rows" }, "document")).toMatchObject({ view: null, compare: null });
  });
});

describe("documentText", () => {
  it("is the same for both views of a row", () => {
    expect(documentText({ ...orderAddress, view: "rows" })).toBe("databee://shop/db/wp_wc_orders/4711");
    expect(documentText(orderAddress)).toBe("databee://shop/db/wp_wc_orders/4711");
  });
});

describe("objectTitle", () => {
  it.each([
    ["the table until the entity is known", undefined, "wp_wc_orders #4711"],
    ["the entity once it is known", "order", "order #4711"],
  ])("shows %s", (_name, entity, title) => {
    expect(objectTitle(orderAddress, entity)).toBe(title);
  });

  it("falls back to the address text for a target that is not a row", () => {
    expect(objectTitle(address("shop", "db", { kind: "table", table: "wp_posts", filter: [] }))).toBe("databee://shop/db/wp_posts");
  });
});

describe("provenanceKey", () => {
  it("joins table, column and the row key", () => {
    expect(provenanceKey(paidViaSource)).toBe("wp_wc_orders_meta.meta_value:id=91");
  });

  it("keeps every part of a composite key", () => {
    expect(provenanceKey({ table: "t", column: "c", row: [{ column: "a", value: "1" }, { column: "b", value: "2" }] })).toBe("t.c:a=1,b=2");
  });
});

describe("findValue", () => {
  const document = objectDocument();

  it("finds a value in a meta row by its provenance", () => {
    expect(findValue(document, paidViaSource)?.raw).toEqual({ t: "text", v: "bacs" });
  });

  it("finds a value in the events group", () => {
    const found = findValue(document, source("wp_wc_orders", "date_created_gmt", "4711"));
    expect(found?.raw).toEqual({ t: "text", v: "2025-01-02 10:00:00" });
  });

  it("does not answer from the previous document when the next one lacks the value", () => {
    const other = objectDocument({ groups: [meta] });
    expect(findValue(document, source("wp_wc_orders", "status", "4711"))?.raw).toEqual({ t: "text", v: "wc-processing" });
    expect(findValue(other, source("wp_wc_orders", "status", "4711"))).toBeNull();
  });

  it("returns null for a provenance the document does not hold", () => {
    expect(findValue(document, source("wp_wc_orders_meta", "meta_value", "999"))).toBeNull();
  });
});

describe("relationText", () => {
  it.each([
    ["the own row", own, "this row"],
    ["a meta group", meta, "meta · order_id"],
    ["a labelled incoming reference", addresses, "Addresses · order_id → this row"],
    ["an unlabelled incoming reference", viaOf(incomingLink("wp_wc_order_addresses", "order_id", null)), "order_id → this row"],
    ["a pattern", viaOf({ kind: "pattern", rule: "gift-notes", sql: "SELECT 1" }), "pattern · SELECT 1"],
    ["the events", events, "every stored timestamp, in order"],
  ])("describes %s", (_name, group, text) => {
    expect(relationText(group)).toBe(text);
  });

  it("marks a reference guessed from a column name as inferred", () => {
    const guessed = viaOf(incomingLink("wp_wc_order_addresses", "order_id", "Addresses", { kind: "sameName", column: "order_id", indexed: true }));
    expect(relationText(guessed)).toBe("Addresses · order_id → this row · inferred");
  });
});

describe("originText", () => {
  it("names the rule behind an incoming reference", () => {
    expect(originText(addresses)).toBe("wp_wc_order_addresses.order_id → wp_wc_orders.id · preset woocommerce");
  });

  it("names the meta columns and the foreign key", () => {
    expect(originText(meta)).toBe("wp_wc_orders_meta.meta_key → wp_wc_orders_meta.meta_value · order_id → this row");
  });

  it("is null for the own row and the events", () => {
    expect(originText(own)).toBeNull();
    expect(originText(events)).toBeNull();
  });
});

describe("countText", () => {
  it.each([
    ["the columns of the own row", own, "3 columns"],
    ["the keys of a meta group", meta, "2 keys · stored order"],
    ["a single row", refunds, "1 row"],
    ["all rows of a capped group", items, "30 rows"],
    ["a bare number for events", events, "1"],
    ["thousands with a separator", { ...items, total: 1200 }, "1,200 rows"],
    ["a single meta key", { ...meta, total: 1 }, "1 key · stored order"],
  ])("counts %s", (_name, group, text) => {
    expect(countText(group)).toBe(text);
  });
});

describe("moreText", () => {
  it("counts the rows a capped group cuts", () => {
    expect(moreText(items)).toBe("+28 rows");
  });

  it("counts cut meta keys as keys, one in the singular", () => {
    expect(moreText({ ...meta, capped: true, total: 3 })).toBe("+1 key");
    expect(moreText({ ...meta, capped: true, total: 40 })).toBe("+38 keys");
  });

  it("is null when the group is not capped or nothing is cut", () => {
    expect(moreText(meta)).toBeNull();
    expect(moreText({ ...items, total: 2 })).toBeNull();
  });
});

describe("focusOrder", () => {
  const groups = [own, meta, addresses, events, coupons];
  const tables = (collapsed: string[]) => focusOrder(objectDocument({ groups }), (id) => collapsed.includes(id)).map((p) => `${p.table}.${p.column}`);

  it("follows group order, then row order, without the events or empty groups", () => {
    expect(tables([])).toEqual([
      "wp_wc_orders.id",
      "wp_wc_orders.status",
      "wp_wc_orders.total_amount",
      "wp_wc_orders_meta.meta_value",
      "wp_wc_orders_meta.meta_value",
      "wp_wc_order_addresses.address_type",
      "wp_wc_order_addresses.city",
      "wp_wc_order_addresses.address_type",
      "wp_wc_order_addresses.city",
    ]);
  });

  it("leaves out the values of a collapsed group", () => {
    const order = tables(["meta"]);
    expect(order).not.toContain("wp_wc_orders_meta.meta_value");
    expect(order).toHaveLength(7);
    expect(order[3]).toBe("wp_wc_order_addresses.address_type");
  });
});

describe("groupAnchors", () => {
  it("adds the group id when a second group shows the same table", () => {
    const anchors = groupAnchors([own, meta, refunds, events]);
    expect(anchors.get("own")).toBe("group-wp_wc_orders");
    expect(anchors.get("meta")).toBe("group-wp_wc_orders_meta");
    expect(anchors.get("refunds")).toBe("group-wp_wc_orders:refunds");
  });

  it("names the events group by its id, as it has no table", () => {
    expect(groupAnchors([events]).get("events")).toBe("group-events");
  });
});
