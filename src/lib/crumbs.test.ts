import { describe, expect, it } from "vitest";
import { routeCrumbs } from "./crumbs";

describe("routeCrumbs", () => {
  it("shows the hierarchy of a route, not where the user came from", () => {
    expect(routeCrumbs({ screen: "list", list: "settings" }).map((c) => c.label)).toEqual(["Options"]);
    expect(routeCrumbs({ screen: "list", list: "actions", params: { status: "failed" } }).map((c) => c.label)).toEqual(["Scheduled actions", "Failed"]);
    expect(routeCrumbs({ screen: "list", list: "orders", params: { status: "wc-on-hold" } }).map((c) => c.label)).toEqual(["Orders", "On hold"]);
  });

  it("links the list crumb back to the unfiltered list", () => {
    const crumbs = routeCrumbs({ screen: "list", list: "products", params: { search: "mug" } });
    expect(crumbs.map((c) => c.label)).toEqual(["Products", "“mug”"]);
    expect(crumbs[0].route).toEqual({ screen: "list", list: "products" });
    expect(crumbs[1].route).toBeUndefined();
  });

  it("names the searched value on the find screen", () => {
    expect(routeCrumbs({ screen: "find", value: "66M12696JX411763D" }).map((c) => c.label)).toEqual(["Find everywhere", "“66M12696JX411763D”"]);
  });

  it("never repeats an item", () => {
    const routes = [
      { screen: "find", value: "shop" },
      { screen: "list", list: "content", params: { kind: "page", search: "shop" } },
    ] as const;
    for (const route of routes) {
      const labels = routeCrumbs(route).map((c) => c.label);
      expect(new Set(labels).size).toBe(labels.length);
    }
  });
});
