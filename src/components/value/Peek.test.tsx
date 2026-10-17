import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { installCommands } from "../../commands";
import { useStore } from "../../state/store";
import { openOrder, stubDocuments } from "../../test/columnFixtures";
import { connectedStore, text, value } from "../../test/fixtures";

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
    mocked.peek.mockResolvedValue({ keyColumn: "ID", values: [["user_login", value({ raw: text("ada") })]] });
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
});
