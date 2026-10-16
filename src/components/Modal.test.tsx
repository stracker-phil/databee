import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Modal } from "./Modal";

function Sheet({ onClose = () => {} }: { onClose?: () => void }) {
  return (
    <>
      <button>Before</button>
      <Modal onClose={onClose} label="Sheet">
        <button>First</button>
        <input aria-label="Middle" />
        <button>Last</button>
      </Modal>
      <button>After</button>
    </>
  );
}

function WithTrigger() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Open</button>
      {open && (
        <Modal onClose={() => setOpen(false)} label="Sheet">
          <button onClick={() => setOpen(false)}>Done</button>
        </Modal>
      )}
    </>
  );
}

describe("Modal", () => {
  describe("focus", () => {
    it("focuses the first control when it opens", () => {
      render(<Sheet />);
      expect(screen.getByRole("button", { name: "First" })).toHaveFocus();
    });

    it("keeps the focus of a child that grabs it itself", () => {
      render(
        <Modal onClose={() => {}} label="Sheet">
          <button>First</button>
          <input aria-label="Name" autoFocus />
        </Modal>,
      );
      expect(screen.getByRole("textbox", { name: "Name" })).toHaveFocus();
    });

    it("wraps Tab from the last control to the first without leaving the sheet", async () => {
      render(<Sheet />);
      await userEvent.tab();
      expect(screen.getByRole("textbox", { name: "Middle" })).toHaveFocus();
      await userEvent.tab();
      expect(screen.getByRole("button", { name: "Last" })).toHaveFocus();
      await userEvent.tab();
      expect(screen.getByRole("button", { name: "First" })).toHaveFocus();
    });

    it("wraps Shift+Tab from the first control to the last", async () => {
      render(<Sheet />);
      await userEvent.tab({ shift: true });
      expect(screen.getByRole("button", { name: "Last" })).toHaveFocus();
    });

    it("returns focus to the trigger when it closes", async () => {
      render(<WithTrigger />);
      const trigger = screen.getByRole("button", { name: "Open" });
      await userEvent.click(trigger);
      expect(screen.getByRole("button", { name: "Done" })).toHaveFocus();

      await userEvent.click(screen.getByRole("button", { name: "Done" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(trigger).toHaveFocus();
    });
  });

  describe("Escape", () => {
    it("closes the sheet and stops the key before a bubbling window listener sees it", async () => {
      const onClose = vi.fn();
      const bubbling = vi.fn();
      window.addEventListener("keydown", bubbling);
      try {
        render(<Sheet onClose={onClose} />);
        await userEvent.keyboard("{Escape}");
        expect(onClose).toHaveBeenCalled();
        expect(bubbling).not.toHaveBeenCalled();
      } finally {
        window.removeEventListener("keydown", bubbling);
      }
    });

    it("lets the key through inside a shortcut recorder", async () => {
      const onClose = vi.fn();
      const bubbling = vi.fn();
      window.addEventListener("keydown", bubbling);
      try {
        render(
          <Modal onClose={onClose} label="Sheet">
            <button data-recording>Press keys…</button>
          </Modal>,
        );
        await userEvent.keyboard("{Escape}");
        expect(onClose).not.toHaveBeenCalled();
        expect(bubbling).toHaveBeenCalled();
      } finally {
        window.removeEventListener("keydown", bubbling);
      }
    });
  });

  it("closes when the backdrop is pressed", async () => {
    const onClose = vi.fn();
    render(<Sheet onClose={onClose} />);
    await userEvent.click(screen.getByRole("dialog", { name: "Sheet" }));
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("dialog", { name: "Sheet" }).parentElement!);
    expect(onClose).toHaveBeenCalled();
  });
});
