import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type DecodedValue } from "../../api";
import { useStore } from "../../state/store";
import { field, wpStore } from "../../test/wpFixtures";
import { FieldList, FieldValue } from "./FieldValue";

const mocked = vi.mocked(api, true);

describe("FieldValue rendering", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });

  it("formats money, relative times and roles", () => {
    const unix = Math.round(Date.now() / 1000) - 2 * 86400;
    render(
      <FieldList
        fields={[
          field("total_amount", "95.40000000", { hint: { type: "money", text: "$95.40" } }),
          field("sent", String(unix), { hint: { type: "time", unix, utc: true } }),
          field("wp_capabilities", "a:…", { hint: { type: "roles", roles: ["customer"] } }),
          field("customer_id", "0", { hint: { type: "guest" } }),
          field("billing_country", "AT", { hint: { type: "country", code: "AT" } }),
        ]}
      />,
    );
    expect(screen.getByText("$95.40")).toBeInTheDocument();
    expect(screen.getByText("2 d ago")).toBeInTheDocument();
    expect(screen.getByText("customer")).toBeInTheDocument();
    expect(screen.getByText("Guest")).toBeInTheDocument();
    expect(screen.getByText("🇦🇹 AT")).toBeInTheDocument();
  });

  it("shows labels with the raw key as tooltip", () => {
    render(<FieldList fields={[field("_ppcp_paypal_intent", "CAPTURE", { label: "Intent" })]} />);
    expect(screen.getByText("Intent")).toHaveAttribute("title", "Intent (_ppcp_paypal_intent)");
  });

  it("opens web links through the backend", async () => {
    render(<FieldValue field={field("entry", "https://shop.test/cart", { hint: { type: "url", href: "https://shop.test/cart" } })} />);
    await userEvent.click(screen.getByRole("link"));
    expect(mocked.openUrl).toHaveBeenCalledWith("https://shop.test/cart");
  });

  it("summarizes user agents and keeps the raw string on hover", () => {
    render(<FieldValue field={field("ua", "Mozilla/5.0 …", { hint: { type: "userAgent", text: "Chrome 154 · macOS" } })} />);
    expect(screen.getByText(/Chrome 154 · macOS/)).toHaveAttribute("title", "Mozilla/5.0 …");
  });

  it("masks secrets until revealed", async () => {
    render(<FieldValue field={field("user_pass", "$P$hash", { sensitive: "secret" })} />);
    expect(screen.queryByText("$P$hash")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Reveal secret" }));
    expect(screen.getByText("$P$hash")).toBeInTheDocument();
  });

  it("collapses long values with size, expand and copy", async () => {
    const token = "0cAFcWeA6zUQyhl2k98qbng9BHiGwe5" + "x".repeat(2000) + "Afg";
    render(<FieldValue field={field("token", token)} />);
    expect(screen.getByText(/0cAFcWeA6zUQyhl2k98qbng9BHiGwe5x…xxxAfg|0cAFcW.*…/)).toBeInTheDocument();
    expect(screen.getByText(/2\.0 KB/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Expand value" }));
    expect(screen.getByText(token)).toBeInTheDocument();
  });

  it("can always show the raw value behind a hint", async () => {
    render(<FieldValue field={field("total_amount", "95.40000000", { hint: { type: "money", text: "$95.40" } })} />);
    await userEvent.click(screen.getByTitle("Show raw value"));
    expect(screen.getByText("95.40000000")).toBeInTheDocument();
  });
});

describe("FieldValue editing", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });

  it("queues a meta edit with the new value", async () => {
    render(<FieldValue field={field("_note", "old")} target={{ kind: "meta", entity: "order", id: "4711" }} />);
    await userEvent.click(screen.getByRole("button", { name: "Edit _note" }));
    const input = screen.getByLabelText("New value");
    await userEvent.clear(input);
    await userEvent.type(input, "new{Enter}");
    expect(mocked.planChange).toHaveBeenCalledWith("s1", { type: "setMeta", database: "db", entity: "order", id: "4711", key: "_note", value: "new" });
    expect(useStore.getState().pending).toHaveLength(1);
  });

  it("queues column edits on the source row", async () => {
    render(<FieldValue field={field("status", "wc-processing")} target={{ kind: "row", table: "wp_wc_orders", keyColumn: "id", key: "4711" }} />);
    await userEvent.click(screen.getByRole("button", { name: "Edit status" }));
    await userEvent.type(screen.getByLabelText("New value"), "{Control>}a{/Control}x{Enter}");
    expect(mocked.planChange).toHaveBeenCalledWith("s1", expect.objectContaining({ type: "setCell", table: "wp_wc_orders", keyColumn: "id", key: "4711", column: "status" }));
  });

  it("toggles flags with one click", async () => {
    render(<FieldValue field={field("is_vat_exempt", "no", { hint: { type: "flag", on: false } })} target={{ kind: "meta", entity: "order", id: "4711" }} />);
    await userEvent.click(screen.getByRole("button", { name: /no/ }));
    expect(mocked.planChange).toHaveBeenCalledWith("s1", expect.objectContaining({ type: "setMeta", key: "is_vat_exempt", value: "yes" }));
  });

  it("edits a serialized value node by path", async () => {
    const decoded: DecodedValue = {
      format: "php",
      root: { kind: "array", children: [{ key: "title", keyKind: "string", node: { kind: "string", text: "PayPal" } }] },
      warning: null,
      error: null,
    };
    mocked.editSerialized.mockResolvedValue('a:1:{s:5:"title";s:3:"New";}');
    render(<FieldValue field={field("settings", 'a:1:{s:5:"title";s:6:"PayPal";}', { decoded })} target={{ kind: "meta", entity: "order", id: "4711" }} />);
    await userEvent.click(screen.getByRole("button", { name: "Edit title" }));
    const input = screen.getByLabelText("New value");
    await userEvent.clear(input);
    await userEvent.type(input, "New{Enter}");
    expect(mocked.editSerialized).toHaveBeenCalledWith('a:1:{s:5:"title";s:6:"PayPal";}', ["title"], { kind: "string", text: "New" });
    expect(mocked.planChange).toHaveBeenCalledWith("s1", expect.objectContaining({ type: "setMeta", value: 'a:1:{s:5:"title";s:3:"New";}' }));
  });

  it("is read-only without an edit target", () => {
    render(<FieldValue field={field("status", "x")} />);
    expect(screen.queryByRole("button", { name: "Edit status" })).not.toBeInTheDocument();
  });
});

describe("Value pivots", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });

  it("filters the matching list by a value and finds a value everywhere", async () => {
    render(<FieldList fields={[field("billing_email", "jane@shop.test"), field("_ppcp_paypal_order_id", "66M12696JX411763D")]} />);
    await userEvent.click(screen.getByRole("button", { name: "Actions for billing_email" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Filter orders by this value" }));
    expect(useStore.getState().wpNav.stack.at(-1)).toEqual({ screen: "list", list: "orders", params: { search: "jane@shop.test" } });
    await userEvent.click(screen.getByRole("button", { name: "Actions for _ppcp_paypal_order_id" }));
    expect(screen.queryByRole("menuitem", { name: /Filter/ })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("menuitem", { name: "Find everywhere" }));
    expect(useStore.getState().wpNav.stack.at(-1)).toEqual({ screen: "find", value: "66M12696JX411763D" });
  });
});
