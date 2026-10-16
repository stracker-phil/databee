import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));
vi.mock("./api", async () => ({ stageApi: (await import("./apiMock")).createStageApiMock() }));

import { resetStore } from "../state/store";
import { settings } from "../test/settings";
import { stageApi } from "./api";
import { PresentationsPage } from "./PresentationsPage";

const mocked = vi.mocked(stageApi, true);

describe("PresentationsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore({ settings: settings() });
  });

  it("shows the hook's status and the command that registers it with Stage", async () => {
    const command = "stage register http://127.0.0.1:27184/v1/stage --token token-123";
    mocked.status.mockResolvedValue({ enabled: true, running: true, error: null, url: "http://127.0.0.1:27184/v1/stage", registerCommand: command, lastRequest: new Date(Date.now() - 120_000).toISOString() });
    render(<PresentationsPage />);
    expect(await screen.findByText("Ready")).toBeInTheDocument();
    expect(screen.getByText(command)).toBeInTheDocument();
    expect(screen.getByText("2 min ago")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(command);

    expect(screen.queryByText("token-123")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "••••••••••••" }));
    expect(screen.getByText("token-123")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "New token" }));
    expect(await screen.findByText("token-new")).toBeInTheDocument();

    mocked.status.mockResolvedValue({ enabled: false, running: false, error: null, url: "http://127.0.0.1:27184/v1/stage", registerCommand: command, lastRequest: null });
    await userEvent.click(screen.getByLabelText("Allow presentations"));
    expect(mocked.updateSettings).toHaveBeenLastCalledWith({ enabled: false, port: 27184, token: "token-new" });
    expect(await screen.findByText("Off")).toBeInTheDocument();
    expect(screen.getByText("None yet")).toBeInTheDocument();
  });

  it("copies the token", async () => {
    render(<PresentationsPage />);
    await userEvent.click(await screen.findByRole("button", { name: "Copy token" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("token-123");
  });

  it("saves a changed port through the Stage settings", async () => {
    render(<PresentationsPage />);
    const port = await screen.findByLabelText("Port");
    await userEvent.clear(port);
    await userEvent.type(port, "30000");
    await userEvent.tab();
    expect(mocked.updateSettings).toHaveBeenLastCalledWith({ enabled: true, port: 30000, token: "token-123" });
  });

  it("names the port conflict when the API can't start", async () => {
    mocked.status.mockResolvedValue({ enabled: true, running: false, error: "Port 27184 is already used by another app.", url: "http://127.0.0.1:27184/v1/stage", registerCommand: "stage register http://127.0.0.1:27184/v1/stage --token t", lastRequest: null });
    render(<PresentationsPage />);
    expect(await screen.findByText("Port 27184 is already used by another app.")).toBeInTheDocument();
  });
});
