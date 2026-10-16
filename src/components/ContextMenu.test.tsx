import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ContextMenu, type MenuEntry } from "./ContextMenu";

const renderMenu = (items: MenuEntry[], onClose = vi.fn()) => {
  render(<ContextMenu x={10} y={10} items={items} onClose={onClose} />);
  return onClose;
};

describe("ContextMenu", () => {
  it("runs the clicked item's action and closes", async () => {
    const open = vi.fn();
    const remove = vi.fn();
    const onClose = renderMenu([
      { label: "Open", action: open },
      { label: "Remove", action: remove },
    ]);
    await userEvent.click(screen.getByRole("menuitem", { name: "Remove" }));
    expect(remove).toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("renders two items with the same label and runs the one that was clicked", async () => {
    const first = vi.fn();
    const second = vi.fn();
    renderMenu([
      { label: "Copy", action: first },
      "separator",
      { label: "Copy", action: second },
    ]);
    const copies = screen.getAllByRole("menuitem", { name: "Copy" });
    expect(copies).toHaveLength(2);
    await userEvent.click(copies[1]);
    expect(second).toHaveBeenCalled();
    expect(first).not.toHaveBeenCalled();
  });

  it("does not run a disabled item", async () => {
    const action = vi.fn();
    renderMenu([{ label: "Paste", action, disabled: true }]);
    await userEvent.click(screen.getByRole("menuitem", { name: "Paste" }));
    expect(action).not.toHaveBeenCalled();
  });

  describe("closing", () => {
    it("closes on Escape", async () => {
      const onClose = renderMenu([{ label: "Open", action: () => {} }]);
      await userEvent.keyboard("{Escape}");
      expect(onClose).toHaveBeenCalled();
    });

    it("closes on a mouse press outside the menu", async () => {
      const onClose = renderMenu([{ label: "Open", action: () => {} }]);
      await userEvent.click(document.body);
      expect(onClose).toHaveBeenCalled();
    });

    it("stays open on a mouse press on the menu itself", async () => {
      const onClose = renderMenu([{ label: "Open", action: () => {} }, "separator"]);
      await userEvent.click(screen.getByRole("menu"));
      expect(onClose).not.toHaveBeenCalled();
    });
  });

  describe("keyboard", () => {
    const items: MenuEntry[] = [
      { label: "Open", action: () => {} },
      { label: "Rename", action: () => {}, disabled: true },
      { label: "Remove", action: () => {} },
    ];

    it("focuses the first item when it opens", () => {
      renderMenu(items);
      expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
    });

    it("moves focus with ArrowDown, skips disabled items and wraps around", async () => {
      renderMenu(items);
      await userEvent.keyboard("{ArrowDown}");
      expect(screen.getByRole("menuitem", { name: "Remove" })).toHaveFocus();
      await userEvent.keyboard("{ArrowDown}");
      expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
    });

    it("moves focus back with ArrowUp and wraps to the last item", async () => {
      renderMenu(items);
      await userEvent.keyboard("{ArrowUp}");
      expect(screen.getByRole("menuitem", { name: "Remove" })).toHaveFocus();
      await userEvent.keyboard("{ArrowUp}");
      expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
    });

    it("returns focus to the element that opened it when it unmounts", () => {
      const opener = document.createElement("button");
      document.body.append(opener);
      opener.focus();
      const { unmount } = render(<ContextMenu x={0} y={0} items={items} onClose={() => {}} />);
      expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
      unmount();
      expect(opener).toHaveFocus();
      opener.remove();
    });
  });
});
