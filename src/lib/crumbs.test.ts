import { describe, expect, it } from "vitest";
import { routeCrumbs } from "./crumbs";

describe("routeCrumbs", () => {
  it("shows the hierarchy of a route, not where the user came from", () => {
    expect(routeCrumbs({ screen: "entity", entity: "order", id: "3226" }).map((c) => c.label)).toEqual(["Orders", "#3226"]);
    expect(routeCrumbs({ screen: "list", list: "settings" }).map((c) => c.label)).toEqual(["Settings & options"]);
    expect(routeCrumbs({ screen: "list", list: "actions", params: { status: "failed" } }).map((c) => c.label)).toEqual(["Scheduled actions", "Failed"]);
    expect(routeCrumbs({ screen: "list", list: "orders", params: { status: "wc-on-hold" } }).map((c) => c.label)).toEqual(["Orders", "On hold"]);
  });

  it("uses the loaded entity's list and title when known", () => {
    const crumbs = routeCrumbs({ screen: "entity", entity: "post", id: "2290" }, { parent: "products", title: "Aperiam Maiores" });
    expect(crumbs.map((c) => c.label)).toEqual(["Products", "Aperiam Maiores"]);
    expect(crumbs[0].route).toEqual({ screen: "list", list: "products" });
    expect(crumbs[1].route).toBeUndefined();
  });

  it("never repeats an item", () => {
    const routes = [
      { screen: "entity", entity: "user", id: "1" },
      { screen: "compare", entity: "order", a: "1", b: "2" },
      { screen: "list", list: "content", params: { kind: "page", search: "shop" } },
    ] as const;
    for (const route of routes) {
      const labels = routeCrumbs(route).map((c) => c.label);
      expect(new Set(labels).size).toBe(labels.length);
    }
  });
});
