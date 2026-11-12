import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, LOG_LEVELS, type AppStatus, type ConnectionStatus } from "../../api";
import { resetStore } from "../../state/store";
import { settings } from "../../test/settings";
import { capitalize } from "./controls";
import { SettingsWindow } from "./SettingsWindow";

const mocked = vi.mocked(api, true);

const status = (patch: Partial<AppStatus> = {}): AppStatus => ({
  version: "1.2.0",
  build: "debug",
  dataFolder: "~/Library/Application Support/com.example.databee",
  logFolder: "~/Library/Logs/com.example.databee",
  logLevel: "info",
  logLevelFromCli: false,
  connections: [],
  ...patch,
});

const connection = (patch: Partial<ConnectionStatus> = {}): ConnectionStatus => ({
  slug: "shop",
  account: "root@db.example.com",
  database: "wp",
  serverVersion: "11.4.2-MariaDB",
  health: { state: "ok" },
  ...patch,
});

const renderPage = (id: "status" | "about") => render(<SettingsWindow request={{ id }} onOpen={() => {}} />);

describe("Status page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore({ settings: settings() });
    mocked.appStatus.mockResolvedValue(status());
  });

  describe("build", () => {
    it("shows the version and the kind of build from the app status", async () => {
      renderPage("status");
      expect(await screen.findByText("1.2.0")).toBeInTheDocument();
      expect(screen.getByText("Debug")).toBeInTheDocument();
      expect(screen.getByText("The separate debug app, on demo data.")).toBeInTheDocument();
    });

    it("shows a release build without the debug note", async () => {
      mocked.appStatus.mockResolvedValue(status({ build: "release" }));
      renderPage("status");
      expect(await screen.findByText("Release")).toBeInTheDocument();
      expect(screen.queryByText("The separate debug app, on demo data.")).not.toBeInTheDocument();
    });
  });

  describe("open connections", () => {
    it("says so when no connection is open", async () => {
      renderPage("status");
      expect(await screen.findByText("No open connections.")).toBeInTheDocument();
    });

    it("lists each connection with its server and health", async () => {
      mocked.appStatus.mockResolvedValue(
        status({
          connections: [
            connection(),
            connection({ slug: "blog", database: null, account: "root@127.0.0.1", health: { state: "down", since: 0, error: "connection refused", retryAt: 0 } }),
            connection({ slug: "cart", database: "wp", account: "app@db.example.com", health: { state: "reconnecting", since: 0, error: "timeout" } }),
          ],
        }),
      );
      renderPage("status");
      const group = within(await screen.findByRole("region", { name: "Open connections" }));
      expect(group.getByText("shop · wp")).toBeInTheDocument();
      expect(group.getByText("root@db.example.com · 11.4.2-MariaDB")).toBeInTheDocument();
      expect(group.getByText("Connected")).toBeInTheDocument();
      expect(group.getByText("blog")).toBeInTheDocument();
      expect(group.getByText("Down: connection refused")).toBeInTheDocument();
      expect(group.getByText("Reconnecting…")).toBeInTheDocument();
      expect(group.queryByText("No open connections.")).not.toBeInTheDocument();
    });

    it("reads the status again on Refresh", async () => {
      renderPage("status");
      await screen.findByText("No open connections.");
      mocked.appStatus.mockResolvedValue(status({ connections: [connection()] }));
      await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
      expect(await screen.findByText("Connected")).toBeInTheDocument();
      expect(screen.queryByText("No open connections.")).not.toBeInTheDocument();
    });
  });

  describe("log level", () => {
    const openMenu = async () => {
      await userEvent.click(screen.getByLabelText("Log level"));
      return screen.getByRole("menu", { name: "Log level" });
    };

    it("shows the saved level on the pop-up button", () => {
      resetStore({ settings: settings({ logLevel: "warn" }) });
      renderPage("status");
      expect(screen.getByLabelText("Log level")).toHaveTextContent("Warn");
    });

    it("lists every level with the saved one checked", async () => {
      renderPage("status");
      const items = within(await openMenu()).getAllByRole("menuitemradio");
      expect(items.map((i) => i.textContent)).toEqual(LOG_LEVELS.map(capitalize));
      expect(items.filter((i) => i.getAttribute("aria-checked") === "true").map((i) => i.textContent)).toEqual(["Info"]);
    });

    it("saves the chosen level in the settings", async () => {
      renderPage("status");
      await userEvent.click(within(await openMenu()).getByRole("menuitemradio", { name: "Debug" }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ logLevel: "debug" }));
    });

    it("warns that trace logging records SQL", () => {
      resetStore({ settings: settings({ logLevel: "trace" }) });
      renderPage("status");
      expect(screen.getByText(/Trace logs include the SQL text of every query/)).toBeInTheDocument();
    });

    it("does not give the trace warning for another level", () => {
      renderPage("status");
      expect(screen.queryByText(/include the SQL text/)).not.toBeInTheDocument();
    });

    it("notes the level in effect when --log-level sets it for this run", async () => {
      mocked.appStatus.mockResolvedValue(status({ logLevel: "debug", logLevelFromCli: true }));
      renderPage("status");
      expect(await screen.findByText(/This run uses debug from --log-level\./)).toBeInTheDocument();
    });

    it("leaves out the --log-level note otherwise", async () => {
      renderPage("status");
      await screen.findByText("1.2.0");
      expect(screen.queryByText(/--log-level/)).not.toBeInTheDocument();
    });
  });
});

describe("About page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore({ settings: settings() });
    mocked.appStatus.mockResolvedValue(status());
  });

  it("shows the version, the folders and the license", async () => {
    renderPage("about");
    expect(await screen.findByText("1.2.0")).toBeInTheDocument();
    expect(screen.getByText("~/Library/Application Support/com.example.databee")).toBeInTheDocument();
    expect(screen.getByText("~/Library/Logs/com.example.databee")).toBeInTheDocument();
    expect(screen.getByText("MIT")).toBeInTheDocument();
  });

  it.each([
    ["Show Data Folder", "help.showDataFolder"],
    ["Open Log Folder", "help.openLogFolder"],
  ])("runs %s as the command %s", async (button, command) => {
    renderPage("about");
    await userEvent.click(screen.getByRole("button", { name: button }));
    expect(mocked.runCommand).toHaveBeenCalledWith(command);
  });

  it("opens the source code in the browser", async () => {
    renderPage("about");
    await userEvent.click(screen.getByRole("button", { name: "Open in Browser" }));
    expect(mocked.openUrl).toHaveBeenCalledWith("https://github.com/stracker-phil/databee");
  });
});
