import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { installCommands, setOverrides } from "../../commands";
import { useStore } from "../../state/store";
import { connectedStore } from "../../test/fixtures";
import { ShortcutsPage } from "./ShortcutsPage";

const mocked = vi.mocked(api, true);

const row = (title: string) => within(screen.getByText(title).closest("[data-anchor]") as HTMLElement);
const field = (title: string) => row(title).getByRole("button", { name: /Record shortcut|Press the new shortcut/ });
const record = async (title: string, keys: string) => {
  await userEvent.click(field(title));
  await userEvent.keyboard(keys);
};
const freeKey = "{Control>}{Alt>}j{/Alt}{/Control}";
const lastSaved =() => mocked.setKeybindings.mock.calls.at(-1)![0];

describe("ShortcutsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    installCommands();
    setOverrides({});
    render(<ShortcutsPage />);
  });

  describe("grouping", () => {
    it("lists the scopes under headings in the order of where their keys work", () => {
      const headings = screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
      expect(headings.slice(0, 5)).toEqual(["Everywhere", "With a database open", "In a table tab", "In the SQL editor", "In an object tab"]);
    });

    it.each([
      ["Zoom in", "Everywhere"],
      ["Filter tables", "With a database open"],
      ["Value filter (WHERE)", "In a table tab"],
      ["Run statement at cursor", "In the SQL editor"],
      ["Show Document", "In an object tab"],
    ])("shows %s under %s", (title, heading) => {
      expect(within(screen.getByRole("region", { name: heading })).getByText(title)).toBeInTheDocument();
    });

    it("shows each command's default shortcut", () => {
      expect(field("Command palette")).toHaveTextContent("⌘K");
    });
  });

  describe("recording", () => {
    it("saves a recorded shortcut as an override and shows it", async () => {
      await record("Command palette", freeKey);
      expect(field("Command palette")).toHaveTextContent("⌃⌥J");
      expect(mocked.setKeybindings).toHaveBeenLastCalledWith({ "palette.open": "Ctrl+Alt+J" });
    });

    it("keeps the shortcut when Escape cancels the recording", async () => {
      await record("Command palette", "{Escape}");
      expect(field("Command palette")).toHaveTextContent("⌘K");
      expect(mocked.setKeybindings).not.toHaveBeenCalled();
    });

    it("unbinds the command on Backspace", async () => {
      await record("Command palette", "{Backspace}");
      expect(field("Command palette")).toHaveTextContent("unbound");
      expect(mocked.setKeybindings).toHaveBeenLastCalledWith({ "palette.open": null });
    });

    it("drops the override when the command's own default key is recorded again", async () => {
      await record("Command palette", freeKey);
      await record("Command palette", "{Meta>}k{/Meta}");
      expect(lastSaved()).toEqual({});
      expect(field("Command palette")).toHaveTextContent("⌘K");
    });
  });

  describe("reset", () => {
    it("offers no reset while the key is the default", () => {
      expect(screen.queryByRole("button", { name: "Reset Command palette" })).not.toBeInTheDocument();
    });

    it("offers a reset while the key differs from the default, and restores the default", async () => {
      await record("Command palette", freeKey);
      await userEvent.click(screen.getByRole("button", { name: "Reset Command palette" }));
      expect(field("Command palette")).toHaveTextContent("⌘K");
      expect(lastSaved()).toEqual({});
      expect(screen.queryByRole("button", { name: "Reset Command palette" })).not.toBeInTheDocument();
    });

    it("offers a reset for an unbound command", async () => {
      await record("Command palette", "{Backspace}");
      expect(screen.getByRole("button", { name: "Reset Command palette" })).toBeInTheDocument();
    });
  });

  describe("a key another command holds", () => {
    const takeZoomInKey = () => record("Zoom in", "{Meta>}k{/Meta}");

    it("asks before taking the key, naming the command that has it", async () => {
      await takeZoomInKey();
      const dialog = screen.getByRole("dialog");
      expect(within(dialog).getByRole("heading", { name: "⌘K is used by “Command palette”" })).toBeInTheDocument();
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeInTheDocument();
      expect(within(dialog).getByRole("button", { name: "Reassign" })).toBeInTheDocument();
      expect(mocked.setKeybindings).not.toHaveBeenCalled();
    });

    it("saves nothing on Cancel and keeps both shortcuts", async () => {
      await takeZoomInKey();
      await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(mocked.setKeybindings).not.toHaveBeenCalled();
      expect(field("Command palette")).toHaveTextContent("⌘K");
      expect(field("Zoom in")).toHaveTextContent("⌘=");
    });

    it("saves the new key and unbinds the other command in one save on Reassign", async () => {
      await takeZoomInKey();
      await userEvent.click(screen.getByRole("button", { name: "Reassign" }));
      expect(mocked.setKeybindings.mock.calls).toEqual([[{ "palette.open": null, "view.zoomIn": "Mod+K" }]]);
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(field("Zoom in")).toHaveTextContent("⌘K");
      expect(field("Command palette")).toHaveTextContent("unbound");
      expect(useStore.getState().keybindings).toEqual({ "palette.open": null, "view.zoomIn": "Mod+K" });
    });
  });
});
