import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { NO_HOVER, useStore } from "../../state/store";
import { useValueHover } from "./useValueHover";

function Area() {
  const ref = useRef<HTMLDivElement>(null);
  useValueHover(ref);
  return (
    <div role="application" ref={ref}>
      <span className="val" data-src="wp_posts.post_title · ID 7" data-table="wp_posts">
        a value
      </span>
      <a data-table="wp_users">a table link</a>
      <span>plain</span>
      <div data-row-table="wp_options">
        <span>row space</span>
        <div className="sh" data-table="wp_posts">
          band
        </div>
        <span className="val" data-src="wp_options.option_value · option_id 3" data-table="wp_options">
          row value
        </span>
        <span data-table="wp_users">row link</span>
      </div>
      <aside className="sidebar">
        <div role="treeitem" data-table="wp_comments">
          sidebar row
        </div>
      </aside>
    </div>
  );
}

describe("useValueHover", () => {
  beforeEach(() => useStore.setState({ hover: NO_HOVER }));

  it("reports a hovered value with its provenance and table, not as a table link", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("a value"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: "wp_posts.post_title · ID 7", table: "wp_posts", linked: false, sidebar: false }));
  });

  it("reports a hovered table link as linked, without provenance", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("a table link"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_users", linked: true, sidebar: false }));
  });

  it("marks a hovered sidebar row as sidebar, and a band outside the sidebar as not", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("sidebar row"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_comments", linked: true, sidebar: true }));
    await userEvent.hover(screen.getByText("band"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: true, sidebar: false }));
  });

  it("reports the table of a hovered row as unlinked, without provenance", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("row space"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_options", linked: false, sidebar: false }));
  });

  it("prefers the value inside a row over the row's table", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("row value"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: "wp_options.option_value · option_id 3", table: "wp_options", linked: false, sidebar: false }));
  });

  it("prefers a table link inside a row over the row's table", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("row link"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_users", linked: true, sidebar: false }));
  });

  it("clears the hover when the pointer moves to plain content or leaves", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("a value"));
    await waitFor(() => expect(useStore.getState().hover.table).toBe("wp_posts"));
    await userEvent.hover(screen.getByText("plain"));
    await waitFor(() => expect(useStore.getState().hover).toEqual(NO_HOVER));

    await userEvent.hover(screen.getByText("a table link"));
    await waitFor(() => expect(useStore.getState().hover.linked).toBe(true));
    fireEvent.mouseLeave(screen.getByRole("application"));
    await waitFor(() => expect(useStore.getState().hover).toEqual(NO_HOVER));
  });
});
