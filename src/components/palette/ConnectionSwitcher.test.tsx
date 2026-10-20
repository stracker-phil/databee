import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type RecentConnection, type SessionInfo } from "../../api";
import { bindingFor, clearCommands, conflicts, handleKeydown, installCommands } from "../../commands";
import { selectActiveSession, useStore } from "../../state/store";
import { connectedStore, session } from "../../test/fixtures";
import { CommandPalette } from "../CommandPalette";

const mocked = vi.mocked(api, true);

const staging: SessionInfo = {
  ...session,
  id: "s2",
  spec: { name: "Staging", host: "127.0.0.1", port: 33060, user: "root", database: "stage" },
  identity: "root@127.0.0.1:33060/stage",
  displayName: "Staging",
  database: "stage",
  slug: "staging",
};

const recent = (name: string, port: number, database: string, slug: string): RecentConnection => ({
  identity: `root@127.0.0.1:${port}/${database}`,
  account: `root@127.0.0.1:${port}`,
  spec: { name, host: "127.0.0.1", port, user: "root", database },
  lastUsed: "2026-10-17T10:00:00Z",
  slug,
});

const blog = recent("Blog", 41000, "blog", "blog");

const option = (title: string) => screen.getAllByRole("option").find((o) => o.getAttribute("data-stage-label") === title);
const titles = () => screen.getAllByRole("option").map((o) => o.getAttribute("data-stage-label"));

async function openSwitcher() {
  render(<CommandPalette />);
  await userEvent.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
  return screen.getByLabelText("Search everything");
}

describe("Switch connection (⌘⇧P)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Element.prototype.scrollIntoView = vi.fn();
    connectedStore();
    useStore.setState((s) => ({
      sessions: [...s.sessions, staging],
      schema: { ...s.schema, [session.id]: { ...s.schema[session.id], databases: [{ name: "db", isSystem: false }, { name: "shop2", isSystem: false }, { name: "mysql", isSystem: true }] } },
    }));
    mocked.listRecents.mockResolvedValue([recent("Shop", 52958, "db", "shop"), blog]);
    installCommands();
    window.addEventListener("keydown", handleKeydown, true);
  });

  afterEach(() => {
    window.removeEventListener("keydown", handleKeydown, true);
    delete (Element.prototype as Partial<Element>).scrollIntoView;
    clearCommands();
  });

  it("is bound to ⌘⇧P without a conflict", () => {
    expect(bindingFor("connection.switch")).toBe("Mod+Shift+P");
    expect(conflicts().filter((c) => c.binding === "Mod+Shift+P")).toEqual([]);
  });

  it("opens the palette with c: and lists open sessions and recents once each", async () => {
    const input = await openSwitcher();
    expect(input).toHaveValue("c:");
    await waitFor(() => expect(option("Blog")).toBeDefined());
    expect(screen.getByText("Connections")).toBeInTheDocument();
    expect(titles().filter((t) => t === "Shop")).toHaveLength(1);
    expect(option("Blog")).toHaveTextContent("127.0.0.1:41000 / blog");
    expect(option("Blog")).toHaveTextContent("blog");
  });

  it("marks the active connection current and other open ones open", async () => {
    await openSwitcher();
    await waitFor(() => expect(option("Blog")).toBeDefined());
    expect(option("Shop")).toHaveTextContent("current");
    expect(option("Staging")).toHaveTextContent("open");
    expect(option("Blog")).not.toHaveTextContent("current");
  });

  it("adds connection lost to the badge of every lost open connection", async () => {
    const down = { state: "down", since: 1_700_000_000_000, error: "Connection refused", retryAt: 1_700_000_030_000 } as const;
    useStore.setState({ health: { s1: down, s2: { state: "reconnecting", since: down.since, error: down.error } } });
    await openSwitcher();
    await waitFor(() => expect(option("Blog")).toBeDefined());
    expect(option("Shop")).toHaveTextContent("current · connection lost");
    expect(option("Staging")).toHaveTextContent("open · connection lost");
    expect(option("Blog")).not.toHaveTextContent("connection lost");
  });

  it("filters connections fuzzily by name, host:port, database and slug", async () => {
    const input = await openSwitcher();
    await waitFor(() => expect(option("Blog")).toBeDefined());
    await userEvent.type(input, "stgng");
    expect(titles()).toEqual(["Staging"]);
    await userEvent.clear(input);
    await userEvent.type(input, "c:41000");
    expect(titles()).toEqual(["Blog"]);
  });

  it("connects to a recent connection on Enter", async () => {
    const input = await openSwitcher();
    await waitFor(() => expect(option("Blog")).toBeDefined());
    await userEvent.type(input, "blog{Enter}");
    expect(mocked.connect).toHaveBeenCalledWith({ spec: blog.spec });
    expect(useStore.getState().paletteOpen).toBe(false);
  });

  it("asks for the password when the recent connection needs one", async () => {
    mocked.connect.mockRejectedValueOnce({ kind: "password_required", message: "Password required", code: null, account: blog.account });
    const input = await openSwitcher();
    await waitFor(() => expect(option("Blog")).toBeDefined());
    await userEvent.type(input, "blog{Enter}");
    await waitFor(() => expect(useStore.getState().dialog?.type).toBe("password"));
  });

  it("activates an open session on Enter instead of connecting again", async () => {
    const input = await openSwitcher();
    await userEvent.type(input, "staging{Enter}");
    expect(useStore.getState().activeSessionId).toBe("s2");
    expect(mocked.connect).not.toHaveBeenCalled();
  });

  it("lists the active connection's databases, marks the current one and switches on Enter", async () => {
    const input = await openSwitcher();
    expect(screen.getByText("Databases")).toBeInTheDocument();
    expect(option("db")).toHaveTextContent("current");
    expect(option("mysql")).toBeUndefined();
    await userEvent.type(input, "shop2{Enter}");
    expect(mocked.useDatabase).toHaveBeenCalledWith(session.id, "shop2");
    await waitFor(() => expect(selectActiveSession(useStore.getState())?.database).toBe("shop2"));
  });

  describe("database accents", () => {
    const dot = (title: string) => option(title)?.querySelector("[data-accent]");

    beforeEach(() => {
      useStore.setState({ accents: { "shop/db": "purple", "staging/stage": "gold", "shop/shop2": "blue" } });
    });

    it("shows a dot on connections whose current database has its own accent", async () => {
      await openSwitcher();
      await waitFor(() => expect(option("Blog")).toBeDefined());
      expect(dot("Shop")).toHaveAttribute("data-accent", "purple");
      expect(dot("Staging")).toHaveAttribute("data-accent", "gold");
      expect(dot("Blog")).not.toBeInTheDocument();
    });

    it("shows a dot on the active connection's databases that have their own accent", async () => {
      await openSwitcher();
      expect(dot("db")).toHaveAttribute("data-accent", "purple");
      expect(dot("shop2")).toHaveAttribute("data-accent", "blue");
    });

    it("shows no dot without accents", async () => {
      useStore.setState({ accents: {} });
      await openSwitcher();
      await waitFor(() => expect(option("Blog")).toBeDefined());
      expect(dot("Shop")).not.toBeInTheDocument();
      expect(dot("db")).not.toBeInTheDocument();
    });
  });

  it("reopens in switch mode when run from the palette's commands", async () => {
    render(<CommandPalette />);
    useStore.getState().setPaletteOpen(true);
    await userEvent.type(await screen.findByLabelText("Search everything"), ">Switch connection{Enter}");
    expect(await screen.findByLabelText("Search everything")).toHaveValue("c:");
  });
});
