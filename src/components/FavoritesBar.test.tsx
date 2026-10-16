import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type Favorite } from "../api";
import { useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { Dialogs } from "./Dialogs";
import { FavoritesBar } from "./FavoritesBar";
import { Toasts } from "./Toasts";

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
    expect(mocked.createFavorite).toHaveBeenCalledWith({
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

  it("moves a favorite to the end when it is dropped on the bar outside any favorite", () => {
    render(<FavoritesBar />);
    fireEvent.dragStart(screen.getByRole("button", { name: "wp_options" }));
    fireEvent.dragOver(screen.getByRole("navigation", { name: "Favorites" }));
    fireEvent.drop(screen.getByRole("navigation", { name: "Favorites" }));
    expect(mocked.reorderFavorites).toHaveBeenCalledWith(["f2", "f1"]);
    expect(useStore.getState().favorites.map((f) => f.id)).toEqual(["f2", "f1"]);
  });

  it("keeps the order when the last favorite is dropped on the bar", () => {
    render(<FavoritesBar />);
    fireEvent.dragStart(screen.getByRole("button", { name: "PPCP settings" }));
    fireEvent.dragOver(screen.getByRole("navigation", { name: "Favorites" }));
    fireEvent.drop(screen.getByRole("navigation", { name: "Favorites" }));
    expect(useStore.getState().favorites.map((f) => f.id)).toEqual(["f1", "f2"]);
  });

  it("ignores a drop on the bar when no favorite is being dragged", () => {
    render(<FavoritesBar />);
    fireEvent.drop(screen.getByRole("navigation", { name: "Favorites" }));
    expect(mocked.reorderFavorites).not.toHaveBeenCalled();
  });

  it("renames a favorite through updateFavorite with the full favorite and shows the new name", async () => {
    mocked.listFavorites.mockResolvedValue([{ ...favorites[0], name: "Options" }, favorites[1]]);
    render(
      <>
        <FavoritesBar />
        <Dialogs />
      </>,
    );
    fireEvent.contextMenu(screen.getByRole("button", { name: "wp_options" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    const name = await screen.findByRole("textbox", { name: "Rename favorite" });
    await userEvent.clear(name);
    await userEvent.type(name, "Options{Enter}");
    expect(mocked.updateFavorite).toHaveBeenCalledWith({ ...favorites[0], name: "Options" });
    expect(await screen.findByRole("button", { name: "Options" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "wp_options" })).not.toBeInTheDocument();
  });

  it("adds the active table to favorites without an id", async () => {
    useStore.setState({ favorites: [] });
    useStore.getState().openTableTab("db", "wp_posts");
    render(<FavoritesBar />);
    await userEvent.click(screen.getByRole("button", { name: "Add current tab to favorites" }));
    expect(mocked.createFavorite).toHaveBeenCalledWith({
      name: "wp_posts",
      scope: "root@127.0.0.1:52958",
      target: { kind: "table", database: "db", table: "wp_posts" },
    });
  });

  it("shows the server's duplicate notice as an info toast, not as an error", async () => {
    useStore.setState({ favorites: [] });
    mocked.createFavorite.mockRejectedValue({ kind: "invalid_input", message: "wp_posts is already in favorites", code: null });
    useStore.getState().openTableTab("db", "wp_posts");
    render(
      <>
        <FavoritesBar />
        <Toasts />
      </>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Add current tab to favorites" }));
    expect(await screen.findByRole("status")).toHaveTextContent("wp_posts is already in favorites");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("does not add the same table twice", async () => {
    useStore.getState().openTableTab("db", "wp_options");
    useStore.getState().addCurrentTabToFavorites();
    expect(mocked.createFavorite).not.toHaveBeenCalled();
    expect(useStore.getState().toasts.at(-1)?.message).toMatch(/already in favorites/);
  });
});
