import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { installCommands } from "../commands";
import { selectSidebarHidden, useStore } from "../state/store";
import { columnMeta, connectedStore, num, pendingChange, queryTab, tableTab, value } from "../test/fixtures";
import { StatusBar } from "./StatusBar";
import { Toasts } from "./Toasts";
import { useValueHover } from "./value/useValueHover";

const meta = { table: "wp_posts", column: "post_author", row: [{ column: "ID", value: "5" }] };

function Shell() {
  const ref = useRef<HTMLDivElement>(null);
  useValueHover(ref);
  return (
    <div className="app" role="application" ref={ref}>
      <span data-src="wp_postmeta.meta_value · meta_id 9" data-table="wp_postmeta">
        hovered value
      </span>
      <span>no provenance here</span>
      <StatusBar />
    </div>
  );
}

describe("StatusBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    installCommands();
  });

  it("has no theme or inspector chips", () => {
    render(<StatusBar />);
    expect(screen.queryByRole("button", { name: /Theme/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Inspector/ })).not.toBeInTheDocument();
  });

  it("hides the sidebar from its toggle and reports the button as not pressed", async () => {
    render(<StatusBar />);
    const toggle = screen.getByRole("button", { name: "Toggle sidebar" });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(selectSidebarHidden(useStore.getState())).toBe(true);
  });

  describe("address", () => {
    beforeEach(() => useStore.setState({ tabs: [tableTab()], activeTabId: "t1" }));

    it("shows a table tab's address", () => {
      render(<StatusBar />);
      expect(screen.getByRole("button", { name: "databee://shop/db/wp_posts" })).toBeInTheDocument();
    });

    it("shows a query tab's database home", () => {
      useStore.setState({ tabs: [queryTab()], activeTabId: "q1" });
      render(<StatusBar />);
      expect(screen.getByRole("button", { name: "databee://shop/db/" })).toBeInTheDocument();
    });

    it("copies the address on click and confirms it", async () => {
      render(
        <>
          <StatusBar />
          <Toasts />
        </>,
      );
      await userEvent.click(screen.getByRole("button", { name: "databee://shop/db/wp_posts" }));
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith("databee://shop/db/wp_posts");
      expect(await screen.findByText("Copied address")).toBeInTheDocument();
    });

    it("adds a filtered hint for a typed SQL filter and keeps the address", () => {
      useStore.setState({ tabs: [tableTab({ filter: "post_type = 'page'" })] });
      render(<StatusBar />);
      expect(screen.getByText("· filtered")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "databee://shop/db/wp_posts" })).toBeInTheDocument();
    });

    it("shows no filtered hint without a typed filter", () => {
      render(<StatusBar />);
      expect(screen.queryByText("· filtered")).not.toBeInTheDocument();
    });
  });

  describe("value provenance", () => {
    const selectAuthor = () =>
      act(() =>
        useStore.getState().selectCell({
          row: 0,
          col: 0,
          column: columnMeta("post_author", "bigint"),
          cell: num("5"),
          source: { sessionId: "s1", resultId: 1 },
          value: value({ raw: num("5"), source: meta }),
        }),
      );

    beforeEach(() => useStore.setState({ tabs: [tableTab()], activeTabId: "t1" }));

    it("shows where the hovered value is stored instead of the address", async () => {
      render(<Shell />);
      await userEvent.hover(screen.getByText("hovered value"));
      expect(await screen.findByText("wp_postmeta.meta_value · meta_id 9")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "databee://shop/db/wp_posts" })).not.toBeInTheDocument();
    });

    it("shows the address again once the pointer leaves", async () => {
      render(<Shell />);
      await userEvent.hover(screen.getByText("hovered value"));
      await screen.findByText("wp_postmeta.meta_value · meta_id 9");
      fireEvent.mouseLeave(screen.getByRole("application"));
      expect(await screen.findByRole("button", { name: "databee://shop/db/wp_posts" })).toBeInTheDocument();
      expect(screen.queryByText("wp_postmeta.meta_value · meta_id 9")).not.toBeInTheDocument();
    });

    it("shows the address again when the pointer moves to something without provenance", async () => {
      render(<Shell />);
      await userEvent.hover(screen.getByText("hovered value"));
      await screen.findByText("wp_postmeta.meta_value · meta_id 9");
      await userEvent.hover(screen.getByText("no provenance here"));
      expect(await screen.findByRole("button", { name: "databee://shop/db/wp_posts" })).toBeInTheDocument();
    });

    it("shows the provenance of the selected cell's value", () => {
      render(<StatusBar />);
      selectAuthor();
      expect(screen.getByText("wp_posts.post_author · ID 5")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "databee://shop/db/wp_posts" })).not.toBeInTheDocument();
    });

    it("prefers the hovered value over the selected one", async () => {
      render(<Shell />);
      selectAuthor();
      await userEvent.hover(screen.getByText("hovered value"));
      expect(await screen.findByText("wp_postmeta.meta_value · meta_id 9")).toBeInTheDocument();
      expect(screen.queryByText("wp_posts.post_author · ID 5")).not.toBeInTheDocument();
    });

    it("shows the address for a selected value without provenance", () => {
      render(<StatusBar />);
      act(() => useStore.getState().selectCell({ row: 0, col: 0, column: columnMeta("c", "text"), cell: num("5"), source: { sessionId: "s1", resultId: 1 }, value: value() }));
      expect(screen.getByRole("button", { name: "databee://shop/db/wp_posts" })).toBeInTheDocument();
    });
  });

  it("shows plain text and no button when nothing is pending", () => {
    render(<StatusBar />);
    expect(screen.getByText("pending 0")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /pending/ })).not.toBeInTheDocument();
  });

  it("flags pending changes", () => {
    useStore.setState({ pending: [pendingChange()] });
    render(<StatusBar />);
    expect(screen.getByRole("button", { name: "1 pending change" })).toBeInTheDocument();
  });

  it("counts only the active session's pending changes", () => {
    useStore.setState({ pending: [pendingChange({ id: 1 }), pendingChange({ id: 2, sessionId: "s2" }), pendingChange({ id: 3, sessionId: "s2" })] });
    render(<StatusBar />);
    expect(screen.getByRole("button", { name: "1 pending change" })).toBeInTheDocument();
  });

  it("shows no pending chip when only another session has changes", () => {
    useStore.setState({ pending: [pendingChange({ sessionId: "s2" })] });
    render(<StatusBar />);
    expect(screen.queryByRole("button", { name: /pending/ })).not.toBeInTheDocument();
  });
});
