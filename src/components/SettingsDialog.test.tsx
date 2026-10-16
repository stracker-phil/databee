import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api } from "../api";
import { resetStore, useStore } from "../state/store";
import { connectedStore, settings } from "../test/fixtures";
import { installCommands } from "../commands";
import { SettingsDialog } from "./SettingsDialog";

const mocked = vi.mocked(api, true);

describe("SettingsDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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

  it("applies the saved theme and accent to the page", async () => {
    mocked.getSettings.mockResolvedValue(settings());
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Dark" }));
    await userEvent.click(screen.getByRole("button", { name: "Accent purple" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.dataset.accent).toBe("purple");
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

  it("shows the hook's status and the command that registers it with Stage", async () => {
    const command = "stage register http://127.0.0.1:27184/v1/stage --token token-123";
    mocked.stageStatus.mockResolvedValue({ enabled: true, running: true, error: null, url: "http://127.0.0.1:27184/v1/stage", registerCommand: command, lastRequest: new Date(Date.now() - 120_000).toISOString() });
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Presentations" }));
    expect(await screen.findByText("Ready")).toBeInTheDocument();
    expect(screen.getByText(command)).toBeInTheDocument();
    expect(screen.getByText("2 min ago")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(command);

    expect(screen.queryByText("token-123")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "••••••••••••" }));
    expect(screen.getByText("token-123")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "New Token" }));
    expect(await screen.findByText("token-new")).toBeInTheDocument();

    mocked.stageStatus.mockResolvedValue({ enabled: false, running: false, error: null, url: "http://127.0.0.1:27184/v1/stage", registerCommand: command, lastRequest: null });
    await userEvent.click(screen.getByLabelText("Allow presentations"));
    expect(mocked.stageUpdateSettings).toHaveBeenLastCalledWith({ enabled: false, port: 27184, token: "token-new" });
    expect(await screen.findByText("Off")).toBeInTheDocument();
    expect(screen.getByText("None yet")).toBeInTheDocument();
  });

  it("copies the token", async () => {
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Presentations" }));
    await userEvent.click(await screen.findByRole("button", { name: "Copy token" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("token-123");
  });

  it("saves a changed port through the Stage settings", async () => {
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Presentations" }));
    const port = await screen.findByLabelText("Port");
    await userEvent.clear(port);
    await userEvent.type(port, "30000");
    await userEvent.tab();
    expect(mocked.stageUpdateSettings).toHaveBeenLastCalledWith({ enabled: true, port: 30000, token: "token-123" });
  });

  it("names the port conflict when the API can't start", async () => {
    mocked.stageStatus.mockResolvedValue({ enabled: true, running: false, error: "Port 27184 is already used by another app.", url: "http://127.0.0.1:27184/v1/stage", registerCommand: "stage register http://127.0.0.1:27184/v1/stage --token t", lastRequest: null });
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Presentations" }));
    expect(await screen.findByText("Port 27184 is already used by another app.")).toBeInTheDocument();
  });

  it("opens on the page the dialog names", () => {
    resetStore({ settings: settings(), dialog: { type: "settings", page: "logging" } });
    render(<SettingsDialog />);
    expect(screen.getByRole("heading", { name: "Logging" })).toBeInTheDocument();
  });
});
