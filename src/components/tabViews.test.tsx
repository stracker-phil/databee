import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import type { Address } from "../api";
import { formatAddress } from "../lib/address";
import { objectColumn } from "../lib/columns";
import { documentText } from "../lib/document";
import type { ObjectTab, ProjectTab, Tab } from "../state/store";
import { queryTab, readoutTab, session, tableTab } from "../test/fixtures";
import { tabView } from "./tabViews";

const rowAddress: Address = {
  connection: session.slug,
  database: "db",
  target: { kind: "row", table: "wp_posts", key: "7" },
  view: null,
  compare: null,
};

const objectTab = (patch: Partial<ObjectTab> = {}): ObjectTab => ({
  id: "o1",
  kind: "object",
  sessionId: session.id,
  title: "wp_posts #7",
  columns: [objectColumn(rowAddress)],
  active: 0,
  address: rowAddress,
  stacks: {},
  nav: { back: [], forward: [] },
  ...patch,
});

const projectTab: ProjectTab = {
  id: "p1",
  kind: "project",
  sessionId: session.id,
  title: "project",
  database: "shop",
  address: { connection: session.slug, database: "shop", target: { kind: "project" }, view: null, compare: null },
  form: null,
  nav: { back: [], forward: [] },
};

describe("table view", () => {
  const view = tabView(tableTab());

  it("names the table and the stage anchor by database and table", () => {
    const tab = tableTab({ database: "shop", table: "wp_posts" });
    expect(view.stage(tab)).toBe("tab:table:shop.wp_posts");
    expect(view.tooltip(tab)).toBe("shop.wp_posts");
  });

  it("lists column matches, the filter and the preview hint in the tooltip", () => {
    const tab = tableTab({
      database: "shop",
      table: "wp_postmeta",
      where: [
        { column: "post_id", value: "7" },
        { column: "meta_value", value: null },
      ],
      filter: "meta_key = 'x'",
      preview: true,
    });
    expect(view.tooltip(tab)).toBe("shop.wp_postmeta · post_id: 7 · meta_value: NULL · meta_key = 'x' · preview (double-click to keep)");
  });

  it("shows a filtered hint only for a typed filter", () => {
    const hint = (filter?: string) => render(<>{view.addressHint?.(tableTab({ filter }))}</>).container.textContent;
    expect(hint("post_status = 'draft'")).toBe("· filtered");
    expect(hint(undefined)).toBe("");
    expect(hint("")).toBe("");
  });
});

describe("object view", () => {
  it("anchors the stage id at the document of the first object column", () => {
    const first: Address = { ...rowAddress, target: { kind: "row", table: "wp_posts", key: "1" } };
    const tab = objectTab({ columns: [objectColumn(first), objectColumn(rowAddress)], active: 1 });
    expect(tabView(tab).stage(tab)).toBe(`tab:object:${documentText(first)}`);
  });

  it("uses the formatted address as tooltip, with the preview hint for a preview tab", () => {
    const plain = objectTab();
    const preview = objectTab({ preview: true });
    expect(tabView(plain).tooltip(plain)).toBe(formatAddress(rowAddress));
    expect(tabView(preview).tooltip(preview)).toBe(`${formatAddress(rowAddress)} · preview (double-click to keep)`);
  });
});

describe("project and readout views", () => {
  it("anchor the stage id by database and use the formatted address as tooltip", () => {
    expect(tabView(projectTab).stage(projectTab)).toBe("tab:project:shop");
    expect(tabView(projectTab).tooltip(projectTab)).toBe(formatAddress(projectTab.address));

    const readout = readoutTab({ database: "shop" });
    expect(tabView(readout).stage(readout)).toBe("tab:readout:shop");
    expect(tabView(readout).tooltip(readout)).toBe(formatAddress(readout.address));
  });
});

describe("query view", () => {
  it("anchors the stage id by tab id and uses the title as tooltip", () => {
    const tab = queryTab({ id: "q9", title: "Slow orders" });
    expect(tabView(tab).stage(tab)).toBe("tab:query:q9");
    expect(tabView(tab).tooltip(tab)).toBe("Slow orders");
  });
});

describe("tabView", () => {
  it("resolves a tab to the view of its kind", () => {
    const table = tableTab({ database: "shop", table: "wp_posts" });
    const readout = readoutTab({ database: "shop" });
    expect(tabView(table).stage(table)).toBe("tab:table:shop.wp_posts");
    expect(tabView(readout).stage(readout)).toBe("tab:readout:shop");
  });

  it("gives only the query view a summary", () => {
    const tabs: Tab[] = [queryTab(), tableTab(), objectTab(), projectTab, readoutTab()];
    expect(tabs.filter((tab) => tabView(tab).Summary).map((tab) => tab.kind)).toEqual(["query"]);
  });
});
