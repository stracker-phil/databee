import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api } from "../api";
import { resetStore, useStore } from "../state/store";
import { settings } from "../test/fixtures";
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
    expect(mocked.updateSettings).toHaveBeenCalledTimes(1);
    await userEvent.tab();
    expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ nullLabel: "∅", theme: "dark" }));
  });

  it("previews in-range sizes while typing", async () => {
    render(<SettingsDialog />);
    const uiSize = screen.getByLabelText("UI size");
    await userEvent.clear(uiSize);
    await userEvent.type(uiSize, "16");
    expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ uiFontSize: 16 }));
    expect(mocked.updateSettings).not.toHaveBeenCalledWith(expect.objectContaining({ uiFontSize: 1 }));
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
    installCommands();
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
    await userEvent.type(screen.getByLabelText("Search shortcuts"), "Command palette");
    const field = screen.getByRole("button", { name: "Record shortcut" });
    expect(field).toHaveTextContent("⌘K");
    await userEvent.click(field);
    await userEvent.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘⇧P");
    expect(mocked.setKeybindings).toHaveBeenCalledWith({ "palette.open": "Mod+Shift+P" });
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

  it("shows whether Stage can find databee and manages the API token", async () => {
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Presentations" }));
    expect(await screen.findByText("Discoverable")).toBeInTheDocument();
    expect(screen.getByText("/apps/databee.json")).toBeInTheDocument();
    expect(screen.queryByText("token-123")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "••••••••••••" }));
    expect(screen.getByText("token-123")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "New" }));
    expect(mocked.stageNewToken).toHaveBeenCalled();
    expect(await screen.findByText("token-new")).toBeInTheDocument();

    mocked.stageStatus.mockResolvedValue({ enabled: false, running: false, error: null, url: "http://127.0.0.1:27184/v1/stage", discoveryFile: null });
    await userEvent.click(screen.getByLabelText("Enable the API"));
    expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ stage: { enabled: false, port: 27184, token: "token-new" } }));
    expect(await screen.findByText("Off (needs the local API)")).toBeInTheDocument();
  });

  it("names the port conflict when the API can't start", async () => {
    mocked.stageStatus.mockResolvedValue({ enabled: true, running: false, error: "Port 27184 is already used by another app.", url: "http://127.0.0.1:27184/v1/stage", discoveryFile: null });
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Presentations" }));
    expect(await screen.findByText("Port 27184 is already used by another app.")).toBeInTheDocument();
    expect(screen.getByText("Not running")).toBeInTheDocument();
  });
});
