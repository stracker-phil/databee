import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { Toolbar } from "../components/Toolbar";
import { connectedStore, session } from "../test/fixtures";
import { selectVisibleDatabases } from "./selectors";
import { useStore } from "./store";

const databaseOptions = async () => {
  await userEvent.click(screen.getByRole("button", { name: "Database" }));
  return screen.getAllByRole("menuitemradio").map((o) => o.textContent);
};

describe("selectVisibleDatabases", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("hides system schemas by default", async () => {
    render(<Toolbar />);

    expect(await databaseOptions()).toEqual(["db"]);
  });

  it("lists system schemas when showSystemSchemas is on", async () => {
    connectedStore({ settings: { showSystemSchemas: true } });
    render(<Toolbar />);

    expect(await databaseOptions()).toEqual(["db", "mysql"]);
  });

  it("keeps a system schema listed while it is the session's current database", async () => {
    useStore.setState({ sessions: [{ ...session, database: "mysql" }] });
    render(<Toolbar />);

    expect(await databaseOptions()).toEqual(["db", "mysql"]);
  });

  it("returns nothing without an active session", () => {
    useStore.setState({ activeSessionId: null });

    expect(selectVisibleDatabases(useStore.getState())).toEqual([]);
  });
});
