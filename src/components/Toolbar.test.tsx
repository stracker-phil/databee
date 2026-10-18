import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { installCommands } from "../commands";
import { address } from "../lib/address";
import { useStore } from "../state/store";
import { connectedStore, tableTab } from "../test/fixtures";
import { CommandPalette } from "./CommandPalette";
import { Toolbar } from "./Toolbar";

describe("Toolbar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    installCommands();
  });

  it("opens the command palette from the Go to field", async () => {
    render(
      <>
        <Toolbar />
        <CommandPalette />
      </>,
    );
    expect(screen.queryByLabelText("Search everything")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Go to" }));
    expect(await screen.findByLabelText("Search everything")).toBeInTheDocument();
  });

  it("has no mode toggle, only connection, search and the action buttons", () => {
    render(<Toolbar />);
    expect(screen.getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["Disconnect", "Go to", "New connection", "New query", "Settings"]);
  });

  it("shows the palette shortcut as a key cap", () => {
    render(<Toolbar />);
    expect(screen.getByRole("button", { name: "Go to" })).toHaveTextContent("⌘K");
  });

  describe("Back and Forward", () => {
    const post7 = address("shop", "db", { kind: "row", table: "wp_posts", key: "7" });

    it("are absent for a query tab", () => {
      render(<Toolbar />);
      expect(screen.queryByRole("button", { name: "Back" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Forward" })).not.toBeInTheDocument();
    });

    it("are disabled for a table tab without history", () => {
      connectedStore({ tabs: [tableTab()] });
      render(<Toolbar />);
      expect(screen.getByRole("button", { name: "Back" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "Forward" })).toBeDisabled();
    });

    it("go back to the table and forward to the row of an object tab", async () => {
      connectedStore({ tabs: [tableTab()] });
      render(<Toolbar />);
      act(() => useStore.getState().navigateTab("t1", post7));
      expect(screen.getByRole("button", { name: "Back" })).toBeEnabled();
      expect(screen.getByRole("button", { name: "Forward" })).toBeDisabled();

      await userEvent.click(screen.getByRole("button", { name: "Back" }));
      await waitFor(() => expect(useStore.getState().tabs[0].kind).toBe("table"));
      expect(screen.getByRole("button", { name: "Forward" })).toBeEnabled();

      await userEvent.click(screen.getByRole("button", { name: "Forward" }));
      await waitFor(() => expect(useStore.getState().tabs[0].kind).toBe("object"));
    });
  });
});
