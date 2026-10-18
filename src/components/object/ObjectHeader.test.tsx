import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { address, formatAddress } from "../../lib/address";
import { useStore } from "../../state/store";
import { connectedStore, text, value } from "../../test/fixtures";
import { showTableIcons } from "../../test/iconFixtures";
import { activeObject, objectDocument, orderAddress } from "../../test/objectFixtures";
import { ObjectHeader } from "./ObjectHeader";

const alternative = (table: string, key: string) => address("shop", "db", { kind: "row", table, key });

const header = (alternatives = [alternative("wp_posts", "4711"), alternative("wp_users", "4711")]) =>
  render(<ObjectHeader document={objectDocument({ alternatives })} owner={orderAddress} view={{ current: "document", onChange: () => {} }} />);

describe("ObjectHeader summary", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
  });

  it("shows the names and values of the header summary", () => {
    const header = { values: [{ name: "status", value: value({ raw: text("wc-processing") }) }] };
    render(<ObjectHeader document={objectDocument({ header })} owner={orderAddress} view={{ current: "document", onChange: () => {} }} />);
    expect(screen.getByText("status")).toBeInTheDocument();
    expect(screen.getByText("wc-processing")).toBeInTheDocument();
  });
});

describe("ObjectHeader in a column stack", () => {
  const inTab = (index: number) => {
    act(() => {
      useStore.getState().openObject(orderAddress);
      if (index > 0) useStore.getState().drill(activeObject().id, 0, alternative("wp_users", "12"));
    });
    render(<ObjectHeader document={objectDocument()} owner={orderAddress} view={{ current: "document", onChange: () => {} }} place={{ tabId: activeObject().id, index }} />);
  };

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
  });

  it("shows the table name as plain text outside a tab", () => {
    header([]);
    expect(screen.getByText("wp_wc_orders")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open table wp_wc_orders" })).not.toBeInTheDocument();
  });

  it("opens the whole table in the same tab from the table name", async () => {
    inTab(0);
    const { id } = activeObject();
    await userEvent.click(screen.getByRole("button", { name: "Open table wp_wc_orders" }));
    expect(useStore.getState().tabs).toHaveLength(1);
    expect(useStore.getState().tabs[0]).toMatchObject({ id, kind: "table", table: "wp_wc_orders", where: [] });
  });

  it("hints that the table name goes to the whole table, with the table's icon", () => {
    showTableIcons({ wp_wc_orders: "receipt" });
    inTab(0);
    const button = screen.getByRole("button", { name: "Open table wp_wc_orders" });
    expect(button.querySelector("[data-open]")).toHaveAttribute("data-open", "go");
    expect(button.querySelector("[data-icon=receipt]")).not.toBeNull();
  });

  it("draws the table's icon but no hint for the plain table name outside a tab", () => {
    showTableIcons({ wp_wc_orders: "receipt" });
    const { container } = header([]);
    expect(screen.getByText("wp_wc_orders").querySelector("[data-icon=receipt]")).not.toBeNull();
    expect(container.querySelector("[data-open]")).toBeNull();
  });

  it("offers no Close column on the first column", () => {
    inTab(0);
    expect(screen.queryByRole("button", { name: "Close column" })).not.toBeInTheDocument();
  });

  it("closes a drilled column, back to the one before it", async () => {
    inTab(1);
    expect(activeObject().columns).toHaveLength(2);
    await userEvent.click(screen.getByRole("button", { name: "Close column" }));
    expect(activeObject().columns).toHaveLength(1);
  });
});

describe("ObjectHeader alternatives", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
  });

  it("lists every other row the shorthand matched as a button titled by table and key", () => {
    header();
    expect(screen.getByText(/Also matched:/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "wp_posts #4711" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "wp_users #4711" })).toBeInTheDocument();
  });

  it("draws the icon of each alternative's table and hints that a click goes there", () => {
    showTableIcons({ wp_users: "user" });
    header();
    const users = screen.getByRole("button", { name: "wp_users #4711" });
    const posts = screen.getByRole("button", { name: "wp_posts #4711" });
    expect(users.querySelector("[data-icon=user]")).not.toBeNull();
    expect(posts.querySelector("[data-icon]")).toBeNull();
    expect(users.querySelector("[data-open]")).toHaveAttribute("data-open", "go");
    expect(posts.querySelector("[data-open]")).toHaveAttribute("data-open", "go");
  });

  it("opens the row of an alternative as an object", async () => {
    header();
    await userEvent.click(screen.getByRole("button", { name: "wp_users #4711" }));
    expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_users/4711");
  });

  it("shows no line when the document has no alternatives", () => {
    header([]);
    expect(screen.queryByText(/Also matched:/)).not.toBeInTheDocument();
  });
});
