import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type RecentConnection } from "../../api";
import { useStore } from "../../state/store";
import { connectedStore, session } from "../../test/fixtures";
import { Toasts } from "../Toasts";
import { ConnectionSlugs } from "./ConnectionSlugs";

const mocked = vi.mocked(api, true);

const recent = (patch: Partial<RecentConnection> = {}): RecentConnection => ({
  identity: session.identity,
  account: session.account,
  spec: session.spec,
  lastUsed: new Date().toISOString(),
  slug: session.slug,
  ...patch,
});

const blog = recent({ identity: "root@127.0.0.1:52959/db", spec: { ...session.spec, name: "Blog", port: 52959 }, slug: "blog" });

describe("ConnectionSlugs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    mocked.listRecents.mockResolvedValue([recent(), blog]);
  });

  it("lists every saved connection with its slug", async () => {
    render(<ConnectionSlugs />);
    expect(await screen.findByLabelText("Shop")).toHaveValue("shop");
    expect(screen.getByLabelText("Blog")).toHaveValue("blog");
  });

  it("shows nothing without saved connections", async () => {
    mocked.listRecents.mockResolvedValue([]);
    render(<ConnectionSlugs />);
    await vi.waitFor(() => expect(mocked.listRecents).toHaveBeenCalled());
    expect(screen.queryByRole("heading", { name: "Addresses" })).not.toBeInTheDocument();
  });

  it("renames the connection when the slug is committed", async () => {
    render(<ConnectionSlugs />);
    const field = await screen.findByLabelText("Shop");
    await userEvent.clear(field);
    await userEvent.type(field, " store {Enter}");
    expect(mocked.renameConnectionSlug).toHaveBeenCalledWith(session.identity, "store");
    expect(useStore.getState().sessions[0].slug).toBe("store");
  });

  it("shows the stored slug again and reports the error when the rename is rejected", async () => {
    mocked.renameConnectionSlug.mockRejectedValueOnce({ kind: "invalid", message: "Slug is taken", code: null, account: null });
    render(
      <>
        <ConnectionSlugs />
        <Toasts />
      </>,
    );
    const field = await screen.findByLabelText("Shop");
    await userEvent.clear(field);
    await userEvent.type(field, "blog{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not rename the connection: Slug is taken");
    expect(screen.getByLabelText("Shop")).toHaveValue("shop");
    expect(useStore.getState().sessions[0].slug).toBe("shop");
  });
});
