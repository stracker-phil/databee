import { render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));
vi.mock("../../stage/api", async () => ({ stageApi: (await import("../../stage/apiMock")).createStageApiMock() }));

import catalog from "../../commands/catalog.gen.json";
import { installCommands } from "../../commands";
import { pageRows, settingsPages } from "../../registry/settingsPages";
import { installStage } from "../../stage/install";
import { resetStore } from "../../state/store";
import { settings } from "../../test/settings";
import { registerCoreSettingsPages } from "./pages";
import { SettingsWindow } from "./SettingsWindow";

const PAGE_IDS = ["appearance", "shortcuts", "queries", "display", "connections", "status", "about", "presentations"];

const anchored = () => [...document.querySelectorAll<HTMLElement>("[data-anchor]")];

/** The title a row or group is known by on screen. */
function shownTitle(element: HTMLElement): string | undefined {
  const text = element.matches(".settings-row") ? element.querySelector(".settings-row-label, label") : element.querySelector("h3");
  return text?.textContent ?? undefined;
}

async function renderPageLoaded(id: string) {
  render(<SettingsWindow request={{ id }} onOpen={() => {}} />);
  await waitFor(() => expect(document.querySelector(".settings-row"), `page "${id}" shows no rows`).not.toBeNull());
}

const rowsOf = (id: string) => pageRows(settingsPages().find((page) => page.id === id)!);

beforeEach(() => {
  vi.clearAllMocks();
  registerCoreSettingsPages();
  installStage("settings");
  installCommands();
  resetStore({ settings: settings() });
});

describe("the Settings search index", () => {
  it("covers every registered page", () => {
    expect(settingsPages().map((page) => page.id).sort()).toEqual([...PAGE_IDS].sort());
  });

  describe.each(PAGE_IDS)("page %s", (id) => {
    it("has an entry for every anchored row or group it shows, under the same title", async () => {
      await renderPageLoaded(id);

      for (const element of anchored()) {
        const anchor = element.dataset.anchor!;
        const entry = rowsOf(id).find((row) => row.id === anchor);
        expect(entry, `page "${id}" shows "${anchor}" but registers no row with that id`).toBeDefined();
        expect(entry?.title, `page "${id}", row "${anchor}": the entry's title differs from the title shown`).toBe(shownTitle(element));
      }
    });

    it("shows every row it registers, except those that wait under another row", async () => {
      await renderPageLoaded(id);

      const shown = new Set(anchored().map((element) => element.dataset.anchor));
      for (const entry of rowsOf(id).filter((row) => !row.under)) {
        expect(shown.has(entry.id), `page "${id}" registers "${entry.id}" (${entry.title}) but shows no element with that anchor`).toBe(true);
      }
    });

    it("gives every row outside a data list an anchor", async () => {
      await renderPageLoaded(id);

      for (const element of document.querySelectorAll<HTMLElement>(".settings-row")) {
        if (element.closest("[data-list]")) continue;
        expect(element.dataset.anchor, `page "${id}" shows the row "${shownTitle(element)}" without an anchor`).toBeTruthy();
      }
    });

    it("names a row of the page in every `under`", () => {
      const ids = new Set(rowsOf(id).map((row) => row.id));
      for (const entry of rowsOf(id).filter((row) => row.under)) {
        expect(ids.has(entry.under!), `page "${id}", row "${entry.id}": under "${entry.under}" is not a row of the page`).toBe(true);
      }
    });
  });
});

describe("the deep-open commands", () => {
  it("name a registered Settings page each, so Rust and the pages cannot drift apart", () => {
    registerCoreSettingsPages();
    installStage("settings");
    const pageIds = settingsPages().map((page) => page.id);
    const targets = catalog.map((command) => command.id).filter((id) => id.startsWith("settings.") && id !== "settings.open");

    expect(targets.length).toBeGreaterThan(0);
    for (const id of targets) {
      expect(pageIds, `command "${id}" opens a page that is not registered`).toContain(id.slice("settings.".length));
    }
  });
});
