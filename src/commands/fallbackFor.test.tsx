import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { CommandPalette } from "../components/CommandPalette";
import { Dialogs } from "../components/Dialogs";
import { useStore } from "../state/store";
import { connectedStore, pendingChange, queryTab } from "../test/fixtures";
import { bindingFor, clearCommands, conflicts, handleKeydown, installCommands, resolveKeydown, setOverrides } from "./index";
import { activeScopes } from "./scopes";

const mocked = vi.mocked(api, true);

const modS = { key: "s", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };

describe("a command that falls back to another command's key", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Element.prototype.scrollIntoView = vi.fn();
    connectedStore({ tabs: [queryTab({ sql: "SELECT 1" })] });
    installCommands();
    window.addEventListener("keydown", handleKeydown, true);
  });

  afterEach(() => {
    window.removeEventListener("keydown", handleKeydown, true);
    delete (Element.prototype as Partial<Element>).scrollIntoView;
    clearCommands();
  });

  describe("with nothing pending", () => {
    it("resolves ⌘S to query.save", () => {
      expect(resolveKeydown(modS, activeScopes())?.id).toBe("query.save");
    });

    it("saves the query of the active tab on ⌘S", async () => {
      render(<Dialogs />);
      await userEvent.keyboard("{Meta>}s{/Meta}");
      expect(await screen.findByRole("dialog", { name: "Save query as" })).toBeInTheDocument();
      await userEvent.keyboard("{Enter}");
      await waitFor(() => expect(mocked.saveQuery).toHaveBeenCalledWith("s1", "db", "Query 1", "SELECT 1", null));
      expect(mocked.applyChanges).not.toHaveBeenCalled();
    });

    it("lists Save query in the palette and leaves out Apply pending changes", async () => {
      useStore.getState().setPaletteOpen(true);
      render(<CommandPalette />);
      const search = screen.getByLabelText("Search everything");
      await userEvent.type(search, "Save query");
      expect(screen.getByText("Save query")).toBeInTheDocument();
      await userEvent.clear(search);
      await userEvent.type(search, "Apply pending");
      expect(screen.queryByText("Apply pending changes")).not.toBeInTheDocument();
    });
  });

  describe("with pending changes", () => {
    beforeEach(() => useStore.setState({ pending: [pendingChange({ id: 4 })] }));

    it("resolves ⌘S to changes.apply", () => {
      expect(resolveKeydown(modS, activeScopes())?.id).toBe("changes.apply");
    });

    it("applies the changes on ⌘S instead of saving the query", async () => {
      render(<Dialogs />);
      await userEvent.keyboard("{Meta>}s{/Meta}");
      await waitFor(() => expect(mocked.applyChanges).toHaveBeenCalledWith("s1", [4]));
      expect(screen.queryByRole("dialog", { name: "Save query as" })).not.toBeInTheDocument();
    });
  });

  describe("bindings", () => {
    it("reports no conflict for ⌘S", () => {
      expect(conflicts().filter((c) => c.binding === "Mod+S")).toEqual([]);
    });

    it("gives query.save the binding of changes.apply, also after a rebinding", () => {
      expect(bindingFor("query.save")).toBe("Mod+S");
      setOverrides({ "changes.apply": "Mod+Shift+S" });
      expect(bindingFor("query.save")).toBe("Mod+Shift+S");
      expect(bindingFor("changes.apply")).toBe("Mod+Shift+S");
      expect(conflicts({ "changes.apply": "Mod+Shift+S" }).filter((c) => c.binding === "Mod+Shift+S")).toEqual([]);
    });

    it("does not unbind query.save by an override on query.save itself", () => {
      setOverrides({ "query.save": "Mod+J" });
      expect(bindingFor("query.save")).toBe("Mod+S");
    });
  });
});
