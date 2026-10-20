import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { installCommands } from "../commands";
import { address } from "../lib/address";
import { useStore } from "../state/store";
import { connectedStore, tableTab } from "../test/fixtures";
import { CommandPalette } from "./CommandPalette";
import { Toolbar } from "./Toolbar";

const mocked = vi.mocked(api, true);

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

  describe("database accent", () => {
    const connection = () => screen.getByTitle(/root@127\.0\.0\.1:52958\/db/);

    it("shows a dot before the database and names the accent in the title when the database has its own", () => {
      useStore.setState({ accents: { "shop/db": "purple" } });
      render(<Toolbar />);
      expect(connection()).toHaveAttribute("title", expect.stringContaining("accent purple"));
      expect(connection().querySelector("[data-accent]")).toHaveAttribute("data-accent", "purple");
    });

    it("shows no dot and no accent in the title without one", () => {
      useStore.setState({ accents: { "shop/other": "purple" } });
      render(<Toolbar />);
      expect(connection()).not.toHaveAttribute("title", expect.stringContaining("accent"));
      expect(connection().querySelector("[data-accent]")).not.toBeInTheDocument();
    });
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

  describe("connection health", () => {
    const since = new Date(2025, 0, 2, 14, 2, 10).getTime();
    const down = { state: "down", since, error: "Connection refused (os error 61)", retryAt: since + 30000 } as const;

    it("shows no warning and no Retry while the connection is fine", () => {
      render(<Toolbar />);
      expect(screen.queryByRole("status")).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    });

    it("warns with the cause and the time in its tooltip when the connection is lost", () => {
      useStore.setState({ health: { s1: down } });
      render(<Toolbar />);
      expect(screen.getByLabelText("Connection lost")).toBeInTheDocument();
      expect(screen.getByRole("status")).toHaveAttribute("title", expect.stringContaining("Connection lost since"));
      expect(screen.getByRole("status")).toHaveAttribute("title", expect.stringContaining("Connection refused (os error 61)"));
    });

    it("asks Rust to reconnect the session when Retry is clicked", async () => {
      useStore.setState({ health: { s1: down } });
      render(<Toolbar />);
      await userEvent.click(screen.getByRole("button", { name: "Retry" }));
      expect(mocked.reconnectSession).toHaveBeenCalledWith("s1");
    });

    it("disables the button and says Reconnecting… while a retry is running", () => {
      useStore.setState({ health: { s1: { state: "reconnecting", since, error: "Connection refused (os error 61)" } } });
      render(<Toolbar />);
      expect(screen.getByRole("button", { name: "Reconnecting…" })).toBeDisabled();
      expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    });

    it("shows the warning of the active session only", () => {
      useStore.setState({ health: { s2: down } });
      render(<Toolbar />);
      expect(screen.queryByRole("status")).not.toBeInTheDocument();
    });

    it("disappears when the connection is back", async () => {
      useStore.setState({ health: { s1: down } });
      render(<Toolbar />);
      await act(async () => useStore.getState().setHealth({ sessionId: "s1", health: { state: "ok" } }));
      expect(screen.queryByRole("status")).not.toBeInTheDocument();
    });
  });
});
