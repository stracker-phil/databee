import { act, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { installCommands } from "../../commands";
import { useStore } from "../../state/store";
import { openOrder, stubDocuments, user } from "../../test/columnFixtures";
import { connectedStore } from "../../test/fixtures";
import { activeObject } from "../../test/objectFixtures";

const mocked = vi.mocked(api, true);

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
  });

  describe("drilling into a reference", () => {
    it("opens the referenced row as a new column on Enter and shows the path", async () => {
      await openOrder();
      fireEvent.mouseDown(screen.getByText("12"));
      await userEvent.keyboard("{Enter}");

      expect(await screen.findByRole("region", { name: "wp_users #12" })).toHaveAttribute("data-stage", "column:1");
      const path = screen.getByRole("navigation", { name: "Path" });
      expect(within(path).getByRole("button", { name: "order #4711" })).toBeInTheDocument();
      expect(within(path).getAllByRole("button")).toHaveLength(1);
      expect(within(path).getByText("user")).toBeInTheDocument();
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

    it("pops back to a column when its crumb is clicked", async () => {
      await openOrder();
      drillToUser();
      await userEvent.click(within(screen.getByRole("navigation", { name: "Path" })).getByRole("button", { name: "order #4711" }));
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
      expect(activeObject().columns).toHaveLength(1);
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

  describe("strips", () => {
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");

    beforeEach(() => {
      Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 700 });
    });

    afterEach(() => {
      Object.defineProperty(HTMLElement.prototype, "offsetWidth", original ?? { configurable: true, get: () => 0 });
    });

    it("collapses a column that does not fit to a strip that pops back to it", async () => {
      await openOrder();
      drillToUser();
      const strip = await screen.findByRole("button", { name: "Back to wp_wc_orders #4711" });
      expect(strip).toHaveAttribute("data-stage", "strip:0");
      expect(screen.queryByRole("region", { name: "wp_wc_orders #4711" })).not.toBeInTheDocument();

      await userEvent.click(strip);
      expect(await screen.findByRole("region", { name: "wp_wc_orders #4711" })).toBeInTheDocument();
      expect(screen.queryByRole("region", { name: "wp_users #12" })).not.toBeInTheDocument();
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
});
