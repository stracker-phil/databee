import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { installCommands } from "../../commands";
import { address, formatAddress } from "../../lib/address";
import { useStore } from "../../state/store";
import { connectWithMaxPanes, openOrder, orderDocument, stubDocuments, stubOffsetWidth, user, userDocument } from "../../test/columnFixtures";
import { connectedStore, settings, text, value } from "../../test/fixtures";
import { activeObject, markCompleted } from "../../test/objectFixtures";

const mocked = vi.mocked(api, true);

const order4712 = address("shop", "db", { kind: "row", table: "wp_wc_orders", key: "4712" });
let restoreWidth: (() => void) | undefined;

const drillToUser = () => act(() => useStore.getState().drill(activeObject().id, 0, user));
const orderColumn = () => screen.getByRole("region", { name: "wp_wc_orders #4711" });
const userColumn = () => screen.getByRole("region", { name: "wp_users #12" });
const focused = (column: number) => document.querySelector(`[data-column="${column}"] .row .focus`);

describe("column stack", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    stubDocuments(mocked);
    mocked.getSettings.mockResolvedValue(settings());
  });

  afterEach(() => {
    restoreWidth?.();
    restoreWidth = undefined;
  });

  describe("drilling into a reference", () => {
    it("opens the referenced row as a new column on Enter and shows the path", async () => {
      await openOrder();
      fireEvent.mouseDown(screen.getByText("12"));
      await userEvent.keyboard("{Enter}");

      expect(await screen.findByRole("region", { name: "wp_users #12" })).toHaveAttribute("data-stage", "column:1");
      const trail = screen.getByRole("navigation", { name: "Trail" });
      expect(within(trail).getByRole("button", { name: "Open table wp_wc_orders" })).toBeInTheDocument();
      expect(within(trail).getByRole("button", { name: "order #4711" })).toBeInTheDocument();
      expect(await within(trail).findByRole("button", { name: "user #12" })).toBeInTheDocument();
      expect(within(trail).getAllByRole("button")).toHaveLength(3);
    });
  });

  describe("reference clicks", () => {
    it("drills a new column when the reference is clicked", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("link", { name: "12" }));
      expect(await screen.findByRole("region", { name: "wp_users #12" })).toBeInTheDocument();
      expect(orderColumn()).toBeInTheDocument();
      expect(activeObject().columns).toHaveLength(2);
    });

    it("announces the drill with a passive indicator that is not a button", async () => {
      await openOrder();
      const link = screen.getByRole("link", { name: "12" });
      const hint = link.closest(".val")!.querySelector("[data-open]")!;
      expect(hint).toHaveAttribute("data-open", "drill");
      expect(hint).toHaveAttribute("aria-hidden", "true");
      expect(hint.closest("button")).toBeNull();
    });
  });

  describe("column headers", () => {
    it("shows no Close column button on the first column", async () => {
      await openOrder();
      expect(screen.queryByRole("button", { name: "Close column" })).not.toBeInTheDocument();
    });

    it("closes a drilled column with its Close column button", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      await userEvent.click(within(userColumn()).getByRole("button", { name: "Close column" }));
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
      expect(activeObject().columns).toHaveLength(1);
    });

    it("opens the whole table in the same tab from the table name of a column header", async () => {
      await openOrder();
      const { id } = activeObject();
      await userEvent.click(within(orderColumn()).getByRole("button", { name: "Open table wp_wc_orders" }));
      expect(useStore.getState().tabs).toHaveLength(1);
      expect(useStore.getState().tabs[0]).toMatchObject({ id, kind: "table", table: "wp_wc_orders", where: [] });
    });
  });

  describe("closing columns", () => {
    it("closes the rightmost column on Escape", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      await userEvent.keyboard("{Escape}");
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
      expect(orderColumn()).toBeInTheDocument();
    });

    it("makes a column active without closing any when its crumb is clicked", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      await userEvent.click(within(screen.getByRole("navigation", { name: "Trail" })).getByRole("button", { name: "order #4711" }));
      expect(orderColumn()).toHaveAttribute("aria-current", "true");
      expect(userColumn()).toBeInTheDocument();
      expect(activeObject().columns).toHaveLength(2);
    });

    it("restores the closed stack on back", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      await userEvent.keyboard("{Escape}");
      await userEvent.keyboard("{Meta>}[[{/Meta}");
      expect(await screen.findByRole("region", { name: "wp_users #12" })).toBeInTheDocument();
      expect(orderColumn()).toBeInTheDocument();
    });
  });

  describe("panes shown", () => {
    const names = ["wp_wc_orders #4711", "wp_users #12", "wp_wc_orders #4712"];
    const shownNames = () => names.filter((name) => screen.queryByRole("region", { name }));

    async function openThree(maxPanes: number, width: number) {
      restoreWidth = stubOffsetWidth(width);
      connectWithMaxPanes(mocked, maxPanes);
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      act(() => useStore.getState().drill(activeObject().id, 1, order4712));
      await screen.findByRole("region", { name: "wp_wc_orders #4712" });
    }

    it.each([
      [1, ["wp_wc_orders #4712"]],
      [2, ["wp_users #12", "wp_wc_orders #4712"]],
      [3, names],
    ])("shows the newest %i columns when the setting allows that many and the window fits them", async (maxPanes, expected) => {
      await openThree(maxPanes, 2000);
      expect(shownNames()).toEqual(expected);
    });

    it("shows fewer panes than the setting allows when the window is too narrow for them", async () => {
      await openThree(3, 1200);
      expect(shownNames()).toEqual(["wp_users #12", "wp_wc_orders #4712"]);
    });

    it("shows one pane when the window fits only one", async () => {
      await openThree(3, 700);
      expect(shownNames()).toEqual(["wp_wc_orders #4712"]);
    });
  });

  describe("hidden columns", () => {
    beforeEach(() => {
      restoreWidth = stubOffsetWidth(700);
    });

    it("shows a hidden column from its crumb without closing the active one", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      expect(screen.queryByRole("region", { name: "wp_wc_orders #4711" })).not.toBeInTheDocument();

      const crumb = within(screen.getByRole("navigation", { name: "Trail" })).getByRole("button", { name: "order #4711" });
      expect(crumb).toHaveAttribute("title", "Show order #4711");
      expect(crumb).toHaveAttribute("data-stage", "trail:0");
      await userEvent.click(crumb);

      expect(await screen.findByRole("region", { name: "wp_wc_orders #4711" })).toBeInTheDocument();
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
      expect(activeObject().columns).toHaveLength(2);
    });

    it("moves to the hidden column on the left with h and shows it", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      await userEvent.keyboard("h");
      expect(await screen.findByRole("region", { name: "wp_wc_orders #4711" })).toHaveAttribute("aria-current", "true");
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
    });
  });

  describe("pane dividers", () => {
    const divider = () => screen.getByRole("separator", { name: "Resize wp_wc_orders #4711 and wp_users #12" });

    it("separates two panes with one resizable divider and none for a single pane", async () => {
      await openOrder();
      expect(screen.queryByRole("separator")).not.toBeInTheDocument();
      drillToUser();
      await screen.findByText("ada");
      expect(screen.getAllByRole("separator")).toHaveLength(1);
      expect(divider()).toBeInTheDocument();
    });

    it("has one divider fewer than panes", async () => {
      restoreWidth = stubOffsetWidth(2000);
      connectWithMaxPanes(mocked, 3);
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      act(() => useStore.getState().drill(activeObject().id, 1, order4712));
      await screen.findByRole("region", { name: "wp_wc_orders #4712" });
      expect(screen.getAllByRole("separator").map((s) => s.getAttribute("aria-label"))).toEqual([
        "Resize wp_wc_orders #4711 and wp_users #12",
        "Resize wp_users #12 and wp_wc_orders #4712",
      ]);
    });

    it("moves the first pane's share with the arrow keys", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      expect(activeObject().paneWidths).toBeUndefined();
      divider().focus();
      await userEvent.keyboard("{ArrowRight}");

      const [first, second] = activeObject().paneWidths!;
      expect(first).toBeGreaterThan(0.5);
      expect(first + second).toBeCloseTo(1);
    });

    it("equalizes the panes on a double-click", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      divider().focus();
      await userEvent.keyboard("{ArrowRight}");
      expect(activeObject().paneWidths).toBeDefined();

      fireEvent.doubleClick(divider());
      expect(activeObject().paneWidths).toBeUndefined();
    });
  });

  describe("maximize", () => {
    const maximizeIn = (column: HTMLElement) => within(column).getByRole("button", { name: "Maximize pane" });
    const maximizeKeys = "{Meta>}{Shift>}{Enter}{/Shift}{/Meta}";

    async function drilled() {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
    }

    it("shows only the pane whose button is clicked, and restores the panes from its Restore button", async () => {
      await drilled();
      await userEvent.click(maximizeIn(userColumn()));

      expect(screen.queryByRole("region", { name: "wp_wc_orders #4711" })).not.toBeInTheDocument();
      expect(screen.queryByRole("separator")).not.toBeInTheDocument();
      const restore = within(userColumn()).getByRole("button", { name: "Restore panes" });
      expect(restore).toHaveAttribute("aria-pressed", "true");

      await userEvent.click(restore);
      expect(await screen.findByRole("region", { name: "wp_wc_orders #4711" })).toBeInTheDocument();
      expect(userColumn()).toBeInTheDocument();
    });

    it("maximizes a pane that was not active", async () => {
      await drilled();
      await userEvent.click(maximizeIn(orderColumn()));
      expect(orderColumn()).toHaveAttribute("aria-current", "true");
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
    });

    it("toggles with the shortcut", async () => {
      await drilled();
      await userEvent.keyboard(maximizeKeys);
      expect(screen.queryByRole("region", { name: "wp_wc_orders #4711" })).not.toBeInTheDocument();

      await userEvent.keyboard(maximizeKeys);
      expect(await screen.findByRole("region", { name: "wp_wc_orders #4711" })).toBeInTheDocument();
      expect(userColumn()).toBeInTheDocument();
    });

    it("restores the panes on Escape before it closes a column", async () => {
      await drilled();
      await userEvent.keyboard(maximizeKeys);
      await userEvent.keyboard("{Escape}");
      expect(await screen.findByRole("region", { name: "wp_wc_orders #4711" })).toBeInTheDocument();
      expect(userColumn()).toBeInTheDocument();

      await userEvent.keyboard("{Escape}");
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
    });

    it("offers no maximize button with a single column", async () => {
      await openOrder();
      expect(screen.queryByRole("button", { name: "Maximize pane" })).not.toBeInTheDocument();
    });

    it("offers no maximize button, and the shortcut does nothing, when only one pane is allowed", async () => {
      connectWithMaxPanes(mocked, 1);
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      expect(screen.queryByRole("button", { name: "Maximize pane" })).not.toBeInTheDocument();

      await userEvent.keyboard(maximizeKeys);
      expect(activeObject().maximized).toBeUndefined();
    });
  });

  describe("keys", () => {
    it("moves the focused value with j and k and shows its provenance in the status bar", async () => {
      await openOrder();
      await userEvent.keyboard("j");
      expect(focused(0)).toHaveTextContent("4711");
      await userEvent.keyboard("j");
      expect(focused(0)).toHaveTextContent("wc-processing");
      expect(screen.getByText("wp_wc_orders.status · id 4711")).toBeInTheDocument();
      await userEvent.keyboard("k");
      expect(focused(0)).toHaveTextContent("4711");
    });

    it("moves the active column with h and l", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      expect(userColumn()).toHaveAttribute("aria-current", "true");
      await userEvent.keyboard("h");
      expect(orderColumn()).toHaveAttribute("aria-current", "true");
      expect(userColumn()).not.toHaveAttribute("aria-current");
      await userEvent.keyboard("l");
      expect(userColumn()).toHaveAttribute("aria-current", "true");
    });

    it("leaves the arrow keys to a focused grid", async () => {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      act(() => useStore.getState().setObjectView(activeObject().id, "rows", undefined, 0));
      await userEvent.click(await screen.findByText("wc-processing"));
      await userEvent.keyboard("{ArrowRight}");
      expect(orderColumn()).toHaveAttribute("aria-current", "true");
    });
  });

  describe("view toggle", () => {
    const toggleIn = (column: HTMLElement) => within(column).queryByRole("group", { name: "View" });

    async function drilled() {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
    }

    it("is offered by the active pane only", async () => {
      await drilled();
      expect(toggleIn(userColumn())).toBeInTheDocument();
      expect(toggleIn(orderColumn())).not.toBeInTheDocument();
    });

    it.each([
      ["document", "Document"],
      ["rows", "Rows"],
    ] as const)("names the %s view as text in a pane that is not active", async (view, label) => {
      await drilled();
      const { id } = activeObject();
      act(() => {
        useStore.getState().setObjectView(id, view, undefined, 0);
        useStore.getState().setActiveColumn(id, 1);
      });
      expect(within(orderColumn()).getByText(label)).toBeInTheDocument();
      expect(within(orderColumn()).queryByRole("group", { name: "View" })).not.toBeInTheDocument();
    });

    it("moves to a pane when the mouse goes down in it", async () => {
      await drilled();
      fireEvent.mouseDown(within(orderColumn()).getByText("wc-processing"));
      expect(toggleIn(orderColumn())).toBeInTheDocument();
      expect(toggleIn(userColumn())).not.toBeInTheDocument();
    });

    it("is offered by a single pane", async () => {
      await openOrder();
      expect(toggleIn(orderColumn())).toBeInTheDocument();
    });

    it("is offered by a maximized pane", async () => {
      await drilled();
      act(() => useStore.getState().toggleMaximized(activeObject().id));
      expect(toggleIn(userColumn())).toBeInTheDocument();
    });
  });

  describe("pane focus keys (⌘⌥←, ⌘⌥→)", () => {
    const left = "{Meta>}{Alt>}{ArrowLeft}{/Alt}{/Meta}";
    const right = "{Meta>}{Alt>}{ArrowRight}{/Alt}{/Meta}";
    const sidebarButton = () => screen.getByRole("button", { name: "Refresh" });

    async function drilled() {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
    }

    it("make the pane on the left or right active and focus it, with the focus in the sidebar", async () => {
      await drilled();
      sidebarButton().focus();
      await userEvent.keyboard(left);
      expect(orderColumn()).toHaveAttribute("aria-current", "true");
      expect(orderColumn()).toHaveFocus();

      sidebarButton().focus();
      await userEvent.keyboard(right);
      expect(userColumn()).toHaveAttribute("aria-current", "true");
      expect(userColumn()).toHaveFocus();
    });

    it("do nothing while text is edited", async () => {
      await drilled();
      await userEvent.click(screen.getByLabelText("Filter tables"));
      await userEvent.keyboard(left);
      expect(userColumn()).toHaveAttribute("aria-current", "true");
    });
  });

  describe("closing the active pane (⌘⇧W)", () => {
    const closeKeys = "{Meta>}{Shift>}w{/Shift}{/Meta}";

    async function three() {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      act(() => useStore.getState().drill(activeObject().id, 1, order4712));
      await screen.findByRole("region", { name: "wp_wc_orders #4712" });
    }

    it("closes the active pane and the panes opened from it, and makes the pane before it active", async () => {
      await three();
      act(() => useStore.getState().setActiveColumn(activeObject().id, 1));
      await userEvent.keyboard(closeKeys);

      expect(activeObject().columns).toHaveLength(1);
      expect(activeObject().active).toBe(0);
      expect(orderColumn()).toHaveAttribute("aria-current", "true");
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
    });

    it("does nothing on the first pane", async () => {
      await three();
      act(() => useStore.getState().setActiveColumn(activeObject().id, 0));
      await userEvent.keyboard(closeKeys);
      expect(activeObject().columns).toHaveLength(3);
    });

    it("is undone by back", async () => {
      await three();
      act(() => useStore.getState().setActiveColumn(activeObject().id, 1));
      await userEvent.keyboard(closeKeys);
      await userEvent.keyboard("{Meta>}[[{/Meta}");
      await waitFor(() => expect(activeObject().columns).toHaveLength(3));
    });
  });

  describe("opening the active pane's row in a new tab (⌘⌥↵)", () => {
    const openKeys = "{Meta>}{Alt>}{Enter}{/Alt}{/Meta}";
    const USER_URL = "databee://shop/db/wp_users/12";
    const activeTabId = () => useStore.getState().activeTabId!;

    async function drilled() {
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
    }

    it("opens the row of a drilled pane in a new tab that is not a preview, and leaves the original tab as it was", async () => {
      await drilled();
      const original = activeTabId();
      await userEvent.keyboard(openKeys);

      expect(useStore.getState().tabs).toHaveLength(2);
      expect(formatAddress(activeObject().address)).toBe(USER_URL);
      expect(activeObject().id).not.toBe(original);
      expect(activeObject().preview).toBeFalsy();
      expect(useStore.getState().tabs.find((t) => t.id === original)).toMatchObject({ active: 1, columns: [{ kind: "object" }, { kind: "object" }] });
    });

    it("opens the row of the pane that a value column came from", async () => {
      await drilled();
      act(() => useStore.getState().openValueColumn(activeTabId(), 1, { kind: "value", value: value({ raw: text("x") }), text: "x", database: "db" }));
      await screen.findByRole("region", { name: "x" });
      await userEvent.keyboard(openKeys);

      expect(useStore.getState().tabs).toHaveLength(2);
      expect(formatAddress(activeObject().address)).toBe(USER_URL);
    });

    it("does nothing on the first pane", async () => {
      await drilled();
      act(() => useStore.getState().setActiveColumn(activeTabId(), 0));
      await userEvent.keyboard(openKeys);
      expect(useStore.getState().tabs).toHaveLength(1);
    });
  });

  describe("pane header menu", () => {
    const headerOf = (column: HTMLElement) => within(column).getByRole("region", { name: "Object header" });
    const labels = () => screen.getAllByRole("menuitem").map((item) => item.textContent);

    async function drilled() {
      mocked.document.mockImplementation(async (_s, addressText) => ({ ...(addressText.includes("/wp_users/") ? userDocument : orderDocument), actions: [markCompleted] }));
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
    }

    it("lists Open in new tab and Close pane after the actions on a drilled pane", async () => {
      await drilled();
      fireEvent.contextMenu(headerOf(userColumn()));
      expect(labels()).toEqual([expect.stringMatching(/^Run mark-completed/), "Open in new tab", "Close pane"]);
    });

    it("opens the pane's row in a new tab from Open in new tab", async () => {
      await drilled();
      fireEvent.contextMenu(headerOf(userColumn()));
      await userEvent.click(screen.getByRole("menuitem", { name: "Open in new tab" }));
      expect(useStore.getState().tabs).toHaveLength(2);
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_users/12");
    });

    it("closes the pane from Close pane", async () => {
      await drilled();
      fireEvent.contextMenu(headerOf(userColumn()));
      await userEvent.click(screen.getByRole("menuitem", { name: "Close pane" }));
      expect(activeObject().columns).toHaveLength(1);
    });

    it("lists only the actions on the root pane", async () => {
      await drilled();
      fireEvent.mouseDown(orderColumn());
      fireEvent.contextMenu(headerOf(orderColumn()));
      expect(labels()).toEqual([expect.stringMatching(/^Run mark-completed/)]);
    });
  });
});
