import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { installCommands } from "../../commands";
import { address } from "../../lib/address";
import { useStore } from "../../state/store";
import { openOrder, stubDocuments, stubOffsetWidth, user } from "../../test/columnFixtures";
import { connectedStore } from "../../test/fixtures";
import { activeObject } from "../../test/objectFixtures";

const mocked = vi.mocked(api, true);

const order4712 = address("shop", "db", { kind: "row", table: "wp_wc_orders", key: "4712" });
const trail = () => screen.getByRole("navigation", { name: "Trail" });
const crumb = (name: string) => within(trail()).getByRole("button", { name });

const drillToUser = () => act(() => useStore.getState().drill(activeObject().id, 0, user));

const isTrail = (el: HTMLElement) => el.getAttribute("aria-label") === "Trail";

/** The trail overflows while it holds more than `fits` crumbs. */
function stubOverflow(fits: number) {
  Object.defineProperty(HTMLElement.prototype, "scrollWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return isTrail(this) ? this.querySelectorAll(".trail-crumb").length : 0;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return isTrail(this) ? fits : 0;
    },
  });
  return () => {
    Reflect.deleteProperty(HTMLElement.prototype, "scrollWidth");
    Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
  };
}

describe("Trail", () => {
  let restore: (() => void) | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    stubDocuments(mocked);
  });

  afterEach(() => {
    restore?.();
    restore = undefined;
  });

  it("leads with a crumb that opens the whole table in the same tab", async () => {
    await openOrder();
    const { id } = activeObject();
    await userEvent.click(crumb("Open table wp_wc_orders"));
    expect(useStore.getState().tabs).toHaveLength(1);
    expect(useStore.getState().tabs[0]).toMatchObject({ id, kind: "table", table: "wp_wc_orders", where: [] });
  });

  it("lists every column and marks the active one as the current location", async () => {
    await openOrder();
    drillToUser();
    await within(trail()).findByRole("button", { name: "user #12" });
    expect(crumb("user #12")).toHaveAttribute("aria-current", "location");
    expect(crumb("order #4711")).not.toHaveAttribute("aria-current");
    expect(crumb("Open table wp_wc_orders")).not.toHaveAttribute("aria-current");
  });

  it("offers to show only the crumbs of columns that are not shown as panes", async () => {
    restore = stubOffsetWidth(700);
    await openOrder();
    drillToUser();
    await within(trail()).findByRole("button", { name: "user #12" });
    expect(crumb("order #4711")).toHaveAttribute("title", "Show order #4711");
    expect(crumb("user #12")).not.toHaveAttribute("title");
  });

  it("offers no show hint when every column is a pane", async () => {
    await openOrder();
    drillToUser();
    await within(trail()).findByRole("button", { name: "user #12" });
    expect(crumb("order #4711")).not.toHaveAttribute("title");
    expect(crumb("user #12")).not.toHaveAttribute("title");
  });

  describe("when it overflows", () => {
    async function openCrowded() {
      restore = stubOverflow(3);
      await openOrder();
      drillToUser();
      await screen.findByText("ada");
      act(() => useStore.getState().drill(activeObject().id, 1, order4712));
      await within(trail()).findByRole("button", { name: "order #4712" });
      return within(trail()).getByRole("button", { name: "More columns" });
    }

    it("folds the table and the oldest hidden column into a More columns button, keeping the panes' crumbs", async () => {
      const more = await openCrowded();
      expect(more).toHaveAttribute("title", "wp_wc_orders › order #4711");
      expect(within(trail()).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["More columns", "user #12", "order #4712"]);
    });

    it("lists the folded table and columns in the menu and shows a chosen column", async () => {
      await userEvent.click(await openCrowded());
      const menu = screen.getByRole("menu");
      expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Open table wp_wc_orders", "order #4711"]);

      await userEvent.click(within(menu).getByRole("menuitem", { name: "order #4711" }));
      expect(await screen.findByRole("region", { name: "wp_wc_orders #4711" })).toHaveAttribute("aria-current", "true");
      expect(screen.queryByRole("region", { name: "wp_wc_orders #4712" })).not.toBeInTheDocument();
      expect(activeObject().columns).toHaveLength(3);
    });

    it("opens the whole table from the folded table item", async () => {
      await userEvent.click(await openCrowded());
      const { id } = activeObject();
      await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Open table wp_wc_orders" }));
      expect(useStore.getState().tabs[0]).toMatchObject({ id, kind: "table", table: "wp_wc_orders" });
    });
  });
});
