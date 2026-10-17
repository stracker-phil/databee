import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type DecodedValue } from "../../api";
import { useStore } from "../../state/store";
import { NOW, field, wpStore } from "../../test/wpFixtures";
import { FieldList, FieldValue } from "./FieldValue";

const mocked = vi.mocked(api, true);

describe("FieldValue rendering", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows the stored money value with the formatted amount next to it", () => {
    render(<FieldValue field={field("total_amount", "95.40000000", { hint: { type: "money", text: "$95.40" } })} />);
    expect(screen.getByText("95.40000000")).toBeInTheDocument();
    expect(screen.getByText("$95.40")).toBeInTheDocument();
  });

  it("shows the stored timestamp with its age and the absolute time as title", () => {
    vi.useFakeTimers({ now: NOW });
    const unix = NOW / 1000 - 2 * 86400;
    render(<FieldValue field={field("sent", String(unix), { hint: { type: "time", unix } })} />);
    expect(screen.getByText(String(unix))).toBeInTheDocument();
    expect(screen.getByText("2 d ago")).toHaveAttribute("title", expect.stringMatching(/\S/));
  });

  it("shows a plain value without any note when the field has no hint", () => {
    render(<FieldValue field={field("customer_id", "0")} />);
    expect(screen.getByText("0")).toBeInTheDocument();
    expect(screen.queryByText("guest")).not.toBeInTheDocument();
  });

  it("names every field by its stored key, never a label", () => {
    render(<FieldList fields={[field("_ppcp_paypal_intent", "CAPTURE", { label: "Intent" })]} />);
    expect(screen.getByText("_ppcp_paypal_intent")).toBeInTheDocument();
    expect(screen.queryByText("Intent")).not.toBeInTheDocument();
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
    expect(screen.getByText("0cAFcWeA6zUQyhl2k98qbng9BHiGwe5x…xxxAfg")).toBeInTheDocument();
    expect(screen.getByText("(2.0 KB)")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Expand value" }));
    expect(screen.getByText(token)).toBeInTheDocument();
  });

  it("draws an entity link as a reference that opens the entity as an object tab", async () => {
    mocked.document.mockResolvedValue({ address: { connection: "shop", database: "db", target: { kind: "row", table: "wp_users", key: "12" }, view: null, compare: null }, entity: "user", header: [], groups: [], alternatives: [], actions: [] });
    render(<FieldValue field={field("customer_id", "12", { link: { entity: "user", id: "12" } })} />);
    await userEvent.click(screen.getByRole("button", { name: "Open user 12" }));
    expect(mocked.document).toHaveBeenCalledWith("s1", "user 12", "db");
    await vi.waitFor(() => expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "object", address: { target: { kind: "row", table: "wp_users", key: "12" } } }));
  });

  it("opens the row a field is stored in from its menu", async () => {
    render(<FieldValue field={field("_ppcp_paypal_intent", "CAPTURE")} source={{ table: "wp_wc_orders_meta", where: [{ column: "order_id", value: "4711" }, { column: "meta_key", value: "_ppcp_paypal_intent" }] }} />);
    await userEvent.click(screen.getByRole("button", { name: "Actions for _ppcp_paypal_intent" }));
    await userEvent.click(screen.getByText("Open row in Database view"));
    expect(useStore.getState().mode).toBe("db");
    expect(useStore.getState().tabs.at(-1)).toMatchObject({
      table: "wp_wc_orders_meta",
      where: [{ column: "order_id", value: "4711" }, { column: "meta_key", value: "_ppcp_paypal_intent" }],
    });
  });
});

describe("FieldValue editing", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });

  it("queues an option edit keyed by the field name", async () => {
    render(<FieldValue field={field("blogname", "old")} target={{ kind: "row", table: "wp_options", keyColumn: "option_name", key: null, column: "option_value" }} />);
    await userEvent.click(screen.getByRole("button", { name: "Edit blogname" }));
    const input = screen.getByLabelText("New value");
    await userEvent.clear(input);
    await userEvent.type(input, "new{Enter}");
    expect(mocked.planChange).toHaveBeenCalledWith("s1", expect.objectContaining({ type: "setCell", table: "wp_options", keyColumn: "option_name", key: "blogname", column: "option_value", value: "new" }));
    expect(useStore.getState().pending).toHaveLength(1);
  });

  it("queues column edits on the source row", async () => {
    render(<FieldValue field={field("status", "wc-processing")} target={{ kind: "row", table: "wp_wc_orders", keyColumn: "id", key: "4711" }} />);
    await userEvent.click(screen.getByRole("button", { name: "Edit status" }));
    await userEvent.type(screen.getByLabelText("New value"), "{Control>}a{/Control}x{Enter}");
    expect(mocked.planChange).toHaveBeenCalledWith("s1", expect.objectContaining({ type: "setCell", table: "wp_wc_orders", keyColumn: "id", key: "4711", column: "status" }));
  });

  it("edits a serialized value node by its index path", async () => {
    const decoded: DecodedValue = {
      format: "php",
      root: { kind: "array", children: [{ key: "title", keyKind: "string", node: { kind: "string", text: "PayPal" } }] },
      warning: null,
      error: null,
    };
    mocked.editSerialized.mockResolvedValue('a:1:{s:5:"title";s:3:"New";}');
    render(<FieldValue field={field("settings", 'a:1:{s:5:"title";s:6:"PayPal";}', { decoded })} target={{ kind: "row", table: "wp_options", keyColumn: "option_name", key: null }} />);
    await userEvent.click(screen.getByRole("button", { name: "Edit title" }));
    const input = screen.getByLabelText("New value");
    await userEvent.clear(input);
    await userEvent.type(input, "New{Enter}");
    expect(mocked.editSerialized).toHaveBeenCalledWith('a:1:{s:5:"title";s:6:"PayPal";}', [0], { kind: "string", text: "New" });
    expect(mocked.planChange).toHaveBeenCalledWith("s1", expect.objectContaining({ type: "setCell", key: "settings", value: 'a:1:{s:5:"title";s:3:"New";}' }));
  });

  it("sends the index of the right node when two properties share a display key", async () => {
    const decoded: DecodedValue = {
      format: "php",
      root: {
        kind: "object",
        class: "Foo",
        children: [
          { key: "bar", keyKind: "protected", node: { kind: "string", text: "first" } },
          { key: "bar", keyKind: "public", node: { kind: "string", text: "second" } },
        ],
      },
      warning: null,
      error: null,
    };
    mocked.editSerialized.mockResolvedValue("edited");
    render(<FieldValue field={field("obj", "O:3:…", { decoded })} target={{ kind: "row", table: "wp_options", keyColumn: "option_name", key: null }} />);
    await userEvent.click(screen.getAllByRole("button", { name: "Edit bar" })[1]);
    await userEvent.type(screen.getByLabelText("New value"), "{Control>}a{/Control}x{Enter}");
    expect(mocked.editSerialized).toHaveBeenCalledWith("O:3:…", [1], { kind: "string", text: "x" });
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
