import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type SearchHit } from "../api";
import { installCommands } from "../commands";
import { useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { orderDocument, wpInfo, wpStore } from "../test/wpFixtures";
import { CommandPalette } from "./CommandPalette";

const mocked = vi.mocked(api, true);

const hit = (id: string, title = `Order #${id}`): SearchHit => ({ entity: "order", id, title, subtitle: null, matched: "id" });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const openPalette = () => {
  useStore.getState().setPaletteOpen(true);
  render(<CommandPalette />);
  return screen.getByLabelText("Search everything");
};
const selectedOptions = () => screen.getAllByRole("option", { selected: true });

describe("CommandPalette", () => {
  const scrollIntoView = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    Element.prototype.scrollIntoView = scrollIntoView;
    connectedStore();
    installCommands();
  });

  afterEach(() => {
    delete (Element.prototype as Partial<Element>).scrollIntoView;
  });

  describe("without a query", () => {
    beforeEach(() =>
      useStore.setState({
        recentNav: [
          { kind: "table", database: "db", table: "wp_posts" },
          { kind: "table", database: "db", table: "wp_options" },
        ],
      }),
    );

    it("lists recent items first under a Recent heading", () => {
      openPalette();
      expect(screen.getByText("Recent")).toBeInTheDocument();
      const options = screen.getAllByRole("option");
      expect(options[0]).toHaveTextContent("db.wp_posts");
      expect(options[1]).toHaveTextContent("db.wp_options");
    });

    it("hides the recent items once a query is typed", async () => {
      await userEvent.type(openPalette(), "zoom");
      expect(screen.queryByText("Recent")).not.toBeInTheDocument();
      expect(screen.queryByText("db.wp_posts")).not.toBeInTheDocument();
    });

    it("moves the selection with ArrowDown and ArrowUp and scrolls the active option into view", async () => {
      openPalette();
      expect(selectedOptions()).toHaveLength(1);
      expect(selectedOptions()[0]).toHaveTextContent("db.wp_posts");

      await userEvent.keyboard("{ArrowDown}");
      expect(selectedOptions()).toHaveLength(1);
      expect(selectedOptions()[0]).toHaveTextContent("db.wp_options");
      expect(scrollIntoView.mock.contexts.at(-1)).toBe(selectedOptions()[0]);

      await userEvent.keyboard("{ArrowUp}");
      expect(selectedOptions()[0]).toHaveTextContent("db.wp_posts");
      expect(scrollIntoView.mock.contexts.at(-1)).toBe(selectedOptions()[0]);
    });

    it("opens the selected recent table on Enter", async () => {
      openPalette();
      await userEvent.keyboard("{ArrowDown}{Enter}");
      const tab = useStore.getState().tabs.at(-1);
      expect(tab).toMatchObject({ kind: "table", table: "wp_options" });
      expect(useStore.getState().paletteOpen).toBe(false);
    });
  });

  describe("opening a table", () => {
    it("opens it in the preview tab on Enter", async () => {
      await userEvent.type(openPalette(), "t:wp_posts{Enter}");
      expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "table", table: "wp_posts", preview: true });
      expect(useStore.getState().paletteOpen).toBe(false);
    });

    it("opens it pinned on ⌘↵", async () => {
      await userEvent.type(openPalette(), "t:wp_posts");
      await userEvent.keyboard("{Meta>}{Enter}{/Meta}");
      expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "table", table: "wp_posts", preview: false });
      expect(useStore.getState().paletteOpen).toBe(false);
    });
  });

  it("closes on Escape", async () => {
    openPalette();
    await userEvent.keyboard("{Escape}");
    expect(useStore.getState().paletteOpen).toBe(false);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  describe("entity search", () => {
    beforeEach(() => wpStore());

    it("does not search entities when the database has none", async () => {
      useStore.setState((s) => ({ wp: { ...s.wp, "s1/db": { ...wpInfo, entities: [] } } }));
      await userEvent.type(openPalette(), "#4711");
      expect(screen.queryByText("searching…")).not.toBeInTheDocument();
      expect(mocked.wpSearch).not.toHaveBeenCalled();
    });

    it("searches entities for an ID query when the database has entities", async () => {
      mocked.wpSearch.mockResolvedValue([hit("4711")]);
      await userEvent.type(openPalette(), "#4711");
      expect(await screen.findByText("Order #4711")).toBeInTheDocument();
      expect(mocked.wpSearch).toHaveBeenLastCalledWith("s1", "db", "4711");
    });

    it("stops showing 'searching…' when the query drops below two characters while the search is pending", async () => {
      const pending = deferred<SearchHit[]>();
      mocked.wpSearch.mockReturnValueOnce(pending.promise);
      const input = openPalette();
      await userEvent.type(input, "ab");
      await vi.waitFor(() => expect(mocked.wpSearch).toHaveBeenCalled());
      expect(screen.getByText("searching…")).toBeInTheDocument();

      await userEvent.keyboard("{Backspace}");
      expect(screen.queryByText("searching…")).not.toBeInTheDocument();
      await act(async () => pending.resolve([hit("1")]));
      expect(screen.queryByText("Order #1")).not.toBeInTheDocument();
    });

    it("keeps exactly one option selected when a reply with fewer hits arrives, and Enter runs it", async () => {
      const second = deferred<SearchHit[]>();
      mocked.wpSearch.mockResolvedValueOnce([hit("120"), hit("121"), hit("122")]).mockReturnValueOnce(second.promise);
      const input = openPalette();
      await userEvent.type(input, "#12");
      await screen.findByText("Order #122");

      await userEvent.keyboard("3{ArrowDown}{ArrowDown}");
      expect(selectedOptions()[0]).toHaveTextContent("Order #122");

      await vi.waitFor(() => expect(mocked.wpSearch).toHaveBeenLastCalledWith("s1", "db", "123"));
      await act(async () => second.resolve([hit("123")]));
      expect(screen.getAllByRole("option")).toHaveLength(1);
      expect(selectedOptions()).toHaveLength(1);
      expect(selectedOptions()[0]).toHaveTextContent("Order #123");

      mocked.document.mockResolvedValue(orderDocument("123"));
      await userEvent.keyboard("{Enter}");
      await vi.waitFor(() => expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "object", address: { target: { kind: "row", table: "wp_wc_orders", key: "123" } } }));
      expect(mocked.document).toHaveBeenCalledWith("s1", "order 123", "db");
    });
  });
});
