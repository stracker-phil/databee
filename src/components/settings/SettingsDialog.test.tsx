import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Blocks } from "lucide-react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, LOG_LEVELS } from "../../api";
import { registerSettingsPage } from "../../registry/settingsPages";
import { resetStore, useStore } from "../../state/store";
import { connectedStore, settings } from "../../test/fixtures";
import { clearCommands, installCommands, registerCommands, registerScope } from "../../commands";
import { registerCoreSettingsPages } from "./pages";
import { SettingsDialog } from "./SettingsDialog";

const mocked = vi.mocked(api, true);

describe("SettingsDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    registerCoreSettingsPages();
    resetStore({ settings: settings(), dialog: { type: "settings" } });
  });

  it("applies changes immediately, committing text fields on blur", async () => {
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Dark" }));
    expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ theme: "dark" }));
    await userEvent.click(screen.getByRole("button", { name: "Display" }));
    const nullLabel = screen.getByLabelText("NULL label");
    await userEvent.clear(nullLabel);
    await userEvent.type(nullLabel, "∅");
    expect(nullLabel).toHaveValue("∅");
    expect(mocked.updateSettings).not.toHaveBeenCalledWith(expect.objectContaining({ nullLabel: "∅" }));
    await userEvent.tab();
    expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ nullLabel: "∅", theme: "dark" }));
  });

  describe("number fields", () => {
    it("commit a typed size on blur, not per keystroke", async () => {
      render(<SettingsDialog />);
      const uiSize = screen.getByLabelText("UI size");
      await userEvent.clear(uiSize);
      await userEvent.type(uiSize, "16");
      expect(mocked.updateSettings).not.toHaveBeenCalled();
      await userEvent.tab();
      expect(mocked.updateSettings).not.toHaveBeenCalledWith(expect.objectContaining({ uiFontSize: 1 }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ uiFontSize: 16 }));
      expect(uiSize).toHaveValue(16);
    });

    it.each([
      ["on blur", "{Tab}"],
      ["on Enter", "{Enter}"],
    ])("commit a typed six-digit value without partials %s", async (_when, key) => {
      render(<SettingsDialog />);
      await userEvent.click(screen.getByRole("button", { name: "Queries" }));
      const rowLimit = screen.getByLabelText("Row limit");
      await userEvent.clear(rowLimit);
      await userEvent.type(rowLimit, `100000${key}`);
      for (const partial of [1, 10, 100, 1000, 10000]) {
        expect(mocked.updateSettings).not.toHaveBeenCalledWith(expect.objectContaining({ rowLimit: partial }));
      }
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ rowLimit: 100000 }));
    });

    it("restore the saved value instead of committing an out-of-range one", async () => {
      render(<SettingsDialog />);
      const uiSize = screen.getByLabelText("UI size");
      await userEvent.clear(uiSize);
      await userEvent.type(uiSize, "99");
      await userEvent.tab();
      expect(mocked.updateSettings).not.toHaveBeenCalled();
      expect(uiSize).toHaveValue(13);
    });

    it("preview a spinner step after a short pause", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      try {
        render(<SettingsDialog />);
        fireEvent.change(screen.getByLabelText("UI size"), { target: { value: "14" } });
        expect(mocked.updateSettings).not.toHaveBeenCalled();
        await act(async () => {
          await vi.advanceTimersByTimeAsync(300);
        });
        expect(mocked.updateSettings).toHaveBeenCalledWith(expect.objectContaining({ uiFontSize: 14 }));
      } finally {
        vi.useRealTimers();
      }
    });
  });

  it("switches pages from the sidebar", async () => {
    render(<SettingsDialog />);
    expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "WordPress" }));
    expect(screen.getByRole("heading", { name: "WordPress" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: "Gravatar avatars" }));
    expect(mocked.updateSettings).toHaveBeenCalledWith(expect.objectContaining({ gravatar: !settings().gravatar }));
  });

  it("closes without saving anything when nothing changed", async () => {
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Close settings" }));
    expect(mocked.updateSettings).not.toHaveBeenCalled();
    expect(useStore.getState().dialog).toBeNull();
  });

  describe("Appearance page", () => {
    const root = document.documentElement;
    const start = (patch: Partial<ReturnType<typeof settings>> = {}) => {
      mocked.getSettings.mockResolvedValue(settings(patch));
      resetStore({ settings: settings(patch), dialog: { type: "settings" } });
      render(<App />);
    };

    it("applies the saved theme and accent to the page", async () => {
      start();
      await userEvent.click(await screen.findByRole("button", { name: "Dark" }));
      await userEvent.click(screen.getByRole("button", { name: "Accent purple" }));
      expect(root.dataset.scheme).toBe("ayu-mirage");
      expect(root.dataset.accent).toBe("purple");
    });

    it("saves the accent and applies it", async () => {
      start();
      await userEvent.click(await screen.findByRole("button", { name: "Accent gold" }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ accent: "gold" }));
      expect(root.dataset.accent).toBe("gold");
    });

    it.each([
      ["Dark", "dark", "ayu-mirage"],
      ["Light", "light", "ayu-light"],
    ])("switches to the %s scheme", async (name, theme, scheme) => {
      start({ theme: theme === "dark" ? "light" : "dark" });
      await userEvent.click(await screen.findByRole("button", { name }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ theme }));
      expect(root.dataset.scheme).toBe(scheme);
    });

    it("saves the row padding and applies it", async () => {
      start();
      await userEvent.click(within(await screen.findByRole("group", { name: "Row padding" })).getByRole("button", { name: "1" }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ rowPadding: 1 }));
      expect(root.style.getPropertyValue("--pad")).toBe("1px");
    });

    it("saves a typed mono font on Enter and applies it", async () => {
      start();
      await userEvent.type(await screen.findByLabelText("Mono font"), "Fira Code{Enter}");
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ fontMono: "Fira Code" }));
      expect(root.style.getPropertyValue("--font-mono")).toContain("Fira Code");
    });

    it("saves null and drops the override when the mono font is emptied", async () => {
      start({ fontMono: "Fira Code" });
      const field = await screen.findByLabelText("Mono font");
      await userEvent.clear(field);
      await userEvent.keyboard("{Enter}");
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ fontMono: null }));
      expect(root.style.getPropertyValue("--font-mono")).toBe("");
    });

    it.each([
      ["Light scheme", "Ayu Mirage", "lightScheme", "ayu-mirage"],
      ["Dark scheme", "Ayu Light", "darkScheme", "ayu-light"],
    ])("saves the %s selection", async (label, option, key, id) => {
      start();
      await userEvent.selectOptions(await screen.findByLabelText(label), option);
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ [key]: id }));
    });
  });

  it("records, resets and saves shortcut overrides", async () => {
    connectedStore();
    useStore.getState().openDialog({ type: "settings", page: "shortcuts" });
    render(<App />);
    await userEvent.type(await screen.findByLabelText("Search shortcuts"), "Command palette");
    const field = screen.getByRole("button", { name: "Record shortcut" });
    expect(field).toHaveTextContent("⌘K");
    await userEvent.click(field);
    await userEvent.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘⇧P");
    expect(mocked.setKeybindings).toHaveBeenLastCalledWith({ "palette.open": "Mod+Shift+P" });

    await userEvent.click(screen.getByRole("button", { name: "Reset Command palette" }));
    expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘K");
    expect(mocked.setKeybindings).toHaveBeenLastCalledWith(expect.not.objectContaining({ "palette.open": expect.anything() }));
  });

  it("records an already-bound shortcut instead of running its command", async () => {
    connectedStore();
    useStore.getState().openDialog({ type: "settings", page: "shortcuts" });
    render(<App />);
    await userEvent.type(await screen.findByLabelText("Search shortcuts"), "Command palette");
    await userEvent.click(screen.getByRole("button", { name: "Record shortcut" }));
    await userEvent.keyboard("{Meta>}t{/Meta}");
    expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘T");
    expect(mocked.setKeybindings).toHaveBeenCalledWith({ "palette.open": "Mod+T" });
    expect(screen.queryByRole("tab", { name: /Query 2/ })).not.toBeInTheDocument();
    expect(useStore.getState().tabs).toHaveLength(1);
  });

  it("finds a command by its id in the shortcut search", async () => {
    installCommands();
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
    await userEvent.type(screen.getByLabelText("Search shortcuts"), "inspectordock");
    expect(screen.getByText("Toggle inspector position (right / bottom)")).toBeInTheDocument();
    expect(screen.queryByText("Zoom in")).not.toBeInTheDocument();
  });

  it("says so when no command matches the shortcut search", async () => {
    installCommands();
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
    await userEvent.type(screen.getByLabelText("Search shortcuts"), "zzzz");
    expect(screen.getByText("No matching commands.")).toBeInTheDocument();
  });

  it("shows each command's scope label", async () => {
    registerScope({ id: "labelled", parent: "global", label: "Labelled scope", isActive: () => false });
    clearCommands();
    registerCommands([{ id: "test.labelled", title: "Labelled command", scope: "labelled", run: () => {} }]);
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
    expect(screen.getByText("Labelled scope")).toBeInTheDocument();
    clearCommands();
  });

  it.each([
    ["Zoom in", "Everywhere"],
    ["Run statement at cursor", "Editor"],
    ["Value filter", "Table"],
    ["Switch between Document and Rows", "Object"],
  ])("shows the scope label of %s", async (title, label) => {
    installCommands();
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
    await userEvent.type(screen.getByLabelText("Search shortcuts"), title);
    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it("warns when two commands in overlapping scopes share a binding", async () => {
    installCommands();
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
    await userEvent.type(screen.getByLabelText("Search shortcuts"), "Zoom in");
    await userEvent.click(screen.getByRole("button", { name: "Record shortcut" }));
    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(screen.getByRole("alert")).toHaveTextContent(/⌘K is used by both/);
    await userEvent.click(screen.getByRole("button", { name: "Reset Zoom in" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("warns that trace logging records SQL", async () => {
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Logging" }));
    await userEvent.selectOptions(screen.getByLabelText("Log level"), "trace");
    expect(screen.getByText(/include the SQL text/)).toBeInTheDocument();
  });

  it("lists every log level", async () => {
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Logging" }));
    const options = within(screen.getByLabelText("Log level")).getAllByRole("option");
    expect(options.map((o) => (o as HTMLOptionElement).value)).toEqual([...LOG_LEVELS]);
  });

  it("opens on the page the dialog names", () => {
    resetStore({ settings: settings(), dialog: { type: "settings", page: "logging" } });
    render(<SettingsDialog />);
    expect(screen.getByRole("heading", { name: "Logging" })).toBeInTheDocument();
  });

  it("opens on the first page when the dialog names an unknown one", () => {
    resetStore({ settings: settings(), dialog: { type: "settings", page: "no-such-page" } });
    render(<SettingsDialog />);
    expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
  });

  describe("registered pages", () => {
    const body = (text: string) => () => <p>{text}</p>;

    it("lists a registered page in the nav under its group and renders its body", async () => {
      registerSettingsPage({ id: "test-extra", group: "Test Group", label: "Extra", icon: Blocks, intro: "Extra intro", Body: body("Extra body") });
      render(<SettingsDialog />);
      const nav = screen.getByRole("navigation", { name: "Settings pages" });
      expect(within(nav).getByText("Test Group")).toBeInTheDocument();
      await userEvent.click(within(nav).getByRole("button", { name: "Extra" }));
      expect(screen.getByRole("heading", { name: "Extra" })).toBeInTheDocument();
      expect(screen.getByText("Extra intro")).toBeInTheDocument();
      expect(screen.getByText("Extra body")).toBeInTheDocument();
    });

    it("keeps one nav entry with the later definition when an id is registered twice", () => {
      registerSettingsPage({ id: "test-twice", group: "Test Group", label: "Twice old", icon: Blocks, Body: body("Old body") });
      registerSettingsPage({ id: "test-twice", group: "Test Group", label: "Twice new", icon: Blocks, Body: body("New body") });
      resetStore({ settings: settings(), dialog: { type: "settings", page: "test-twice" } });
      render(<SettingsDialog />);
      const nav = screen.getByRole("navigation", { name: "Settings pages" });
      expect(within(nav).getAllByRole("button", { name: /^Twice/ })).toHaveLength(1);
      expect(within(nav).getByRole("button", { name: "Twice new" })).toBeInTheDocument();
      expect(screen.getByText("New body")).toBeInTheDocument();
      expect(screen.queryByText("Old body")).not.toBeInTheDocument();
    });

    it("adds a page registered while the dialog is open", () => {
      render(<SettingsDialog />);
      expect(screen.queryByRole("button", { name: "Late" })).not.toBeInTheDocument();
      act(() => registerSettingsPage({ id: "test-late", group: "Test Group", label: "Late", icon: Blocks, Body: body("Late body") }));
      expect(screen.getByRole("button", { name: "Late" })).toBeInTheDocument();
    });
  });
});
