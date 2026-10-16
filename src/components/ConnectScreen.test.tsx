import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { resetStore, useStore } from "../state/store";
import { session, settings } from "../test/fixtures";
import { ConnectScreen } from "./ConnectScreen";
import { Dialogs } from "./Dialogs";
import { Toasts } from "./Toasts";

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
    mocked.connect.mockRejectedValueOnce({ kind: "password_required", message: "needed", code: null, account: "wp@db.test:3307" });
    render(
      <>
        <ConnectScreen />
        <Dialogs />
      </>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Password for wp@db.test:3307");

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
    mocked.listRecents.mockResolvedValue([
      { identity: session.identity, account: session.account, spec: session.spec, lastUsed: new Date().toISOString() },
    ]);
    mocked.connect.mockResolvedValue(session);
    render(<ConnectScreen />);
    const recent = await screen.findByRole("button", { name: "Connect to Shop" });
    await userEvent.click(recent);
    expect(screen.getByLabelText("Port")).toHaveValue("52958");
    await userEvent.dblClick(recent);
    expect(mocked.connect).toHaveBeenCalledWith(expect.objectContaining({ spec: session.spec }));
  });

  it("describes a recent connection by the account Rust sent", async () => {
    mocked.listRecents.mockResolvedValue([
      { identity: session.identity, account: "wp@db.test:3307", spec: session.spec, lastUsed: new Date().toISOString() },
    ]);
    render(<ConnectScreen />);
    const recent = await screen.findByRole("button", { name: "Connect to Shop" });
    expect(recent).toHaveTextContent("wp@db.test:3307 / db");
  });

  it("loads favorites for the account of the connected session", async () => {
    mocked.connect.mockResolvedValue({ ...session, account: "wp@db.test:3307" });
    mocked.listFavorites.mockResolvedValue([]);
    render(<ConnectScreen />);
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    await vi.waitFor(() => expect(mocked.listFavorites).toHaveBeenCalledWith("wp@db.test:3307"));
  });

  it("shows the configured default port in the Port field", () => {
    resetStore({ settings: settings({ defaultPort: 3307 }) });
    render(<ConnectScreen />);
    expect(screen.getByLabelText("Port")).toHaveValue("3307");
  });

  describe("removing a recent connection", () => {
    const recent = { identity: session.identity, account: session.account, spec: session.spec, lastUsed: new Date().toISOString() };

    beforeEach(() => mocked.listRecents.mockResolvedValue([recent]));

    it("drops it from the list after the backend forgot it", async () => {
      render(<ConnectScreen />);
      await userEvent.click(await screen.findByRole("button", { name: `Remove ${session.identity}` }));
      expect(mocked.removeRecent).toHaveBeenCalledWith(session.identity);
      expect(screen.queryByRole("button", { name: "Connect to Shop" })).not.toBeInTheDocument();
    });

    it("keeps it and shows an error toast when the backend fails", async () => {
      mocked.removeRecent.mockRejectedValueOnce({ kind: "io", message: "disk full", code: null });
      render(
        <>
          <ConnectScreen />
          <Toasts />
        </>,
      );
      await userEvent.click(await screen.findByRole("button", { name: `Remove ${session.identity}` }));
      expect(await screen.findByRole("alert")).toHaveTextContent("Could not remove the recent connection: disk full");
      expect(screen.getByRole("button", { name: "Connect to Shop" })).toBeInTheDocument();
    });
  });
});
