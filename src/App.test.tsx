import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", async (orig) => ({ ...(await orig<object>()), api: (await import("./test/apiMock")).createApiMock() }));

import App from "./App";
import { installCommands, runCommand } from "./commands";
import { resetStore, useStore } from "./state/store";
import { connectedStore } from "./test/fixtures";

describe("App", () => {
  beforeEach(() => resetStore());

  it("shows the connect screen when nothing is connected", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "New connection" })).toBeInTheDocument();
  });

  it("renders the workspace for a connected session", async () => {
    connectedStore();
    render(<App />);
    expect(screen.getByText("Shop")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Query 1/ })).toBeInTheDocument();
    expect(await screen.findByText("wp_options")).toBeInTheDocument();
    expect(screen.getByText("10.3.39-MariaDB")).toBeInTheDocument();
  });

  it("runs menu commands by id", () => {
    connectedStore();
    installCommands();
    runCommand("tabs.new");
    expect(useStore.getState().tabs).toHaveLength(2);
    runCommand("tabs.close");
    expect(useStore.getState().tabs).toHaveLength(1);
    runCommand("settings.open");
    expect(useStore.getState().dialog).toEqual({ type: "settings" });
  });

  it("resolves shortcuts through the registry in both views", async () => {
    connectedStore();
    render(<App />);
    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(useStore.getState().paletteOpen).toBe(true);
    await userEvent.keyboard("{Escape}");
    await userEvent.keyboard("{Meta>}t{/Meta}");
    expect(useStore.getState().tabs).toHaveLength(2);
    await userEvent.keyboard("{Control>}{Tab}{/Control}");
    expect(useStore.getState().activeTabId).toBe(useStore.getState().tabs[0].id);
    await userEvent.keyboard("{Meta>}={/Meta}");
    expect(useStore.getState().settings?.uiFontSize).toBe(14);
    expect(useStore.getState().settings?.dataFontSize).toBe(13);
  });
});
