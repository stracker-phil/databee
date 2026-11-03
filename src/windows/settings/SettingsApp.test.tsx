import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { installCommands } from "../../commands";
import { resetStore } from "../../state/store";
import { settings } from "../../test/settings";
import { SettingsApp } from "./SettingsApp";

const mocked = vi.mocked(api, true);

const lastHandler = <T,>(on: { mock: { calls: unknown[][] } }) => on.mock.calls.at(-1)![0] as T;

async function renderApp(query = "") {
  window.history.replaceState({}, "", `/${query}`);
  render(<SettingsApp />);
  await screen.findByRole("navigation", { name: "Settings pages" });
}

describe("SettingsApp", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
    mocked.getSettings.mockResolvedValue(settings());
  });

  afterEach(() => window.history.replaceState({}, "", "/"));

  describe("pages", () => {
    it("opens on the first page without a ?page=", async () => {
      await renderApp();
      expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
    });

    it("opens on the page named by ?page=", async () => {
      await renderApp("?page=logging");
      expect(screen.getByRole("heading", { name: "Logging" })).toBeInTheDocument();
    });

    it("switches page when Rust sends open_settings", async () => {
      await renderApp();
      act(() => lastHandler<(page: string) => void>(mocked.onOpenSettings)("queries"));
      expect(screen.getByRole("heading", { name: "Queries" })).toBeInTheDocument();
    });

    it("switches page when the user picks one in the nav", async () => {
      await renderApp();
      await userEvent.click(screen.getByRole("button", { name: "Display" }));
      expect(screen.getByRole("heading", { name: "Display" })).toBeInTheDocument();
    });
  });

  describe("settings_changed", () => {
    it("reloads the settings and shows what another window saved", async () => {
      await renderApp("?page=display");
      expect(screen.getByLabelText("NULL label")).not.toHaveValue("∅");
      mocked.getSettings.mockResolvedValue(settings({ nullLabel: "∅" }));
      act(() => lastHandler<() => void>(mocked.onSettingsChanged)());
      await waitFor(() => expect(screen.getByLabelText("NULL label")).toHaveValue("∅"));
    });
  });

  describe("appearance", () => {
    it.each([
      ["dark", "ayu-mirage"],
      ["light", "ayu-light"],
    ] as const)("applies the %s theme of the saved settings to the page", async (theme, scheme) => {
      mocked.getSettings.mockResolvedValue(settings({ theme }));
      await renderApp();
      await waitFor(() => expect(document.documentElement.dataset.scheme).toBe(scheme));
    });

    it("applies an accent saved in the window", async () => {
      await renderApp();
      await userEvent.click(screen.getByRole("button", { name: "Accent gold" }));
      await waitFor(() => expect(document.documentElement.dataset.accent).toBe("gold"));
    });
  });

  describe("keyboard", () => {
    it("hides the window on Escape outside a field", async () => {
      await renderApp();
      await userEvent.keyboard("{Escape}");
      expect(mocked.hideSettings).toHaveBeenCalled();
    });

    it("keeps the window on Escape inside the search field", async () => {
      await renderApp();
      await userEvent.click(screen.getByLabelText("Search Settings"));
      await userEvent.keyboard("{Escape}");
      expect(mocked.hideSettings).not.toHaveBeenCalled();
    });

    it("hides the window on ⌘W, also inside the search field", async () => {
      await renderApp();
      await userEvent.click(screen.getByLabelText("Search Settings"));
      await userEvent.keyboard("{Meta>}w{/Meta}");
      expect(mocked.hideSettings).toHaveBeenCalled();
    });

    it("keeps the window when Escape cancels a shortcut recording", async () => {
      installCommands();
      await renderApp("?page=shortcuts");
      await userEvent.type(screen.getByLabelText("Search shortcuts"), "Command palette");
      await userEvent.click(screen.getByRole("button", { name: "Record shortcut" }));
      await userEvent.keyboard("{Escape}");
      expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘K");
      expect(mocked.hideSettings).not.toHaveBeenCalled();
    });

    it("focuses the search field on ⌘F", async () => {
      await renderApp();
      expect(screen.getByLabelText("Search Settings")).not.toHaveFocus();
      await userEvent.keyboard("{Meta>}f{/Meta}");
      expect(screen.getByLabelText("Search Settings")).toHaveFocus();
    });
  });
});
