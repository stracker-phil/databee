import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { connectedStore } from "../test/fixtures";
import { Sidebar } from "./Sidebar";

const overlay = () => document.body.querySelector(".overflow-reveal");
const row = (name: string) => screen.getByText(name).closest<HTMLElement>("[role=treeitem]")!;

function cutOff(name: string, cut: boolean) {
  const label = screen.getByText(name);
  Object.defineProperty(label, "scrollWidth", { configurable: true, value: cut ? 200 : 100 });
  Object.defineProperty(label, "clientWidth", { configurable: true, value: 100 });
}

describe("useOverflowReveal in the Sidebar tree", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    render(<Sidebar />);
  });

  it("shows the full name of a cut-off table at once when its label is hovered", async () => {
    cutOff("wp_postmeta", true);
    await userEvent.hover(screen.getByText("wp_postmeta"));
    expect(overlay()).toHaveTextContent("wp_postmeta");
    expect(overlay()).toHaveAttribute("aria-hidden", "true");
  });

  it("shows the name when the row's icon is hovered instead of the label", async () => {
    cutOff("wp_postmeta", true);
    await userEvent.hover(row("wp_postmeta").querySelector("[data-icon]")!);
    expect(overlay()).toHaveTextContent("wp_postmeta");
  });

  it("shows no overlay for a name that fits", async () => {
    cutOff("wp_options", false);
    await userEvent.hover(screen.getByText("wp_options"));
    expect(overlay()).toBeNull();
  });

  it("replaces the overlay with the next cut-off row's name", async () => {
    cutOff("wp_postmeta", true);
    cutOff("wp_posts", true);
    await userEvent.hover(screen.getByText("wp_postmeta"));
    await userEvent.hover(screen.getByText("wp_posts"));
    expect(overlay()).toHaveTextContent("wp_posts");
    expect(document.body.querySelectorAll(".overflow-reveal")).toHaveLength(1);
  });

  it("removes the overlay when the pointer moves to a row that fits", async () => {
    cutOff("wp_postmeta", true);
    cutOff("wp_options", false);
    await userEvent.hover(screen.getByText("wp_postmeta"));
    await userEvent.hover(screen.getByText("wp_options"));
    expect(overlay()).toBeNull();
  });

  describe("once shown", () => {
    beforeEach(async () => {
      cutOff("wp_postmeta", true);
      await userEvent.hover(screen.getByText("wp_postmeta"));
      expect(overlay()).not.toBeNull();
    });

    it("disappears when the pointer leaves the tree", () => {
      fireEvent.mouseLeave(screen.getByRole("tree"));
      expect(overlay()).toBeNull();
    });

    it("disappears when any element scrolls", () => {
      fireEvent.scroll(screen.getByRole("tree"));
      expect(overlay()).toBeNull();
    });

    it("disappears when the window loses focus", () => {
      fireEvent.blur(window);
      expect(overlay()).toBeNull();
    });
  });
});
