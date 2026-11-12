import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Blocks } from "lucide-react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { registerSettingsPage } from "../../registry/settingsPages";
import { resetStore, useStore } from "../../state/store";
import { connectedStore, settings } from "../../test/fixtures";
import { Group } from "./controls";
import { SettingsWindow, type SettingsRequest } from "./SettingsWindow";

const mocked = vi.mocked(api, true);
const scrollIntoView = vi.fn();

/** The window with the request state its host keeps (a new object per request); `onOpen` sees every page or row the user picks. */
function renderWindow(initial?: string) {
  const onOpen = vi.fn();
  function Host() {
    const [request, setRequest] = useState<SettingsRequest>({ id: initial });
    return (
      <SettingsWindow
        request={request}
        onOpen={(id) => {
          onOpen(id);
          setRequest({ id });
        }}
      />
    );
  }
  render(<Host />);
  return onOpen;
}

const nav = () => screen.getByRole("navigation", { name: "Settings pages" });
const search = () => screen.getByLabelText("Search Settings");
const goTo = (page: string) => userEvent.click(within(nav()).getByRole("button", { name: page }));
const rowOf = (label: string) => screen.getByLabelText(label).closest("[data-anchor]")!;

describe("SettingsWindow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Element.prototype.scrollIntoView = scrollIntoView;
    resetStore({ settings: settings() });
  });

  describe("pages", () => {
    it("opens on the first page by default", () => {
      renderWindow();
      expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
      expect(within(nav()).getByRole("button", { name: "Appearance" })).toHaveAttribute("aria-current", "page");
    });

    it("opens on the page it is given", () => {
      renderWindow("status");
      expect(screen.getByRole("heading", { name: "Status" })).toBeInTheDocument();
      expect(within(nav()).getByRole("button", { name: "Status" })).toHaveAttribute("aria-current", "page");
      expect(within(nav()).getByRole("button", { name: "Appearance" })).not.toHaveAttribute("aria-current");
    });

    it("opens on the first page when the page is unknown", () => {
      renderWindow("no-such-page");
      expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
    });

    it("renders nothing until the settings are loaded", () => {
      resetStore();
      renderWindow();
      expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
    });

    it("lists the pages under their group headings", () => {
      renderWindow();
      expect(within(within(nav()).getByRole("group", { name: "Database" })).getAllByRole("button").map((b) => b.textContent)).toEqual(["Queries", "Display", "Connections"]);
    });

    it("reports the page a nav click picks and shows it", async () => {
      const onOpen = renderWindow();
      await goTo("Queries");
      expect(onOpen).toHaveBeenCalledWith("queries");
      expect(screen.getByRole("heading", { name: "Queries" })).toBeInTheDocument();
      await userEvent.click(screen.getByRole("switch", { name: "Confirm destructive statements" }));
      expect(mocked.updateSettings).toHaveBeenCalledWith(expect.objectContaining({ confirmDestructive: !settings().confirmDestructive }));
    });
  });

  describe("search", () => {
    it("replaces the page list with a Results group of the matching pages and rows", async () => {
      renderWindow();
      await userEvent.type(search(), "queries");
      const results = within(nav()).getByRole("group", { name: "Results" });
      expect(within(results).getByRole("button", { name: "Queries" })).toBeInTheDocument();
      expect(within(nav()).queryByRole("group", { name: "Interface" })).not.toBeInTheDocument();
      expect(within(nav()).queryByRole("button", { name: "Display" })).not.toBeInTheDocument();
    });

    it("shows a row hit on two lines, the page and below it the row, named by both", async () => {
      renderWindow();
      await userEvent.type(search(), "dark");
      const hit = within(nav()).getByRole("button", { name: "Dark scheme, Appearance" });
      expect(within(hit).getByText("Appearance")).toBeInTheDocument();
      expect(within(hit).getByText("Dark scheme")).toBeInTheDocument();
    });

    it("says No Results when nothing matches", async () => {
      renderWindow();
      await userEvent.type(search(), "zzzz");
      expect(screen.getByText("No Results")).toBeInTheDocument();
      expect(within(nav()).queryByRole("button")).not.toBeInTheDocument();
    });

    it("opens the first hit on Enter", async () => {
      const onOpen = renderWindow();
      await userEvent.type(search(), "queries{Enter}");
      expect(onOpen).toHaveBeenLastCalledWith("queries");
      expect(screen.getByRole("heading", { name: "Queries" })).toBeInTheDocument();
    });

    it("opens the row of the first hit on Enter", async () => {
      const onOpen = renderWindow();
      await userEvent.type(search(), "row limit{Enter}");
      expect(onOpen).toHaveBeenLastCalledWith("queries-row-limit");
      expect(screen.getByRole("heading", { name: "Queries" })).toBeInTheDocument();
      expect(rowOf("Row limit")).toHaveClass("found");
    });

    it("does nothing on Enter without a hit", async () => {
      const onOpen = renderWindow();
      await userEvent.type(search(), "zzzz{Enter}");
      expect(onOpen).not.toHaveBeenCalled();
    });

    it("opens the page of a clicked row hit and highlights the row", async () => {
      const onOpen = renderWindow("queries");
      await userEvent.type(search(), "dark");
      await userEvent.click(within(nav()).getByRole("button", { name: "Dark scheme, Appearance" }));
      expect(onOpen).toHaveBeenCalledWith("appearance-dark-scheme");
      expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
      const row = rowOf("Dark scheme");
      expect(row).toHaveClass("found");
      expect(scrollIntoView.mock.contexts).toContain(row);
    });

    it("highlights the row again when the same hit is picked twice", async () => {
      renderWindow();
      await userEvent.type(search(), "dark");
      const hit = () => within(nav()).getByRole("button", { name: "Dark scheme, Appearance" });
      await userEvent.click(hit());
      const row = rowOf("Dark scheme");
      expect(row).toHaveClass("found");
      row.classList.remove("found");
      await userEvent.click(hit());
      expect(row).toHaveClass("found");
    });

    it("clears the search on a first Escape and brings the page list back", async () => {
      renderWindow();
      await userEvent.type(search(), "dark");
      await userEvent.keyboard("{Escape}");
      expect(search()).toHaveValue("");
      expect(within(nav()).getByRole("group", { name: "Interface" })).toBeInTheDocument();
    });
  });

  describe("rows", () => {
    it("opens the page that holds a row id and scrolls the row into view", () => {
      renderWindow("queries-row-limit");
      expect(screen.getByRole("heading", { name: "Queries" })).toBeInTheDocument();
      expect(within(nav()).getByRole("button", { name: "Queries" })).toHaveAttribute("aria-current", "page");
      expect(scrollIntoView.mock.contexts).toContain(rowOf("Row limit"));
    });

    it("highlights a group when its id is given", () => {
      renderWindow("appearance-layout");
      expect(screen.getByRole("region", { name: "Layout" })).toHaveClass("found");
    });

    it("does not scroll when a page id is given", () => {
      renderWindow("queries");
      expect(scrollIntoView).not.toHaveBeenCalled();
    });

    it("reveals the anchor named by `under` while the row itself is not shown", () => {
      registerSettingsPage({
        id: "test-under",
        group: "Test Group",
        label: "Under",
        icon: Blocks,
        rows: [{ id: "under-row", title: "Hidden row", under: "under-group" }],
        Body: () => (
          <Group title="Shown group" anchor="under-group">
            <p>Contents</p>
          </Group>
        ),
      });
      renderWindow("under-row");
      expect(screen.getByRole("region", { name: "Shown group" })).toHaveClass("found");
    });
  });

  describe("settings", () => {
    it("applies changes immediately, committing text fields on blur", async () => {
      renderWindow();
      await userEvent.click(screen.getByRole("button", { name: "Dark" }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ theme: "dark" }));
      await goTo("Display");
      const nullLabel = screen.getByLabelText("NULL label");
      await userEvent.clear(nullLabel);
      await userEvent.type(nullLabel, "∅");
      expect(nullLabel).toHaveValue("∅");
      expect(mocked.updateSettings).not.toHaveBeenCalledWith(expect.objectContaining({ nullLabel: "∅" }));
      await userEvent.tab();
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ nullLabel: "∅", theme: "dark" }));
    });

    it("saves quietly, without a Settings saved toast", async () => {
      renderWindow();
      await userEvent.click(screen.getByRole("button", { name: "Dark" }));
      expect(mocked.updateSettings).toHaveBeenCalled();
      expect(screen.queryByText("Settings saved")).not.toBeInTheDocument();
    });

    it("turns the table live check off and on from the Queries page", async () => {
      renderWindow("queries");
      const toggle = screen.getByRole("switch", { name: "Live check in table tabs" });
      expect(toggle).toBeChecked();
      await userEvent.click(toggle);
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ tableLiveCheck: false }));
      expect(toggle).not.toBeChecked();
      await userEvent.click(toggle);
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ tableLiveCheck: true }));
    });

    describe("number fields", () => {
      it("commit a typed size on blur, not per keystroke", async () => {
        renderWindow();
        const uiSize = screen.getByLabelText("UI size");
        await userEvent.clear(uiSize);
        await userEvent.type(uiSize, "16");
        expect(mocked.updateSettings).not.toHaveBeenCalled();
        await userEvent.tab();
        expect(mocked.updateSettings).not.toHaveBeenCalledWith(expect.objectContaining({ uiFontSize: 1 }));
        expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ uiFontSize: 16 }));
        expect(uiSize).toHaveValue(16);
      });

      it.each([
        ["on blur", "{Tab}"],
        ["on Enter", "{Enter}"],
      ])("commit a typed six-digit value without partials %s", async (_when, key) => {
        renderWindow("queries");
        const rowLimit = screen.getByLabelText("Row limit");
        await userEvent.clear(rowLimit);
        await userEvent.type(rowLimit, `100000${key}`);
        for (const partial of [1, 10, 100, 1000, 10000]) {
          expect(mocked.updateSettings).not.toHaveBeenCalledWith(expect.objectContaining({ rowLimit: partial }));
        }
        expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ rowLimit: 100000 }));
      });

      it("save the live interval from the Queries page", async () => {
        renderWindow("queries");
        const interval = screen.getByLabelText("Live interval");
        expect(interval).toHaveValue(2);
        await userEvent.clear(interval);
        await userEvent.type(interval, "5{Tab}");
        expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ liveIntervalSecs: 5 }));
      });

      it("restore the saved value instead of committing an out-of-range one", async () => {
        renderWindow();
        const uiSize = screen.getByLabelText("UI size");
        await userEvent.clear(uiSize);
        await userEvent.type(uiSize, "99");
        await userEvent.tab();
        expect(mocked.updateSettings).not.toHaveBeenCalled();
        expect(uiSize).toHaveValue(13);
      });

      it("preview a spinner step after a short pause", async () => {
        vi.useFakeTimers({ shouldAdvanceTime: true });
        try {
          renderWindow();
          fireEvent.change(screen.getByLabelText("UI size"), { target: { value: "14" } });
          expect(mocked.updateSettings).not.toHaveBeenCalled();
          await act(async () => {
            await vi.advanceTimersByTimeAsync(300);
          });
          expect(mocked.updateSettings).toHaveBeenCalledWith(expect.objectContaining({ uiFontSize: 14 }));
        } finally {
          vi.useRealTimers();
        }
      });
    });
  });

  describe("Appearance page", () => {
    it("saves the accent", async () => {
      renderWindow();
      await userEvent.click(screen.getByRole("button", { name: "Accent gold" }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ accent: "gold" }));
    });

    describe("accent override note", () => {
      beforeEach(() => connectedStore());

      it("says the open database uses its own accent", () => {
        useStore.setState({ accents: { "shop/db": "purple" } });
        renderWindow();
        expect(screen.getByText(/db uses its own: purple\./)).toBeInTheDocument();
      });

      it("only mentions the Project page when the open database has none", () => {
        renderWindow();
        expect(screen.getByText(/can override it on its Project page/)).toBeInTheDocument();
        expect(screen.queryByText(/uses its own/)).not.toBeInTheDocument();
      });
    });

    describe("scheme pop-ups", () => {
      const openMenu = async (label: string) => {
        await userEvent.click(screen.getByLabelText(label));
        return screen.getByRole("menu", { name: label });
      };
      const captions = (menu: HTMLElement) => within(menu).getAllByRole("group").map((g) => g.getAttribute("aria-label"));
      const choices = (menu: HTMLElement, caption: string) =>
        within(within(menu).getByRole("group", { name: caption })).getAllByRole("menuitemradio").map((o) => o.textContent);

      it.each([
        ["Light scheme", ["Light", "Dark"]],
        ["Dark scheme", ["Dark", "Light"]],
      ])("lists the %s groups with the slot's own kind first", async (label, order) => {
        renderWindow();
        expect(captions(await openMenu(label))).toEqual(order);
      });

      it("lists the dark and light schemes in their groups", async () => {
        renderWindow();
        const menu = await openMenu("Dark scheme");
        expect(choices(menu, "Dark")).toEqual(["Ayu Mirage", "Monokai", "Nord", "Solarized Dark"]);
        expect(choices(menu, "Light")).toEqual(["Ayu Light", "Catppuccin Latte", "GitHub Light", "Solarized Light"]);
      });

      it("marks the saved scheme as the checked choice", async () => {
        renderWindow();
        const menu = await openMenu("Dark scheme");
        expect(within(menu).getByRole("menuitemradio", { name: "Ayu Mirage" })).toHaveAttribute("aria-checked", "true");
        expect(within(menu).getByRole("menuitemradio", { name: "Nord" })).toHaveAttribute("aria-checked", "false");
      });

      it.each([
        ["Light scheme", "Ayu Mirage", "lightScheme", "ayu-mirage"],
        ["Dark scheme", "Ayu Light", "darkScheme", "ayu-light"],
        ["Dark scheme", "Nord", "darkScheme", "nord"],
      ])("saves the %s selection %s", async (label, option, key, id) => {
        renderWindow();
        await userEvent.click(within(await openMenu(label)).getByRole("menuitemradio", { name: option }));
        expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ [key]: id }));
      });
    });

    it.each([
      ["Dark", "dark", "light"],
      ["Light", "light", "dark"],
    ])("saves the %s theme", async (name, theme, current) => {
      resetStore({ settings: settings({ theme: current as "light" | "dark" }) });
      renderWindow();
      await userEvent.click(screen.getByRole("button", { name }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ theme }));
    });

    it("saves the row padding", async () => {
      renderWindow();
      await userEvent.click(within(screen.getByRole("group", { name: "Row padding" })).getByRole("button", { name: "1" }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ rowPadding: 1 }));
    });

    it.each([1, 2, 3])("saves %i panes side by side", async (panes) => {
      resetStore({ settings: settings({ maxPanes: panes === 1 ? 2 : 1 }) });
      renderWindow();
      await userEvent.click(within(screen.getByRole("group", { name: "Panes side by side" })).getByRole("button", { name: String(panes) }));
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ maxPanes: panes }));
    });

    it("marks the saved number of panes as pressed", () => {
      resetStore({ settings: settings({ maxPanes: 3 }) });
      renderWindow();
      const group = screen.getByRole("group", { name: "Panes side by side" });
      expect(within(group).getByRole("button", { name: "3" })).toHaveAttribute("aria-pressed", "true");
      expect(within(group).getByRole("button", { name: "2" })).toHaveAttribute("aria-pressed", "false");
    });

    it("saves a typed mono font on Enter", async () => {
      renderWindow();
      await userEvent.type(screen.getByLabelText("Mono font"), "Fira Code{Enter}");
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ fontMono: "Fira Code" }));
    });

    it("saves null when the mono font is emptied", async () => {
      resetStore({ settings: settings({ fontMono: "Fira Code" }) });
      renderWindow();
      await userEvent.clear(screen.getByLabelText("Mono font"));
      await userEvent.keyboard("{Enter}");
      expect(mocked.updateSettings).toHaveBeenLastCalledWith(expect.objectContaining({ fontMono: null }));
    });
  });

  describe("registered pages", () => {
    const body = (text: string) => () => <p>{text}</p>;

    it("lists a registered page in the nav under its group and renders its body", async () => {
      registerSettingsPage({ id: "test-extra", group: "Test Group", label: "Extra", icon: Blocks, intro: "Extra intro", Body: body("Extra body") });
      renderWindow();
      expect(within(nav()).getByText("Test Group")).toBeInTheDocument();
      await goTo("Extra");
      expect(screen.getByRole("heading", { name: "Extra" })).toBeInTheDocument();
      expect(screen.getByText("Extra intro")).toBeInTheDocument();
      expect(screen.getByText("Extra body")).toBeInTheDocument();
    });

    it("keeps one nav entry with the later definition when an id is registered twice", () => {
      registerSettingsPage({ id: "test-twice", group: "Test Group", label: "Twice old", icon: Blocks, Body: body("Old body") });
      registerSettingsPage({ id: "test-twice", group: "Test Group", label: "Twice new", icon: Blocks, Body: body("New body") });
      renderWindow("test-twice");
      expect(within(nav()).getAllByRole("button", { name: /^Twice/ })).toHaveLength(1);
      expect(within(nav()).getByRole("button", { name: "Twice new" })).toBeInTheDocument();
      expect(screen.getByText("New body")).toBeInTheDocument();
      expect(screen.queryByText("Old body")).not.toBeInTheDocument();
    });

    it("adds a page registered while the window is open", () => {
      renderWindow();
      expect(screen.queryByRole("button", { name: "Late" })).not.toBeInTheDocument();
      act(() => registerSettingsPage({ id: "test-late", group: "Test Group", label: "Late", icon: Blocks, Body: body("Late body") }));
      expect(screen.getByRole("button", { name: "Late" })).toBeInTheDocument();
    });

    it("finds a registered page's rows in the search", async () => {
      registerSettingsPage({ id: "test-rows", group: "Test Group", label: "Rows page", icon: Blocks, rows: [{ id: "test-rows-zebra", title: "Zebra stripes" }], Body: body("Rows body") });
      renderWindow();
      await userEvent.type(search(), "zebra");
      await userEvent.click(within(nav()).getByRole("button", { name: "Zebra stripes, Rows page" }));
      expect(screen.getByRole("heading", { name: "Rows page" })).toBeInTheDocument();
    });
  });
});
