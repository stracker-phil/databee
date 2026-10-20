import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type Peek, type PeekMatch } from "../../api";
import { installCommands } from "../../commands";
import { address, formatAddress } from "../../lib/address";
import { useStore } from "../../state/store";
import { emailRef, emailSet, openOrder, stubDocuments, user } from "../../test/columnFixtures";
import { connectedStore, num, text, value } from "../../test/fixtures";
import { activeObject } from "../../test/objectFixtures";
import { PeekOverlay } from "./Peek";
import { Value } from "./Value";

const mocked = vi.mocked(api, true);

const peekCard = () => screen.findByRole("complementary", { name: "Peek wp_users 12" });

async function holdAltOverReference() {
  await openOrder();
  fireEvent.mouseOver(screen.getByText("12"));
  fireEvent.keyDown(window, { key: "Alt", altKey: true });
}

describe("Peek", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    stubDocuments(mocked);
    mocked.peek.mockResolvedValue({ address: user, keyColumn: "ID", summary: { values: [{ name: "user_login", value: value({ raw: text("ada") }) }] }, matches: null });
  });

  it("shows the hovered reference's target values and the rule it followed while Alt is held", async () => {
    await holdAltOverReference();
    const card = await peekCard();
    expect(await screen.findByText("ada")).toBeInTheDocument();
    expect(card).toHaveTextContent("user_login");
    expect(screen.getByText("via customer_id → wp_users.ID · preset woocommerce")).toBeInTheDocument();
    expect(mocked.peek).toHaveBeenCalledWith("s1", "databee://shop/db/wp_users/12");
  });

  it("hides the peek when Alt is released", async () => {
    await holdAltOverReference();
    await peekCard();
    fireEvent.keyUp(window, { key: "Alt" });
    expect(screen.queryByRole("complementary", { name: "Peek wp_users 12" })).not.toBeInTheDocument();
  });

  it("shows nothing when Alt is held over a plain value", async () => {
    await openOrder();
    fireEvent.mouseOver(screen.getByText("wc-processing"));
    fireEvent.keyDown(window, { key: "Alt", altKey: true });
    expect(screen.queryByRole("complementary", { name: /^Peek/ })).not.toBeInTheDocument();
  });

  it("shows the error when the target cannot be read", async () => {
    mocked.peek.mockRejectedValue({ kind: "invalid_input", message: "No such row", code: null });
    useStore.setState({ dataVersion: 1000 });
    await holdAltOverReference();
    expect(await screen.findByText("No such row")).toBeInTheDocument();
  });

  it("drills into the target on Enter while the peek shows", async () => {
    await holdAltOverReference();
    await peekCard();
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByRole("region", { name: "wp_users #12" })).toHaveAttribute("data-stage", "column:1");
    expect(screen.queryByRole("complementary", { name: "Peek wp_users 12" })).not.toBeInTheDocument();
  });

  describe("a peeked row", () => {
    let version = 9000;
    const peekRow = async (values: Peek["summary"]["values"]) => {
      mocked.peek.mockResolvedValue({ address: user, keyColumn: "ID", summary: { values }, matches: null });
      useStore.setState({ dataVersion: ++version });
      await holdAltOverReference();
      return peekCard();
    };

    it("lists each value as a table row with its name as the row header", async () => {
      const card = await peekRow([{ name: "user_login", value: value({ raw: text("ada") }) }]);
      const table = within(card).getByRole("table");
      const rows = within(table).getAllByRole("row");
      expect(rows).toHaveLength(1);
      expect(within(rows[0]).getByRole("rowheader")).toHaveTextContent("user_login");
      expect(within(rows[0]).getByRole("cell")).toHaveTextContent("ada");
    });

    it("keeps the order and pairs each name with its value", async () => {
      const card = await peekRow([
        { name: "user_login", value: value({ raw: text("ada") }) },
        { name: "user_email", value: value({ raw: text("ada@example.com") }) },
        { name: "user_status", value: value({ raw: num("0"), kind: { kind: "number" } }) },
      ]);
      const rows = within(within(card).getByRole("table")).getAllByRole("row");
      expect(rows.map((r) => [within(r).getByRole("rowheader").textContent, within(r).getByRole("cell").textContent])).toEqual([
        ["user_login", "ada"],
        ["user_email", "ada@example.com"],
        ["user_status", "0"],
      ]);
    });

    it("shows a value's secondary text in its cell", async () => {
      const card = await peekRow([{ name: "ID", value: value({ raw: num("12"), secondary: { text: "Jane Doe", source: null } }) }]);
      expect(within(within(card).getByRole("cell")).getByText("Jane Doe")).toBeInTheDocument();
    });

    it("says there are no values instead of rendering a table", async () => {
      const card = await peekRow([]);
      expect(await within(card).findByText("no values")).toBeInTheDocument();
      expect(within(card).queryByRole("table")).not.toBeInTheDocument();
    });

    it("keeps the stage anchor and the footer", async () => {
      const card = await peekRow([{ name: "user_login", value: value({ raw: text("ada") }) }]);
      expect(card).toHaveAttribute("data-stage", "peek");
      expect(card).toHaveTextContent("via customer_id → wp_users.ID · preset woocommerce");
    });
  });

  describe("a reference to a set of rows", () => {
    let version = 5000;
    const orderMatch = (key: string, email = "ada@example.com"): PeekMatch => ({
      address: address("shop", "db", { kind: "row", table: "wp_wc_orders", key }),
      key: [{ column: "id", value: key }],
      summary: { values: [{ name: "billing_email", value: value({ raw: text(email) }) }] },
    });
    const setPeek = (rows: PeekMatch[], total = rows.length): Peek => ({ address: emailSet, keyColumn: null, summary: { values: [] }, matches: { total, rows } });

    const peekSet = async (peek: Peek) => {
      mocked.peek.mockResolvedValue(peek);
      render(
        <>
          <PeekOverlay />
          <Value value={emailRef} />
        </>,
      );
      fireEvent.mouseOver(screen.getByText("ada@example.com"));
      fireEvent.keyDown(window, { key: "Alt", altKey: true });
      return screen.findByRole("complementary", { name: `Peek ${formatAddress(emailSet)}` });
    };

    beforeEach(() => useStore.setState({ dataVersion: ++version }));

    it("titles the card with the table and the filter and lists the matching rows", async () => {
      const card = await peekSet(setPeek([orderMatch("4712"), orderMatch("4713"), orderMatch("4714")]));
      expect(card).toHaveTextContent("wp_wc_orders · billing_email = ada@example.com");
      expect(await screen.findByText("3 rows match")).toBeInTheDocument();
      for (const key of ["#4712", "#4713", "#4714"]) expect(card).toHaveTextContent(key);
      expect(card).toHaveTextContent("billing_email");
    });

    it("lists the matches as buttons without a summary table", async () => {
      const card = await peekSet(setPeek([orderMatch("4712"), orderMatch("4713")]));
      expect(await screen.findByRole("button", { name: /#4712/ })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /#4713/ })).toBeInTheDocument();
      expect(within(card).queryByRole("table")).not.toBeInTheDocument();
      expect(card).toHaveAttribute("data-stage", "peek");
    });

    it("leaves out the row the reference is stored in", async () => {
      const card = await peekSet(setPeek([orderMatch("4711"), orderMatch("4712"), orderMatch("4713")]));
      expect(await screen.findByText("2 other rows match")).toBeInTheDocument();
      expect(card).not.toHaveTextContent("#4711");
      expect(card).toHaveTextContent("#4712");
    });

    it("says no row matches for an empty set", async () => {
      await peekSet(setPeek([]));
      expect(await screen.findByText("no row matches")).toBeInTheDocument();
    });

    it("says no other row matches when the only match is the row itself", async () => {
      await peekSet(setPeek([orderMatch("4711")]));
      expect(await screen.findByText("no other row matches")).toBeInTheDocument();
    });

    it("opens a matching row when it is clicked", async () => {
      await peekSet(setPeek([orderMatch("4712")]));
      await userEvent.click(await screen.findByRole("button", { name: /#4712/ }));
      await vi.waitFor(() => expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_wc_orders/4712"));
    });
  });
});
