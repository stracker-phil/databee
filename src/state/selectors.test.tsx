import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { Toolbar } from "../components/Toolbar";
import { connectedStore, session } from "../test/fixtures";
import { selectVisibleDatabases } from "./selectors";
import { useStore } from "./store";

const databaseOptions = () => Array.from(screen.getByRole("combobox", { name: "Database" }).querySelectorAll("option")).map((o) => o.textContent);

describe("selectVisibleDatabases", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("hides system schemas by default", () => {
    render(<Toolbar />);

    expect(databaseOptions()).toEqual(["db"]);
  });

  it("lists system schemas when showSystemSchemas is on", () => {
    connectedStore({ settings: { showSystemSchemas: true } });
    render(<Toolbar />);

    expect(databaseOptions()).toEqual(["db", "mysql"]);
  });

  it("keeps a system schema listed while it is the session's current database", () => {
    useStore.setState({ sessions: [{ ...session, database: "mysql" }] });
    render(<Toolbar />);

    expect(databaseOptions()).toEqual(["db", "mysql"]);
  });

  it("returns nothing without an active session", () => {
    useStore.setState({ activeSessionId: null });

    expect(selectVisibleDatabases(useStore.getState())).toEqual([]);
  });
});
