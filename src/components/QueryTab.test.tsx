import { EditorView } from "@codemirror/view";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type ColumnMeta } from "../api";
import { useStore, type QueryTab as QueryTabState } from "../state/store";
import { connectedStore, pendingChange, queryTab, resultSet, table, text, value } from "../test/fixtures";
import { Dialogs } from "./Dialogs";
import { handleKeydown, installCommands } from "../commands";
import { QueryTab } from "./QueryTab";
import { StatusBar } from "./StatusBar";
import { Toasts } from "./Toasts";

const mocked = vi.mocked(api, true);

function renderTab(sql: string, patch: Partial<QueryTabState> = {}) {
  connectedStore({ tabs: [queryTab({ sql, ...patch })] });
  const Current = () => {
    const tab = useStore((s) => s.tabs[0]);
    return tab.kind === "query" ? <QueryTab tab={tab} /> : null;
  };
  return render(
    <>
      <Current />
      <StatusBar />
      <Dialogs />
    </>,
  );
}

describe("QueryTab", () => {
  beforeEach(() => vi.clearAllMocks());

  it("runs the query and shows rows with the NULL label", async () => {
    mocked.execute.mockResolvedValue({ results: [resultSet()], elapsedMs: 12, database: "db", failure: null });
    renderTab("SELECT ID, post_title FROM wp_posts");
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));

    expect(mocked.execute).toHaveBeenCalledWith("s1", "SELECT ID, post_title FROM wp_posts", false);
    const grid = await screen.findByRole("grid");
    expect(within(grid).getByText("Hello world")).toBeInTheDocument();
    expect(within(grid).getByText("NULL")).toBeInTheDocument();
    expect(screen.getByText("2 rows")).toBeInTheDocument();
    expect(screen.getByText("12 ms")).toBeInTheDocument();
  });

  it("runs only the statement under the cursor with ⌘↵", async () => {
    // The shortcut is an app command; the window listener is what App installs.
    installCommands();
    window.addEventListener("keydown", handleKeydown, true);
    try {
      mocked.execute.mockResolvedValue({ results: [resultSet()], elapsedMs: 1, database: "db", failure: null });
      const { container } = renderTab("SELECT 1;\nSELECT 2;");
      const content = container.querySelector(".cm-content") as HTMLElement;
      EditorView.findFromDOM(content)!.dispatch({ selection: { anchor: "SELECT 1;\nSELECT 2;".length } });
      fireEvent.keyDown(content, { key: "Enter", metaKey: true });
      expect(mocked.execute).toHaveBeenCalledWith("s1", "SELECT 2", false);
    } finally {
      window.removeEventListener("keydown", handleKeydown, true);
    }
  });

  it("explains when the row limit cut the result", async () => {
    const rs = resultSet({ totalRows: 5000, truncated: true });
    mocked.execute.mockResolvedValue({ results: [rs], elapsedMs: 3, database: "db", failure: null });
    renderTab("SELECT * FROM wp_postmeta");
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    expect(await screen.findByText("Showing 2 of 5,000 rows (row limit 1,000)")).toBeInTheDocument();
  });

  it("shows server errors with their code", async () => {
    mocked.execute.mockRejectedValue({ kind: "database", message: "Table 'db.nope' doesn't exist", code: "1146" });
    renderTab("SELECT * FROM nope");
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Error 1146");
    expect(alert).toHaveTextContent("Table 'db.nope' doesn't exist");
  });

  describe("when a statement of a script fails", () => {
    const failure = { index: 1, error: { kind: "database" as const, message: "Table 'db.missing' doesn't exist", code: "1146", account: null } };
    const partial = { results: [resultSet()], elapsedMs: 3, database: "db", failure };

    it("shows the earlier result and names the failed statement", async () => {
      mocked.execute.mockResolvedValue(partial);
      renderTab("SELECT 1; SELECT * FROM missing; SELECT 3");
      await userEvent.click(screen.getByRole("button", { name: /^Run all$/ }));

      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent("Statement 2 failed");
      expect(alert).toHaveTextContent("Error 1146");
      expect(alert).toHaveTextContent("Table 'db.missing' doesn't exist");
      expect(within(screen.getByRole("grid")).getByText("Hello world")).toBeInTheDocument();
    });

    it("hides the failure while the next run is in progress", async () => {
      mocked.execute.mockResolvedValueOnce(partial).mockReturnValueOnce(new Promise(() => {}));
      renderTab("SELECT 1; SELECT * FROM missing");
      await userEvent.click(screen.getByRole("button", { name: /^Run all$/ }));
      await screen.findByText("Statement 2 failed");

      await userEvent.click(screen.getByRole("button", { name: /^Run all$/ }));
      expect(await screen.findByText("Running…")).toBeInTheDocument();
      expect(screen.queryByText("Statement 2 failed")).not.toBeInTheDocument();
    });
  });

  it("toasts that the session was reset when a timeout reconnected it", async () => {
    mocked.execute.mockRejectedValue({ kind: "timeout", message: "Query timed out", code: null, sessionReset: true });
    renderTab("SELECT SLEEP(100)");
    render(<Toasts />);
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    expect(await screen.findByText("Session reset after timeout: temporary tables, variables and open transactions are gone")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Query timed out");
  });

  it("does not toast a session reset for a plain timeout", async () => {
    mocked.execute.mockRejectedValue({ kind: "timeout", message: "Query timed out", code: null });
    renderTab("SELECT SLEEP(100)");
    render(<Toasts />);
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Query timed out");
    expect(screen.queryByText(/Session reset/)).not.toBeInTheDocument();
  });

  it("reports affected rows for writes", async () => {
    const rs = resultSet({ returnsRows: false, columns: [], rows: [], affectedRows: 3, lastInsertId: 42 });
    mocked.execute.mockResolvedValue({ results: [rs], elapsedMs: 1, database: "db", failure: null });
    renderTab("INSERT INTO t VALUES (1),(2),(3)");
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    expect(await screen.findByText("3 rows affected")).toBeInTheDocument();
    expect(screen.getByText("Last insert ID: 42")).toBeInTheDocument();
  });

  it("asks for confirmation before destructive statements and reruns when confirmed", async () => {
    mocked.execute
      .mockRejectedValueOnce({ kind: "confirmation_required", message: "This statement needs confirmation: TRUNCATE removes all rows of the table", code: null })
      .mockResolvedValueOnce({ results: [resultSet({ returnsRows: false, columns: [], rows: [] })], elapsedMs: 1, database: "db", failure: null });
    renderTab("TRUNCATE wp_actionscheduler_logs");
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("TRUNCATE removes all rows of the table");
    await userEvent.click(within(dialog).getByRole("button", { name: "Run anyway" }));
    expect(mocked.execute).toHaveBeenLastCalledWith("s1", "TRUNCATE wp_actionscheduler_logs", true);
  });

  it("shows the server's reason in the confirmation dialog", async () => {
    mocked.execute.mockRejectedValueOnce({ kind: "confirmation_required", message: "This statement needs confirmation: DROP TABLE", code: null, account: null, reason: "DROP TABLE" });
    renderTab("DROP TABLE x");
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("DROP TABLE");
    expect(dialog).not.toHaveTextContent("needs confirmation");
  });

  it("does not run destructive statements when the confirmation is cancelled", async () => {
    mocked.execute.mockRejectedValueOnce({ kind: "confirmation_required", message: "DROP TABLE permanently removes data", code: null });
    const { container } = renderTab("DROP TABLE x");
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    await userEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Cancel" }));
    expect(mocked.execute).not.toHaveBeenCalledWith("s1", "DROP TABLE x", true);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("grid")).not.toBeInTheDocument();
    expect(screen.queryByText(/rows affected/)).not.toBeInTheDocument();
    expect(container.querySelector(".cm-content")).toHaveTextContent("DROP TABLE x");
  });

  it("offers cancel while a query runs", async () => {
    mocked.execute.mockReturnValue(new Promise(() => {}));
    renderTab("SELECT SLEEP(10)");
    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(mocked.cancelQuery).toHaveBeenCalledWith("s1");
  });

  describe("shortcut hint", () => {
    beforeEach(() => {
      connectedStore();
      installCommands();
    });

    it("shows the bound run shortcuts", () => {
      renderTab("SELECT 1");
      expect(screen.getByText(/⌘↵ statement/)).toBeInTheDocument();
      expect(screen.getByText(/⌘⇧↵ all/)).toBeInTheDocument();
    });

    it("follows a rebinding of editor.run", async () => {
      renderTab("SELECT 1");
      await act(() => useStore.getState().saveKeybindings({ "editor.run": "Mod+Shift+R" }));
      expect(screen.getByText(/⌘⇧R statement/)).toBeInTheDocument();
      expect(screen.queryByText(/⌘↵ statement/)).not.toBeInTheDocument();
    });

    it("drops the run part of the hint when editor.run is unbound", async () => {
      renderTab("SELECT 1");
      await act(() => useStore.getState().saveKeybindings({ "editor.run": null }));
      expect(screen.queryByText(/ statement/)).not.toBeInTheDocument();
      expect(screen.getByText(/⌘⇧↵ all/)).toBeInTheDocument();
    });
  });

  describe("saved queries", () => {
    const sql = "SELECT * FROM wp_wc_orders WHERE status = 'wc-failed'";
    const project = { id: "failed-orders", origin: { kind: "project" as const }, database: "db", pin: null };
    const preset = { id: "abandoned-carts", origin: { kind: "preset" as const, preset: "woocommerce" }, database: "db", pin: null };
    const saveButton = () => screen.getByRole("button", { name: "Save" });
    const pinButton = () => screen.getByRole("button", { name: /^Pin/ });

    it("asks for a title when saving an unsaved tab, stores the query in the project and shows its origin", async () => {
      renderTab(sql);
      await userEvent.click(saveButton());
      const input = screen.getByRole("textbox", { name: "Save query as" });
      await userEvent.clear(input);
      await userEvent.type(input, "Failed orders{Enter}");

      await waitFor(() => expect(mocked.saveQuery).toHaveBeenCalledWith("s1", "db", "Failed orders", sql, null));
      expect(await screen.findByText("project")).toBeInTheDocument();
      expect(useStore.getState().tabs[0]).toMatchObject({ title: "Failed orders", saved: { id: "saved-1", origin: { kind: "project" }, database: "db" } });
      expect(pinButton()).toBeEnabled();
    });

    it("updates the stored query without asking when saving a saved tab", async () => {
      renderTab(sql, { title: "Failed orders", saved: project });
      await userEvent.click(saveButton());
      await waitFor(() => expect(mocked.updateSavedQuery).toHaveBeenCalledWith("s1", "db", { id: "failed-orders", title: "Failed orders", sql, pin: null }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(mocked.saveQuery).not.toHaveBeenCalled();
    });

    it("keeps Save off for an empty editor", () => {
      renderTab("");
      expect(saveButton()).toBeDisabled();
    });

    it.each([
      ["an unsaved tab", {}],
      ["a preset query", { saved: preset }],
    ])("disables Pin for %s", (_name, patch) => {
      renderTab(sql, patch);
      expect(pinButton()).toBeDisabled();
    });

    describe("pinning", () => {
      beforeEach(() => renderTab(sql, { title: "Failed orders", saved: project }));

      it("stores a count", async () => {
        await userEvent.click(pinButton());
        const dialog = await screen.findByRole("dialog");
        expect(dialog).toHaveTextContent("Pin Failed orders");
        await userEvent.click(within(dialog).getByRole("radio", { name: "Count" }));
        await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));

        await waitFor(() => expect(mocked.updateSavedQuery).toHaveBeenCalledWith("s1", "db", { id: "failed-orders", title: "Failed orders", sql, pin: { show: "count" } }));
        expect(await screen.findByRole("button", { name: /Pinned · count/ })).toBeInTheDocument();
      });

      it("stores the first rows with their limit", async () => {
        await userEvent.click(pinButton());
        const dialog = await screen.findByRole("dialog");
        await userEvent.click(within(dialog).getByRole("radio", { name: "Rows" }));
        const limit = within(dialog).getByLabelText("Rows to show");
        await userEvent.clear(limit);
        await userEvent.type(limit, "10");
        await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));

        await waitFor(() => expect(mocked.updateSavedQuery).toHaveBeenCalledWith("s1", "db", { id: "failed-orders", title: "Failed orders", sql, pin: { show: "rows", limit: 10 } }));
        expect(await screen.findByRole("button", { name: /Pinned · 10 rows/ })).toBeInTheDocument();
      });
    });

    describe("a preset query", () => {
      beforeEach(() => {
        mocked.copyPresetQueryToProject.mockResolvedValue({ id: "copy-abandoned-carts", title: "Abandoned carts", sql, pin: null });
        renderTab(sql, { title: "copy of Abandoned carts", saved: preset });
      });

      it("shows its origin and offers to copy it to the project", () => {
        expect(screen.getByText("preset:woocommerce")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Copy to project" })).toBeInTheDocument();
      });

      it("saves to the project's copy once copied", async () => {
        await userEvent.click(screen.getByRole("button", { name: "Copy to project" }));
        await waitFor(() => expect(mocked.copyPresetQueryToProject).toHaveBeenCalledWith("s1", "db", "woocommerce", "abandoned-carts"));
        expect(await screen.findByText("project")).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Copy to project" })).not.toBeInTheDocument();

        await userEvent.click(saveButton());
        await waitFor(() => expect(mocked.updateSavedQuery).toHaveBeenCalledWith("s1", "db", { id: "copy-abandoned-carts", title: "Abandoned carts", sql, pin: null }));
      });

      it("asks for a title when saved before it was copied, and stores a new project query", async () => {
        await userEvent.click(saveButton());
        expect(screen.getByRole("textbox", { name: "Save query as" })).toHaveValue("copy of Abandoned carts");
        await userEvent.keyboard("{Enter}");
        await waitFor(() => expect(mocked.saveQuery).toHaveBeenCalledWith("s1", "db", "copy of Abandoned carts", sql, null));
      });
    });

    describe("shortcuts", () => {
      beforeEach(() => {
        installCommands();
        window.addEventListener("keydown", handleKeydown, true);
      });
      afterEach(() => window.removeEventListener("keydown", handleKeydown, true));

      it("⌘S saves the query of the active tab when nothing is pending", async () => {
        renderTab(sql);
        await userEvent.keyboard("{Meta>}s{/Meta}");
        expect(await screen.findByRole("dialog", { name: "Save query as" })).toBeInTheDocument();
        expect(mocked.applyChanges).not.toHaveBeenCalled();
      });

      it("⌘S applies the pending changes first", async () => {
        renderTab(sql);
        act(() => useStore.setState({ pending: [pendingChange({ id: 4 })] }));
        await userEvent.keyboard("{Meta>}s{/Meta}");
        await waitFor(() => expect(mocked.applyChanges).toHaveBeenCalledWith("s1", [4]));
        expect(screen.queryByRole("dialog", { name: "Save query as" })).not.toBeInTheDocument();
      });

      it("P in the results opens the pin dialog of a saved query", async () => {
        mocked.execute.mockResolvedValue({ results: [resultSet()], elapsedMs: 1, database: "db", failure: null });
        renderTab(sql, { title: "Failed orders", saved: project });
        await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
        (await screen.findByRole("grid")).focus();
        await userEvent.keyboard("p");
        expect(await screen.findByRole("dialog")).toHaveTextContent("Pin Failed orders");
      });
    });
  });

  describe("value counts", () => {
    const sql = "SELECT status FROM wp_wc_orders";
    const statusColumn = { name: "status", typeName: "VARCHAR", origin: { database: "db", table: "wp_wc_orders", column: "status" } };
    const counts = (...entries: [string, number][]) => entries.map(([v, count]) => ({ value: value({ raw: text(v) }), count }));

    async function openMenu(columns: ColumnMeta[] = [statusColumn]) {
      mocked.execute.mockResolvedValue({ results: [resultSet({ columns, rows: [[text("wc-failed")]], totalRows: 1 })], elapsedMs: 1, database: "db", failure: null });
      renderTab(sql);
      await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
      fireEvent.contextMenu(await screen.findByRole("columnheader", { name: /status/ }));
      return screen.findByRole("menuitem", { name: "Value counts" });
    }

    beforeEach(() => mocked.narrowRefusal.mockResolvedValue(null));

    it("lists the stored values of the column read from a table with their counts", async () => {
      mocked.valueCounts.mockResolvedValue(counts(["wc-completed", 900], ["wc-failed", 7]));
      await userEvent.click(await openMenu());

      const list = await screen.findByRole("listbox", { name: "Values" });
      expect(mocked.valueCounts).toHaveBeenCalledWith("s1", "db", "wp_wc_orders", "status", [], 50);
      expect(within(list).getByRole("option", { name: /wc-failed/ })).toHaveTextContent("7");
      expect(within(list).getByRole("option", { name: /wc-completed/ })).toHaveTextContent("900");
    });

    it("disables the item for a column that is not read from a table", async () => {
      const item = await openMenu([{ ...statusColumn, origin: null }]);
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", "This column is not read from a table");
    });

    it("disables the item with the reason when the query cannot be narrowed", async () => {
      mocked.narrowRefusal.mockResolvedValue("A query with GROUP BY cannot be narrowed");
      const item = await openMenu();
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", "A query with GROUP BY cannot be narrowed");
      expect(mocked.narrowRefusal).toHaveBeenCalledWith(sql);
    });

    it("narrows the query to the picked value and runs it again", async () => {
      mocked.valueCounts.mockResolvedValue(counts(["wc-failed", 7]));
      mocked.narrowQuery.mockResolvedValue("SELECT status FROM wp_wc_orders WHERE `status` = 'wc-failed'");
      await userEvent.click(await openMenu());
      await userEvent.click(await screen.findByRole("option", { name: /wc-failed/ }));

      await waitFor(() => expect(mocked.narrowQuery).toHaveBeenCalledWith("s1", sql, "status", "wc-failed"));
      await waitFor(() => expect(mocked.execute).toHaveBeenLastCalledWith("s1", "SELECT status FROM wp_wc_orders WHERE `status` = 'wc-failed'", false));
      expect(useStore.getState().tabs[0]).toMatchObject({ sql: "SELECT status FROM wp_wc_orders WHERE `status` = 'wc-failed'" });
      expect(screen.queryByRole("dialog", { name: /Value counts/ })).not.toBeInTheDocument();
    });

    it("reports a query that could not be narrowed and keeps the SQL", async () => {
      mocked.valueCounts.mockResolvedValue(counts(["wc-failed", 7]));
      mocked.narrowQuery.mockRejectedValue({ kind: "invalid_input", message: "Cannot narrow this query", code: null });
      render(<Toasts />);
      await userEvent.click(await openMenu());
      await userEvent.click(await screen.findByRole("option", { name: /wc-failed/ }));
      expect(await screen.findByText("Could not filter the query: Cannot narrow this query")).toBeInTheDocument();
      expect(useStore.getState().tabs[0]).toMatchObject({ sql });
    });
  });

  describe("completion schema", () => {
    it("is requested again after a DDL statement reloads the table list", async () => {
      mocked.execute.mockResolvedValue({ results: [resultSet({ returnsRows: false, columns: [], rows: [] })], elapsedMs: 1, database: "db", failure: null });
      mocked.listTables.mockResolvedValue([table("wp_new")]);
      renderTab("CREATE TABLE wp_new (id INT)");
      await waitFor(() => expect(mocked.completionSchema).toHaveBeenCalled());
      mocked.completionSchema.mockClear();

      await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));

      await waitFor(() => expect(mocked.completionSchema).toHaveBeenCalledWith("s1", "db"));
    });
  });
});
