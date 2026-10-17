import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { address, formatAddress } from "../../lib/address";
import { connectedStore, text, value } from "../../test/fixtures";
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
