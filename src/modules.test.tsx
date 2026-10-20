import { act, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", async (orig) => ({ ...(await orig<object>()), api: (await import("./test/apiMock")).createApiMock() }));
vi.mock("./stage/api", async () => ({ stageApi: (await import("./stage/apiMock")).createStageApiMock() }));

import App from "./App";
import { runCommand } from "./commands";
import { installModules } from "./modules";
import { resetStore } from "./state/store";
import { connectedStore } from "./test/fixtures";

describe("installModules", () => {
  beforeEach(() => {
    resetStore();
    installModules();
  });

  it("makes Settings list the core pages and the Presentations page", async () => {
    connectedStore();
    render(<App />);
    await screen.findByText("wp_options");
    await act(() => runCommand("settings.open"));

    const nav = await screen.findByRole("navigation", { name: "Settings pages" });
    for (const label of ["Appearance", "Shortcuts", "Queries", "Logging", "Presentations"]) {
      expect(within(nav).getByRole("button", { name: label })).toBeInTheDocument();
    }
  });
});
