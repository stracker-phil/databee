import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { PopUpButton } from "./PopUpButton";

const fruit = [
  { value: "apple", label: "Apple" },
  { value: "banana", label: "Banana" },
  { value: "avocado", label: "Avocado" },
];

type Props = Partial<Parameters<typeof PopUpButton<string>>[0]>;

function Harness({ initial = "apple", onChange = () => {}, ...props }: Props & { initial?: string }) {
  const [value, setValue] = useState(initial);
  return (
    <PopUpButton
      aria-label="Fruit"
      options={fruit}
      {...props}
      value={value}
      onChange={(v) => {
        setValue(v);
        onChange(v);
      }}
    />
  );
}

const button = () => screen.getByRole("button", { name: "Fruit" });
const open = () => userEvent.click(button());
const item = (name: string) => screen.getByRole("menuitemradio", { name });

describe("PopUpButton", () => {
  describe("button", () => {
    it("shows the current option's label and is named by its aria-label", () => {
      render(<Harness plain initial="banana" />);
      expect(button()).toHaveTextContent(/^Banana$/);
      expect(button()).toHaveAttribute("aria-haspopup", "menu");
      expect(button()).toHaveAttribute("aria-expanded", "false");
    });

    it("is named by a label that points at its id, which also names the menu", async () => {
      render(
        <>
          <label htmlFor="scheme">Colour scheme</label>
          <PopUpButton id="scheme" value="apple" options={fruit} onChange={() => {}} />
        </>,
      );
      await userEvent.click(screen.getByRole("button", { name: "Colour scheme" }));
      expect(screen.getByRole("menu", { name: "Colour scheme" })).toBeInTheDocument();
    });

    it("passes data-stage attributes through", () => {
      render(<Harness data-stage="form:fruit" data-stage-label="Fruit" />);
      expect(button()).toHaveAttribute("data-stage", "form:fruit");
      expect(button()).toHaveAttribute("data-stage-label", "Fruit");
    });

    it("does not open while disabled", async () => {
      render(<Harness disabled />);
      await open();
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    });
  });

  describe("opening", () => {
    it("opens on click with a check on the current option and focus on it", async () => {
      render(<Harness initial="banana" />);
      await open();
      expect(screen.getByRole("menu", { name: "Fruit" })).toBeInTheDocument();
      expect(button()).toHaveAttribute("aria-expanded", "true");
      expect(item("Banana")).toHaveAttribute("aria-checked", "true");
      expect(item("Apple")).toHaveAttribute("aria-checked", "false");
      expect(item("Avocado")).toHaveAttribute("aria-checked", "false");
      expect(item("Banana")).toHaveFocus();
    });

    it("focuses the first option when the current value is not among them", async () => {
      render(<Harness initial="gone" />);
      await open();
      expect(item("Apple")).toHaveFocus();
    });

    it("opens on ArrowDown and ArrowUp from the button", async () => {
      render(<Harness />);
      button().focus();
      await userEvent.keyboard("{ArrowDown}");
      expect(screen.getByRole("menu")).toBeInTheDocument();
      await userEvent.keyboard("{Escape}");
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
      await userEvent.keyboard("{ArrowUp}");
      expect(screen.getByRole("menu")).toBeInTheDocument();
    });

    it("lists groups under their captions", async () => {
      render(
        <PopUpButton
          aria-label="Scheme"
          value="dusk"
          onChange={() => {}}
          options={[
            { caption: "Light", options: [{ value: "dawn", label: "Dawn" }] },
            { caption: "Dark", options: [{ value: "dusk", label: "Dusk" }, { value: "night", label: "Night" }] },
          ]}
        />,
      );
      await userEvent.click(screen.getByRole("button", { name: "Scheme" }));
      expect(within(screen.getByRole("group", { name: "Light" })).getAllByRole("menuitemradio")).toHaveLength(1);
      const dark = screen.getByRole("group", { name: "Dark" });
      expect(within(dark).getByRole("menuitemradio", { name: "Night" })).toBeInTheDocument();
      expect(within(dark).getByRole("menuitemradio", { name: "Dusk" })).toHaveAttribute("aria-checked", "true");
    });
  });

  describe("keys in the menu", () => {
    it("moves focus with ArrowDown and wraps from the last to the first option", async () => {
      render(<Harness initial="banana" />);
      await open();
      await userEvent.keyboard("{ArrowDown}");
      expect(item("Avocado")).toHaveFocus();
      await userEvent.keyboard("{ArrowDown}");
      expect(item("Apple")).toHaveFocus();
    });

    it("moves focus with ArrowUp and wraps from the first to the last option", async () => {
      render(<Harness />);
      await open();
      await userEvent.keyboard("{ArrowUp}");
      expect(item("Avocado")).toHaveFocus();
      await userEvent.keyboard("{ArrowUp}");
      expect(item("Banana")).toHaveFocus();
    });

    it("jumps to the last and first option with End and Home", async () => {
      render(<Harness initial="banana" />);
      await open();
      await userEvent.keyboard("{End}");
      expect(item("Avocado")).toHaveFocus();
      await userEvent.keyboard("{Home}");
      expect(item("Apple")).toHaveFocus();
    });

    it("focuses the next option starting with the typed letter and wraps around", async () => {
      render(<Harness />);
      await open();
      await userEvent.keyboard("a");
      expect(item("Avocado")).toHaveFocus();
      await userEvent.keyboard("a");
      expect(item("Apple")).toHaveFocus();
      await userEvent.keyboard("b");
      expect(item("Banana")).toHaveFocus();
    });

    it("keeps focus when no option starts with the typed letter", async () => {
      render(<Harness />);
      await open();
      await userEvent.keyboard("z");
      expect(item("Apple")).toHaveFocus();
    });

    it("chooses the focused option with Enter", async () => {
      const onChange = vi.fn();
      render(<Harness onChange={onChange} />);
      await open();
      await userEvent.keyboard("{ArrowDown}{Enter}");
      expect(onChange).toHaveBeenCalledWith("banana");
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    });

    it("closes on Escape and returns focus to the button without choosing", async () => {
      const onChange = vi.fn();
      render(<Harness onChange={onChange} />);
      await open();
      await userEvent.keyboard("{ArrowDown}{Escape}");
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
      expect(button()).toHaveFocus();
      expect(onChange).not.toHaveBeenCalled();
    });
  });

  describe("choosing", () => {
    it("calls onChange with the value, closes, returns focus and shows the new label", async () => {
      const onChange = vi.fn();
      render(<Harness plain onChange={onChange} />);
      await open();
      await userEvent.click(item("Banana"));
      expect(onChange).toHaveBeenCalledWith("banana");
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
      expect(button()).toHaveFocus();
      expect(button()).toHaveTextContent(/^Banana$/);
    });

    it("closes without calling onChange when the current option is chosen", async () => {
      const onChange = vi.fn();
      render(<Harness onChange={onChange} />);
      await open();
      await userEvent.click(item("Apple"));
      expect(onChange).not.toHaveBeenCalled();
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
      expect(button()).toHaveFocus();
    });
  });

  describe("closing", () => {
    it("closes on a mouse press outside the menu", async () => {
      render(<Harness />);
      await open();
      await userEvent.click(document.body);
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
      expect(button()).toHaveAttribute("aria-expanded", "false");
    });

    it("closes on a second click on the button", async () => {
      render(<Harness />);
      await open();
      await open();
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    });

    it("stays open on a mouse press on the menu itself", async () => {
      render(<Harness />);
      await open();
      await userEvent.click(screen.getByRole("menu"));
      expect(screen.getByRole("menu")).toBeInTheDocument();
    });
  });
});
