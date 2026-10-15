import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", async (orig) => ({ ...(await orig<object>()), api: (await import("./test/apiMock")).createApiMock() }));

import App, { runMenuAction } from "./App";
import { resetStore, useStore } from "./state/store";
import { connectedStore } from "./test/fixtures";

describe("App", () => {
  beforeEach(() => resetStore());

  it("shows the connect screen when nothing is connected", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "New connection" })).toBeInTheDocument();
  });

  it("renders the workspace for a connected session", async () => {
    connectedStore();
    render(<App />);
    expect(screen.getByText("Shop")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Query 1/ })).toBeInTheDocument();
    expect(await screen.findByText("wp_options")).toBeInTheDocument();
    expect(screen.getByText("10.3.39-MariaDB")).toBeInTheDocument();
  });

  it("maps menu commands to tab actions", () => {
    connectedStore();
    runMenuAction("new_query");
    expect(useStore.getState().tabs).toHaveLength(2);
    runMenuAction("close_tab");
    expect(useStore.getState().tabs).toHaveLength(1);
    runMenuAction("settings");
    expect(useStore.getState().dialog).toEqual({ type: "settings" });
  });
});
