import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Blocks } from "lucide-react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, LOG_LEVELS } from "../../api";
import { clearCommands, installCommands, registerCommands, registerScope } from "../../commands";
import { registerSettingsPage } from "../../registry/settingsPages";
import { resetStore, useStore } from "../../state/store";
import { connectedStore, settings } from "../../test/fixtures";
import { SettingsWindow } from "./SettingsWindow";

const mocked = vi.mocked(api, true);
const scrollIntoView = vi.fn();

/** The window with the page state its host keeps; `onPage` sees every page the user picks. */
function renderWindow(initial?: string) {
  const onPage = vi.fn();
  function Host() {
    const [page, setPage] = useState(initial);
    return (
      <SettingsWindow
        page={page}
        onPage={(id) => {
          onPage(id);
          setPage(id);
        }}
      />
    );
  }
  render(<Host />);
  return onPage;
}

const nav = () => screen.getByRole("navigation", { name: "Settings pages" });
const search = () => screen.getByLabelText("Search Settings");
const goTo = (page: string) => userEvent.click(within(nav()).getByRole("button", { name: page }));

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
      renderWindow("logging");
      expect(screen.getByRole("heading", { name: "Logging" })).toBeInTheDocument();
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

    it("reports the page a nav click picks and shows it", async () => {
      const onPage = renderWindow();
      await goTo("Queries");
      expect(onPage).toHaveBeenCalledWith("queries");
      expect(screen.getByRole("heading", { name: "Queries" })).toBeInTheDocument();
      await userEvent.click(screen.getByRole("switch", { name: "Confirm destructive statements" }));
      expect(mocked.updateSettings).toHaveBeenCalledWith(expect.objectContaining({ confirmDestructive: !settings().confirmDestructive }));
    });
  });

  describe("search", () => {
    it("narrows the page list to the pages whose label matches", async () => {
      renderWindow();
      await userEvent.type(search(), "log");
      expect(within(nav()).getByRole("button", { name: "Logging" })).toBeInTheDocument();
      expect(within(nav()).queryByRole("button", { name: "Queries" })).not.toBeInTheDocument();
    });

    it("says No Results when nothing matches", async () => {
      renderWindow();
      await userEvent.type(search(), "zzzz");
      expect(screen.getByText("No Results")).toBeInTheDocument();
      expect(within(nav()).queryByRole("button")).not.toBeInTheDocument();
    });

    it("opens the first hit on Enter", async () => {
      const onPage = renderWindow();
      await userEvent.type(search(), "logging{Enter}");
      expect(onPage).toHaveBeenLastCalledWith("logging");
      expect(screen.getByRole("heading", { name: "Logging" })).toBeInTheDocument();
    });

    it("does nothing on Enter without a hit", async () => {
      const onPage = renderWindow();
      await userEvent.type(search(), "zzzz{Enter}");
      expect(onPage).not.toHaveBeenCalled();
    });

    it("shows a hit found by a section title with that section and opens it", async () => {
      const onPage = renderWindow();
      await userEvent.type(search(), "addresses");
      const hit = within(nav()).getByRole("button", { name: "Connections Addresses" });
      expect(hit).toHaveTextContent("Addresses");
      await userEvent.click(hit);
      expect(onPage).toHaveBeenCalledWith("connections-addresses");
      expect(screen.getByRole("heading", { name: "Connections" })).toBeInTheDocument();
      expect(scrollIntoView.mock.contexts).toContain(screen.getByRole("region", { name: "Addresses" }));
    });

    it("opens the section of the first hit on Enter", async () => {
      const onPage = renderWindow();
      await userEvent.type(search(), "look{Enter}");
      expect(onPage).toHaveBeenLastCalledWith("appearance-look");
      expect(scrollIntoView.mock.contexts).toContain(screen.getByRole("region", { name: "Look" }));
    });
  });

  describe("sections", () => {
    it("opens the page that holds a section id and scrolls the group with that anchor into view", () => {
      renderWindow("queries-limits");
      expect(screen.getByRole("heading", { name: "Queries" })).toBeInTheDocument();
      expect(within(nav()).getByRole("button", { name: "Queries" })).toHaveAttribute("aria-current", "page");
      expect(scrollIntoView.mock.contexts).toContain(screen.getByRole("region", { name: "Queries and checks" }));
    });

    it("does not scroll when a page id is given", () => {
      renderWindow("queries");
      expect(scrollIntoView).not.toHaveBeenCalled();
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

    describe("scheme groups", () => {
      const groupLabels = (select: HTMLElement) => Array.from(select.querySelectorAll("optgroup")).map((g) => g.label);
      const groupOptions = (select: HTMLElement, label: string) =>
        within(within(select).getByRole("group", { name: label })).getAllByRole("option").map((o) => o.textContent);

      it.each([
        ["Light scheme", ["Light", "Dark"]],
        ["Dark scheme", ["Dark", "Light"]],
      ])("lists the %s groups with the slot's own kind first", (label, order) => {
        renderWindow();
        expect(groupLabels(screen.getByLabelText(label))).toEqual(order);
      });

      it("lists the dark and light schemes in their groups", () => {
        renderWindow();
        const select = screen.getByLabelText("Dark scheme");
        expect(groupOptions(select, "Dark")).toEqual(["Ayu Mirage", "Monokai", "Nord", "Solarized Dark"]);
        expect(groupOptions(select, "Light")).toEqual(["Ayu Light", "Catppuccin Latte", "GitHub Light", "Solarized Light"]);
      });

      it.each([
        ["Light scheme", "Ayu Mirage", "lightScheme", "ayu-mirage"],
        ["Dark scheme", "Ayu Light", "darkScheme", "ayu-light"],
        ["Dark scheme", "Nord", "darkScheme", "nord"],
      ])("saves the %s selection %s", async (label, option, key, id) => {
        renderWindow();
        await userEvent.selectOptions(screen.getByLabelText(label), option);
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

  describe("Shortcuts page", () => {
    beforeEach(() => {
      connectedStore();
      installCommands();
    });

    it("records, resets and saves shortcut overrides", async () => {
      renderWindow("shortcuts");
      await userEvent.type(screen.getByLabelText("Search shortcuts"), "Command palette");
      const field = screen.getByRole("button", { name: "Record shortcut" });
      expect(field).toHaveTextContent("⌘K");
      await userEvent.click(field);
      await userEvent.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
      expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘⇧P");
      expect(mocked.setKeybindings).toHaveBeenLastCalledWith({ "palette.open": "Mod+Shift+P" });

      await userEvent.click(screen.getByRole("button", { name: "Reset Command palette" }));
      expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘K");
      expect(mocked.setKeybindings).toHaveBeenLastCalledWith(expect.not.objectContaining({ "palette.open": expect.anything() }));
    });

    it("records an already-bound shortcut", async () => {
      renderWindow("shortcuts");
      await userEvent.type(screen.getByLabelText("Search shortcuts"), "Command palette");
      await userEvent.click(screen.getByRole("button", { name: "Record shortcut" }));
      await userEvent.keyboard("{Meta>}t{/Meta}");
      expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘T");
      expect(mocked.setKeybindings).toHaveBeenCalledWith({ "palette.open": "Mod+T" });
    });

    it("records a shortcut when the click does not focus the field, as in WebKit", async () => {
      renderWindow("shortcuts");
      await userEvent.type(screen.getByLabelText("Search shortcuts"), "Command palette");
      fireEvent.click(screen.getByRole("button", { name: "Record shortcut" }));
      await userEvent.keyboard("{Meta>}t{/Meta}");
      expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘T");
      expect(mocked.setKeybindings).toHaveBeenLastCalledWith({ "palette.open": "Mod+T" });

      fireEvent.click(screen.getByRole("button", { name: "Record shortcut" }));
      await userEvent.keyboard("{Escape}");
      expect(screen.getByRole("button", { name: "Record shortcut" })).toHaveTextContent("⌘T");
    });

    it("finds a command by its id in the shortcut search", async () => {
      renderWindow("shortcuts");
      await userEvent.type(screen.getByLabelText("Search shortcuts"), "view.sidebar");
      expect(screen.getByText("Toggle sidebar")).toBeInTheDocument();
      expect(screen.queryByText("Zoom in")).not.toBeInTheDocument();
    });

    it("says so when no command matches the shortcut search", async () => {
      renderWindow("shortcuts");
      await userEvent.type(screen.getByLabelText("Search shortcuts"), "zzzz");
      expect(screen.getByText("No matching commands.")).toBeInTheDocument();
    });

    it("shows each command's scope label", () => {
      registerScope({ id: "labelled", parent: "global", label: "Labelled scope", isActive: () => false });
      clearCommands();
      registerCommands([{ id: "test.labelled", title: "Labelled command", scope: "labelled", run: () => {} }]);
      renderWindow("shortcuts");
      expect(screen.getByText("Labelled scope")).toBeInTheDocument();
      clearCommands();
    });

    it.each([
      ["Zoom in", "Everywhere"],
      ["Run statement at cursor", "Editor"],
      ["Value filter", "Table"],
      ["Show Document", "Object"],
    ])("shows the scope label of %s", async (title, label) => {
      renderWindow("shortcuts");
      await userEvent.type(screen.getByLabelText("Search shortcuts"), title);
      expect(screen.getByText(label)).toBeInTheDocument();
    });

    it("warns when two commands in overlapping scopes share a binding", async () => {
      renderWindow("shortcuts");
      await userEvent.type(screen.getByLabelText("Search shortcuts"), "Zoom in");
      await userEvent.click(screen.getByRole("button", { name: "Record shortcut" }));
      await userEvent.keyboard("{Meta>}k{/Meta}");
      expect(screen.getByRole("alert")).toHaveTextContent(/⌘K is used by both/);
      await userEvent.click(screen.getByRole("button", { name: "Reset Zoom in" }));
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });

  describe("Logging page", () => {
    it("warns that trace logging records SQL", async () => {
      renderWindow("logging");
      await userEvent.selectOptions(screen.getByLabelText("Log level"), "trace");
      expect(screen.getByText(/include the SQL text/)).toBeInTheDocument();
    });

    it("lists every log level", () => {
      renderWindow("logging");
      const options = within(screen.getByLabelText("Log level")).getAllByRole("option");
      expect(options.map((o) => (o as HTMLOptionElement).value)).toEqual([...LOG_LEVELS]);
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
  });
});
