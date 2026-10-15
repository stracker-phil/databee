import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type Favorite } from "../api";
import { useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { Dialogs } from "./Dialogs";
import { FavoritesBar } from "./FavoritesBar";

const mocked = vi.mocked(api, true);

const favorites: Favorite[] = [
  { id: "f1", name: "wp_options", scope: "root@127.0.0.1:52958", target: { kind: "table", database: "db", table: "wp_options" } },
  { id: "f2", name: "PPCP settings", scope: null, target: { kind: "query", sql: "SELECT * FROM wp_options WHERE option_name LIKE 'woocommerce_ppcp%'" } },
];

describe("FavoritesBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    useStore.setState({ favorites });
  });

  it("opens table favorites as table tabs", async () => {
    render(<FavoritesBar />);
    await userEvent.click(screen.getByRole("button", { name: "wp_options" }));
    const tab = useStore.getState().tabs.at(-1)!;
    expect(tab.kind === "table" && tab.table).toBe("wp_options");
  });

  it("opens query favorites as a named query tab", async () => {
    render(<FavoritesBar />);
    await userEvent.click(screen.getByRole("button", { name: "PPCP settings" }));
    const tab = useStore.getState().tabs.at(-1)!;
    expect(tab.title).toBe("PPCP settings");
    expect(tab.kind === "query" && tab.sql).toContain("woocommerce_ppcp");
  });

  it("removes a favorite from its context menu", async () => {
    render(<FavoritesBar />);
    fireEvent.contextMenu(screen.getByRole("button", { name: "wp_options" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Remove" }));
    expect(mocked.removeFavorite).toHaveBeenCalledWith("f1");
    expect(screen.queryByRole("button", { name: "wp_options" })).not.toBeInTheDocument();
  });

  it("pins the current query under a chosen name", async () => {
    connectedStore({ tabs: [{ id: "q1", kind: "query", sessionId: "s1", title: "Query 1", sql: "SELECT 1", running: false, activeResult: 0 }] });
    render(
      <>
        <FavoritesBar />
        <Dialogs />
      </>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Add current tab to favorites" }));
    const name = await screen.findByRole("textbox", { name: "Name this query" });
    await userEvent.clear(name);
    await userEvent.type(name, "One{Enter}");
    expect(mocked.saveFavorite).toHaveBeenCalledWith({
      id: "",
      name: "One",
      scope: "root@127.0.0.1:52958",
      target: { kind: "query", sql: "SELECT 1" },
    });
  });

  it("reorders favorites by drag and drop", () => {
    render(<FavoritesBar />);
    const [a, b] = [screen.getByRole("button", { name: "wp_options" }), screen.getByRole("button", { name: "PPCP settings" })];
    fireEvent.dragStart(b);
    fireEvent.dragOver(a);
    fireEvent.drop(a);
    expect(mocked.reorderFavorites).toHaveBeenCalledWith(["f2", "f1"]);
    expect(useStore.getState().favorites.map((f) => f.id)).toEqual(["f2", "f1"]);
  });
});
