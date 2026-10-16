import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { installCommands } from "../commands";
import { useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { StatusBar } from "./StatusBar";

const mocked = vi.mocked(api, true);

describe("StatusBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    installCommands();
  });

  it("cycles the theme from its chip", async () => {
    render(<StatusBar />);
    await userEvent.click(screen.getByRole("button", { name: "Theme: system" }));
    expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ theme: "light" }));
    await userEvent.click(await screen.findByRole("button", { name: "Theme: light" }));
    expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ theme: "dark" }));
  });

  it("moves the inspector from its chip", async () => {
    render(<StatusBar />);
    await userEvent.click(screen.getByRole("button", { name: "Inspector: right" }));
    await vi.waitFor(() => expect(useStore.getState().settings?.inspectorDock).toBe("bottom"));
    expect(screen.getByRole("button", { name: "Inspector: bottom" })).toBeInTheDocument();
  });

  it("flags pending changes", () => {
    useStore.setState({ pending: [{ id: 1, sessionId: "s1" } as never] });
    render(<StatusBar />);
    expect(screen.getByRole("button", { name: "1 pending change" })).toBeInTheDocument();
  });

  it("counts only the active session's pending changes", () => {
    useStore.setState({
      pending: [
        { id: 1, sessionId: "s1" },
        { id: 2, sessionId: "s2" },
        { id: 3, sessionId: "s2" },
      ] as never,
    });
    render(<StatusBar />);
    expect(screen.getByRole("button", { name: "1 pending change" })).toBeInTheDocument();
  });

  it("shows no pending chip when only another session has changes", () => {
    useStore.setState({ pending: [{ id: 1, sessionId: "s2" }] as never });
    render(<StatusBar />);
    expect(screen.queryByRole("button", { name: /pending/ })).not.toBeInTheDocument();
  });
});
