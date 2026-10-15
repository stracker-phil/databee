import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { resetStore, useStore } from "../state/store";
import { session, settings } from "../test/fixtures";
import { ConnectScreen } from "./ConnectScreen";
import { Dialogs } from "./Dialogs";

const mocked = vi.mocked(api, true);

describe("ConnectScreen", () => {
  beforeEach(() => {
    resetStore({ settings: settings() });
    vi.clearAllMocks();
  });

  it("rejects an invalid port without connecting", async () => {
    render(<ConnectScreen />);
    const port = screen.getByLabelText("Port");
    await userEvent.clear(port);
    await userEvent.type(port, "99999");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Port must be between 1 and 65535");
    expect(mocked.connect).not.toHaveBeenCalled();
  });

  it("connects with the entered details", async () => {
    mocked.connect.mockResolvedValue(session);
    render(<ConnectScreen />);
    const port = screen.getByLabelText("Port");
    await userEvent.clear(port);
    await userEvent.type(port, "52958");
    await userEvent.type(screen.getByLabelText("Password"), "root");
    await userEvent.type(screen.getByLabelText("Database (optional)"), "db");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));

    expect(mocked.connect).toHaveBeenCalledWith({
      spec: { name: null, host: "127.0.0.1", port: 52958, socket: null, user: "root", database: "db" },
      password: "root",
      savePassword: true,
    });
    expect(useStore.getState().sessions).toEqual([session]);
  });

  it("asks for a password when the server requires one", async () => {
    mocked.connect.mockRejectedValueOnce({ kind: "password_required", message: "needed", code: null });
    render(
      <>
        <ConnectScreen />
        <Dialogs />
      </>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Password for root@127.0.0.1:3306");

    mocked.connect.mockResolvedValue(session);
    await userEvent.type(within(dialog).getByLabelText("Password"), "secret");
    await userEvent.click(within(dialog).getByRole("button", { name: "Connect" }));
    expect(mocked.connect).toHaveBeenLastCalledWith(expect.objectContaining({ password: "secret", savePassword: true }));
  });

  it("shows connection errors inline", async () => {
    mocked.connect.mockRejectedValue({ kind: "connection", message: "Could not connect: refused", code: null });
    render(<ConnectScreen />);
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not connect: refused");
  });

  it("fills the form from a recent connection and connects on double-click", async () => {
    mocked.listRecents.mockResolvedValue([{ identity: session.identity, spec: session.spec, lastUsed: new Date().toISOString() }]);
    mocked.connect.mockResolvedValue(session);
    render(<ConnectScreen />);
    const recent = await screen.findByRole("button", { name: "Connect to Shop" });
    await userEvent.click(recent);
    expect(screen.getByLabelText("Port")).toHaveValue("52958");
    await userEvent.dblClick(recent);
    expect(mocked.connect).toHaveBeenCalledWith(expect.objectContaining({ spec: session.spec }));
  });
});
