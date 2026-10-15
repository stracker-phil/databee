import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api } from "../api";
import { resetStore } from "../state/store";
import { settings } from "../test/fixtures";
import { installCommands } from "../commands";
import { SettingsDialog } from "./SettingsDialog";

const mocked = vi.mocked(api, true);

describe("SettingsDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore({ settings: settings(), dialog: { type: "settings" } });
  });

  it("saves edited values", async () => {
    render(<SettingsDialog />);
    const rowLimit = screen.getByLabelText("Row limit");
    await userEvent.clear(rowLimit);
    await userEvent.type(rowLimit, "250");
    await userEvent.click(screen.getByRole("button", { name: "Dark" }));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(mocked.updateSettings).toHaveBeenCalledWith(expect.objectContaining({ rowLimit: 250, theme: "dark" }));
  });

  it("discards changes on cancel", async () => {
    render(<SettingsDialog />);
    await userEvent.click(screen.getByRole("button", { name: "Dark" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(mocked.updateSettings).not.toHaveBeenCalled();
  });

  it("applies the saved theme and accent to the page", async () => {
    mocked.getSettings.mockResolvedValue(settings());
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Dark" }));
    await userEvent.click(screen.getByRole("button", { name: "Accent purple" }));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.dataset.accent).toBe("purple");
  });

  it("records, resets and saves shortcut overrides", async () => {
    installCommands();
    render(<SettingsDialog />);
    await userEvent.type(screen.getByLabelText("Search shortcuts"), "Command palette");
    const field = screen.getByRole("button", { name: "Record shortcut" });
    expect(field).toHaveTextContent("⌘K");
    await userEvent.click(field);
    await userEvent.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘⇧P");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(mocked.setKeybindings).toHaveBeenCalledWith({ "palette.open": "Mod+Shift+P" });
  });

  it("warns when two commands in overlapping scopes share a binding", async () => {
    installCommands();
    render(<SettingsDialog />);
    await userEvent.type(screen.getByLabelText("Search shortcuts"), "Zoom in");
    await userEvent.click(screen.getByRole("button", { name: "Record shortcut" }));
    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(screen.getByRole("alert")).toHaveTextContent(/⌘K is used by both/);
    await userEvent.click(screen.getByRole("button", { name: "Reset Zoom in" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("warns that trace logging records SQL", async () => {
    render(<SettingsDialog />);
    await userEvent.selectOptions(screen.getByLabelText("Log level"), "trace");
    expect(screen.getByText(/include the SQL text/)).toBeInTheDocument();
  });
});
