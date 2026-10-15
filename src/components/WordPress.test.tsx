import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type DecodedValue, type EntityDoc, type WpInfo } from "../api";
import { useStore } from "../state/store";
import { connectedStore, num, session, text } from "../test/fixtures";
import { DocumentTab } from "./DocumentTab";
import { Inspector } from "./Inspector";
import { OptionsTab } from "./OptionsTab";
import { TableTab } from "./TableTab";
import { ValueTree } from "./ValueTree";
import { WordPressPanel } from "./WordPressPanel";

const mocked = vi.mocked(api, true);

const wpInfo: WpInfo = {
  context: { prefix: "wp_", version: "58975", sites: [], wc: { version: "9.3.0", hpos: "on_with_sync", hasActionScheduler: true } },
  entities: [
    ["post", "Post"],
    ["order", "Order"],
  ],
  tableEntities: { wp_wc_orders: { entity: "order", idColumn: "id" } },
};

const decoded: DecodedValue = {
  format: "php",
  root: {
    kind: "array",
    children: [
      { key: "enabled", keyKind: "string", node: { kind: "string", text: "yes" } },
      { key: "fee", keyKind: "string", node: { kind: "float", text: "1.2" } },
    ],
  },
  warning: null,
  error: null,
};

const order: EntityDoc = {
  entity: "order",
  label: "Order",
  id: "4711",
  title: "jane@shop.test",
  subtitle: "wc-processing",
  table: "wp_wc_orders",
  idColumn: "id",
  fields: [
    { name: "id", value: num("4711"), decoded: null, link: null },
    { name: "customer_id", value: num("12"), decoded: null, link: { entity: "user", id: "12" } },
  ],
  meta: [
    { name: "billing", private: false, entries: [{ name: "billing_country", value: text("AT"), decoded: null, link: null }] },
    { name: "ppcp", private: true, entries: [{ name: "_ppcp_fees", value: text("a:…"), decoded, link: null }] },
  ],
  relations: [{ name: "items", label: "Items", entity: "order_item", count: 2 }],
};

function withWp() {
  connectedStore();
  useStore.setState({ wp: { [`${session.id}/db`]: wpInfo } });
}

describe("ValueTree", () => {
  it("renders keys and typed values of decoded data", () => {
    render(<ValueTree value={decoded} />);
    const tree = screen.getByRole("tree", { name: "PHP value" });
    expect(within(tree).getByText("enabled")).toBeInTheDocument();
    expect(within(tree).getByText('"yes"')).toBeInTheDocument();
    expect(within(tree).getByText("1.2")).toBeInTheDocument();
  });

  it("shows decode errors and lenient-decoding warnings", () => {
    render(<ValueTree value={{ format: "php", root: null, warning: null, error: "unexpected end at byte 9" }} />);
    expect(screen.getByRole("alert")).toHaveTextContent("unexpected end at byte 9");
  });
});

describe("DocumentTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    withWp();
    mocked.wpLoad.mockResolvedValue([order]);
  });

  const tab = { id: "d1", kind: "document" as const, sessionId: "s1", title: "Order #4711", database: "db", entity: "order", entityId: "4711" };

  it("shows fields, public and private meta with decoded values", async () => {
    render(<DocumentTab tab={tab} />);
    expect(await screen.findByText("jane@shop.test")).toBeInTheDocument();
    expect(mocked.wpLoad).toHaveBeenCalledWith("s1", "db", "order", ["4711"]);
    expect(screen.getByText("billing_country")).toBeInTheDocument();
    expect(screen.getByText("Private meta")).toBeInTheDocument();
    expect(screen.getByRole("tree", { name: "PHP value" })).toBeInTheDocument();
  });

  it("follows links to other documents", async () => {
    render(<DocumentTab tab={tab} />);
    await userEvent.click(await screen.findByRole("button", { name: "Open user 12" }));
    const opened = useStore.getState().tabs.at(-1)!;
    expect(opened.kind === "document" && [opened.entity, opened.entityId]).toEqual(["user", "12"]);
  });

  it("loads related documents only when a relation is opened", async () => {
    mocked.wpExpand.mockResolvedValue([{ ...order, entity: "order_item", label: "Order item", id: "1", title: "Mug", relations: [], meta: [] }]);
    const { container } = render(<DocumentTab tab={tab} />);
    await screen.findByText("Items");
    expect(mocked.wpExpand).not.toHaveBeenCalled();
    const details = container.querySelector("details.relation") as HTMLDetailsElement;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(await screen.findByText("Mug")).toBeInTheDocument();
    expect(mocked.wpExpand).toHaveBeenCalledWith("s1", "db", "order", "4711", "items");
  });

  it("jumps to the source row in the table view", async () => {
    render(<DocumentTab tab={tab} />);
    await userEvent.click(await screen.findByRole("button", { name: /Source row/ }));
    const opened = useStore.getState().tabs.at(-1)!;
    expect(opened.kind === "table" && [opened.table, opened.filter]).toEqual(["wp_wc_orders", "`id` = 4711"]);
  });

  it("reports missing documents", async () => {
    mocked.wpLoad.mockResolvedValue([]);
    render(<DocumentTab tab={tab} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("order #4711 was not found");
  });
});

describe("WordPressPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    withWp();
  });

  it("shows the detected install and opens documents by ID", async () => {
    render(<WordPressPanel />);
    expect(screen.getByText("HPOS + sync")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Entity"), "order");
    await userEvent.type(screen.getByLabelText("ID"), "4711");
    await userEvent.click(screen.getByRole("button", { name: "Open document" }));
    const opened = useStore.getState().tabs.at(-1)!;
    expect(opened.title).toBe("Order #4711");
  });

  it("explains when a database is not WordPress", () => {
    connectedStore();
    useStore.setState({ wp: { [`${session.id}/db`]: null } });
    render(<WordPressPanel />);
    expect(screen.getByText("db is not a WordPress database.")).toBeInTheDocument();
  });
});

describe("OptionsTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    withWp();
    mocked.wpOptions.mockResolvedValue({
      table: "wp_options",
      totalCount: 741,
      autoloadBytes: 2048,
      groups: [{ prefix: "woocommerce", count: 300, bytes: 4096, autoloadCount: 20, autoloadBytes: 2048 }],
      transients: [
        { name: "old_cache", siteWide: false, bytes: 10, expiresAt: 1000, expired: true },
        { name: "forever", siteWide: false, bytes: 5, expiresAt: null, expired: false },
      ],
    });
  });

  const tab = { id: "o1", kind: "options" as const, sessionId: "s1", title: "Options", database: "db" };

  it("summarizes autoload weight and transients", async () => {
    render(<OptionsTab tab={tab} />);
    expect(await screen.findByText("741")).toBeInTheDocument();
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
    expect(screen.getByText("woocommerce")).toBeInTheDocument();
    expect(screen.getByText("never expires")).toBeInTheDocument();
  });

  it("opens cleanup SQL for review instead of deleting", async () => {
    mocked.wpTransientCleanupSql.mockResolvedValue("DELETE FROM `db`.`wp_options` WHERE …");
    render(<OptionsTab tab={tab} />);
    await userEvent.click(await screen.findByRole("button", { name: /Cleanup SQL for 1 expired/ }));
    const opened = useStore.getState().tabs.at(-1)!;
    expect(opened.kind === "query" && opened.sql).toContain("DELETE FROM");
    expect(mocked.execute).not.toHaveBeenCalled();
  });

  it("opens a prefix group as a filtered table", async () => {
    render(<OptionsTab tab={tab} />);
    await userEvent.click(await screen.findByText("woocommerce"));
    const opened = useStore.getState().tabs.at(-1)!;
    expect(opened.kind === "table" && opened.filter).toContain("option_name LIKE 'woocommerce%'");
  });
});

describe("Inspector", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("decodes serialized cell values and can show the raw text", async () => {
    mocked.decodeValue.mockResolvedValue(decoded);
    useStore.setState({ selectedCell: { column: { name: "option_value", typeName: "LONGTEXT" }, cell: text('a:2:{s:7:"enabled";…}') } });
    render(<Inspector />);
    expect(await screen.findByRole("tree", { name: "PHP value" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Raw" }));
    expect(screen.getByText('a:2:{s:7:"enabled";…}')).toBeInTheDocument();
  });

  it("shows plain values as text", async () => {
    useStore.setState({ selectedCell: { column: { name: "siteurl", typeName: "TEXT" }, cell: text("https://shop.test") } });
    render(<Inspector />);
    expect(screen.getByText("https://shop.test")).toBeInTheDocument();
    expect(screen.queryByRole("tree")).not.toBeInTheDocument();
  });
});

describe("TableTab with WordPress entities", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    withWp();
  });

  it("opens a row as a document from its context menu", async () => {
    mocked.browseTable.mockResolvedValue({
      result: {
        columns: [{ name: "id", typeName: "BIGINT UNSIGNED" }, { name: "status", typeName: "VARCHAR" }],
        rows: [[num("4711"), text("wc-processing")]],
        totalRows: 1,
        truncated: false,
        affectedRows: 0,
        lastInsertId: null,
        returnsRows: true,
      },
      hasMore: false,
    });
    render(<TableTab tab={{ id: "t1", kind: "table", sessionId: "s1", title: "wp_wc_orders", database: "db", table: "wp_wc_orders", view: "data" }} />);
    fireEvent.contextMenu(await screen.findByText("wc-processing"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Open order #4711 as document" }));
    const opened = useStore.getState().tabs.at(-1)!;
    expect(opened.kind === "document" && opened.entityId).toBe("4711");
  });

  it("applies an initial filter", async () => {
    mocked.browseTable.mockResolvedValue({ result: { columns: [], rows: [], totalRows: 0, truncated: false, affectedRows: 0, lastInsertId: null, returnsRows: true }, hasMore: false });
    render(<TableTab tab={{ id: "t2", kind: "table", sessionId: "s1", title: "wp_options", database: "db", table: "wp_options", view: "data", filter: "option_name = 'siteurl'" }} />);
    await screen.findByText("No columns");
    expect(mocked.browseTable).toHaveBeenCalledWith("s1", expect.objectContaining({ filter: "option_name = 'siteurl'" }));
  });
});
