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
    </div>
  );
}

describe("useValueHover", () => {
  beforeEach(() => useStore.setState({ hover: NO_HOVER }));

  it("reports a hovered value with its provenance and table, not as a table link", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("a value"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: "wp_posts.post_title · ID 7", table: "wp_posts", linked: false }));
  });

  it("reports a hovered table link as linked, without provenance", async () => {
    render(<Area />);
    await userEvent.hover(screen.getByText("a table link"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_users", linked: true }));
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
