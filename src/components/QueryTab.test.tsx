import { EditorView } from "@codemirror/view";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { useStore } from "../state/store";
import { connectedStore, queryTab, resultSet, table } from "../test/fixtures";
import { Dialogs } from "./Dialogs";
import { handleKeydown, installCommands } from "../commands";
import { QueryTab } from "./QueryTab";
import { StatusBar } from "./StatusBar";
import { Toasts } from "./Toasts";

const mocked = vi.mocked(api, true);

function renderTab(sql: string) {
  connectedStore({ tabs: [queryTab({ sql })] });
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
