import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api, type RemoteEvent, type RemoteRequest } from "../api";
import { connectedStore } from "../test/fixtures";
import { objectDocument } from "../test/objectFixtures";

const mocked = vi.mocked(api, true);

/** Renders the app and returns a sender that delivers a request through the handler `onRemote` received. */
async function remote() {
  render(<App />);
  await screen.findByText("wp_options");
  await waitFor(() => expect(mocked.onRemote).toHaveBeenCalled());
  const handler = mocked.onRemote.mock.calls[0][0];
  return (id: string, request: RemoteRequest) => handler({ id, request } satisfies RemoteEvent);
}

const selectedTab = () => screen.getByRole("tab", { selected: true });

describe("remote requests", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("opens an address and replies with success", async () => {
    const send = await remote();
    send("r1", { type: "open", address: "databee://shop/db/wp_wc_orders/4711", connection: null });
    await waitFor(() => expect(mocked.remoteReply).toHaveBeenCalledWith("r1", null, null));
    expect(await screen.findByRole("tab", { name: /wp_wc_orders #4711/, selected: true })).toBeInTheDocument();
  });

  it("replies with an error when the address names a connection that is not open", async () => {
    const send = await remote();
    send("r2", { type: "open", address: "databee://ghost/db/wp_posts", connection: null });
    await waitFor(() => expect(mocked.remoteReply).toHaveBeenCalledWith("r2", null, "No open connection is named ghost"));
    expect(screen.getAllByRole("tab")).toHaveLength(1);
  });

  it("opens the row a shorthand names in the active connection", async () => {
    mocked.document.mockResolvedValue(objectDocument());
    const send = await remote();
    send("r3", { type: "open", address: "order 4711", connection: null });
    await waitFor(() => expect(mocked.remoteReply).toHaveBeenCalledWith("r3", null, null));
    expect(mocked.document).toHaveBeenCalledWith("s1", "order 4711", null);
    expect(selectedTab()).toHaveAccessibleName(/order #4711/);
  });

  it("replies with an error when a shorthand names a connection that is not open", async () => {
    const send = await remote();
    send("r4", { type: "open", address: "order 4711", connection: "ghost" });
    await waitFor(() => expect(mocked.remoteReply).toHaveBeenCalledWith("r4", null, "No open connection is named ghost"));
    expect(mocked.document).not.toHaveBeenCalled();
  });

  it("replies with the error when Rust cannot resolve the shorthand", async () => {
    mocked.document.mockRejectedValue({ kind: "io", message: "No order 9", code: null });
    const send = await remote();
    send("r5", { type: "open", address: "order 9", connection: null });
    await waitFor(() => expect(mocked.remoteReply).toHaveBeenCalledWith("r5", null, "No order 9"));
  });

  it("runs a command and replies with success", async () => {
    const send = await remote();
    send("r6", { type: "command", id: "tabs.new" });
    await waitFor(() => expect(mocked.remoteReply).toHaveBeenCalledWith("r6", null, null));
    expect(screen.getAllByRole("tab")).toHaveLength(2);
  });

  it("replies with an error for an unknown command id", async () => {
    const send = await remote();
    send("r7", { type: "command", id: "no.such.command" });
    await waitFor(() => expect(mocked.remoteReply).toHaveBeenCalledWith("r7", null, "No command no.such.command"));
  });

  it("replies with the formatted address of the active tab", async () => {
    const send = await remote();
    send("r8", { type: "current" });
    await waitFor(() => expect(mocked.remoteReply).toHaveBeenCalledWith("r8", "databee://shop/db/", null));
  });
});
