import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", async (orig) => ({ ...(await orig<object>()), api: (await import("./test/apiMock")).createApiMock() }));
vi.mock("./stage/api", async () => ({ stageApi: (await import("./stage/apiMock")).createStageApiMock() }));

import { SettingsWindow } from "./components/settings/SettingsWindow";
import { installModules } from "./modules";
import { resetStore } from "./state/store";
import { settings } from "./test/settings";

describe("installModules", () => {
  beforeEach(() => {
    resetStore({ settings: settings() });
  });

  it.each(["main", "settings"] as const)("makes the %s window's Settings list the core pages and the Presentations page", (window) => {
    installModules(window);
    render(<SettingsWindow request={{}} onOpen={() => {}} />);

    const nav = screen.getByRole("navigation", { name: "Settings pages" });
    for (const label of ["Appearance", "Shortcuts", "Queries", "Display", "Connections", "Status", "About", "Presentations"]) {
      expect(within(nav).getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(within(nav).queryByRole("button", { name: "Logging" })).not.toBeInTheDocument();
  });
});
