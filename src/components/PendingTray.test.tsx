import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api, type AppliedBatch, type PlannedChange } from "../api";
import { installCommands, runCommand } from "../commands";
import { useStore } from "../state/store";
import { connectedStore, pendingChange } from "../test/fixtures";
import { PendingTray } from "./PendingTray";
import { Toasts } from "./Toasts";

const mocked = vi.mocked(api, true);

const planned = (patch: Partial<PlannedChange> = {}): PlannedChange => ({
  id: 1,
  description: "Set post_title",
  statements: ["UPDATE wp_posts SET post_title = 'x'"],
  canUndo: true,
  ...patch,
});

const batch = (patch: Partial<AppliedBatch> = {}): AppliedBatch => ({ batchId: 1, affected: 1, canUndo: true, ...patch });

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const renderTray = () =>
  render(
    <>
      <PendingTray />
      <Toasts />
    </>,
  );

const applyButton = () => screen.getByRole("button", { name: /Apply \(⌘S\)/ });
const undoButton = () => screen.getByRole("button", { name: /Undo last apply/ });
const queryUndoButton = () => screen.queryByRole("button", { name: /Undo last apply/ });

describe("PendingTray", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.applyChanges.mockReset().mockResolvedValue(batch());
    mocked.undoBatch.mockReset().mockResolvedValue(1);
    mocked.discardChange.mockReset().mockResolvedValue(undefined);
    connectedStore();
  });

  describe("queueing", () => {
    it("shows the planned description and its statements for a queued row edit", async () => {
      mocked.planChange.mockResolvedValueOnce(planned({ id: 5, description: "Set wp_posts.post_title where ID = 1", statements: ["UPDATE `wp_posts` SET `post_title` = 'x' WHERE `ID` = 1", "-- second"] }));
      await useStore.getState().queueChange({ type: "setCell", database: "db", table: "wp_posts", keyColumn: "ID", key: "1", column: "post_title", value: "x" });
      renderTray();

      expect(screen.getByText("1 pending change")).toBeInTheDocument();
      expect(screen.getByText("Set wp_posts.post_title where ID = 1")).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "Show SQL" }));
      const statements = within(screen.getByRole("list", { name: "SQL" })).getAllByRole("listitem");
      expect(statements.map((li) => li.textContent)).toEqual(["UPDATE `wp_posts` SET `post_title` = 'x' WHERE `ID` = 1", "-- second"]);
    });

    it("remembers the table a queued row edit writes", async () => {
      mocked.planChange.mockResolvedValueOnce(planned({ id: 5 }));
      await useStore.getState().queueChange({ type: "setCell", database: "db", table: "wp_posts", keyColumn: "ID", key: "1", column: "post_title", value: "x" });
      expect(useStore.getState().pending[0]).toMatchObject({ id: 5, target: { database: "db", table: "wp_posts" } });
    });

    it("keeps no table for the plan of a rule action", async () => {
      mocked.queueActionPreview.mockResolvedValueOnce(planned({ id: 6 }));
      await useStore.getState().queueActionPreview("s1", 3);
      expect(useStore.getState().pending[0].target).toBeUndefined();
    });

    it("resolves to the id of the plan Rust returned", async () => {
      mocked.planChange.mockResolvedValueOnce(planned({ id: 5 }));
      const id = await useStore.getState().queueChange({ type: "deleteRow", database: "db", table: "wp_posts", keyColumn: "ID", key: "1" });
      expect(id).toBe(5);
    });

    it("resolves to null when planning is rejected", async () => {
      mocked.planChange.mockRejectedValueOnce({ kind: "invalid_input", message: "Column cannot be NULL", code: null });
      const id = await useStore.getState().queueChange({ type: "deleteRow", database: "db", table: "wp_posts", keyColumn: "ID", key: "1" });
      expect(id).toBeNull();
    });

    it("marks changes without automatic undo", () => {
      useStore.setState({ pending: [pendingChange({ id: 1, description: "Delete row", canUndo: false }), pendingChange({ id: 2, description: "Set x" })] });
      renderTray();

      expect(screen.getAllByText("no undo")).toHaveLength(1);
      expect(within(screen.getByText("Delete row").parentElement!).getByText("no undo")).toBeInTheDocument();
    });

    it("reports the error and queues nothing when planning is rejected", async () => {
      mocked.planChange.mockRejectedValueOnce({ kind: "invalid_input", message: "Column cannot be NULL", code: null });
      await useStore.getState().queueChange({ type: "deleteRow", database: "db", table: "wp_posts", keyColumn: "ID", key: "1" });
      renderTray();

      expect(screen.queryByText(/pending change/)).not.toBeInTheDocument();
      expect(screen.getByRole("alert")).toHaveTextContent("Cannot prepare change: Column cannot be NULL");
    });
  });

  describe("applying", () => {
    beforeEach(() => useStore.setState({ pending: [pendingChange({ id: 1, statements: ["UPDATE wp_posts SET post_title = 'x'"] })] }));

    it("sends only the ids queued for the active session and keeps the other sessions' changes", async () => {
      useStore.setState({
        pending: [
          pendingChange({ id: 1, description: "Mine A" }),
          pendingChange({ id: 2, description: "Theirs", sessionId: "s2" }),
          pendingChange({ id: 3, description: "Mine B" }),
        ],
      });
      renderTray();

      await userEvent.click(applyButton());

      expect(mocked.applyChanges).toHaveBeenCalledWith("s1", [1, 3]);
      expect(useStore.getState().pending.map((p) => p.id)).toEqual([2]);
    });

    it("clears the tray, enables Undo and reports the count after a successful apply", async () => {
      renderTray();

      await userEvent.click(applyButton());

      expect(await screen.findByText("No pending changes")).toBeInTheDocument();
      expect(undoButton()).toBeEnabled();
      expect(screen.getByRole("status")).toHaveTextContent("Applied 1 change");
    });

    it("offers no Undo when the applied batch cannot be undone", async () => {
      mocked.applyChanges.mockResolvedValue(batch({ canUndo: false }));
      renderTray();

      await userEvent.click(applyButton());

      expect(screen.getByRole("status")).toHaveTextContent("Applied 1 change");
      expect(queryUndoButton()).not.toBeInTheDocument();
    });

    it("applies one batch and shows one toast when Apply is clicked twice while the call is pending", async () => {
      const call = deferred<AppliedBatch>();
      mocked.applyChanges.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(applyButton());
      await userEvent.click(applyButton());
      await act(async () => call.resolve(batch()));

      expect(mocked.applyChanges).toHaveBeenCalledTimes(1);
      expect(mocked.applyChanges).toHaveBeenCalledWith("s1", [1]);
      expect(screen.getAllByRole("status")).toHaveLength(1);
      expect(screen.getByRole("status")).toHaveTextContent("Applied 1 change");
      expect(screen.getByRole("status")).not.toHaveTextContent("×");
    });

    it("disables Apply and Undo as busy while applying and enables them after it resolves", async () => {
      useStore.setState({ applied: [{ sessionId: "s1", batchId: 9, canUndo: true }] });
      const call = deferred<AppliedBatch>();
      mocked.applyChanges.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(applyButton());
      expect(applyButton()).toBeDisabled();
      expect(applyButton()).toHaveAttribute("aria-busy", "true");
      expect(undoButton()).toBeDisabled();
      expect(undoButton()).toHaveAttribute("aria-busy", "true");

      await act(async () => call.resolve(batch({ batchId: 10 })));
      expect(screen.getByText("No pending changes")).toBeInTheDocument();
      expect(undoButton()).toBeEnabled();
      expect(undoButton()).toHaveAttribute("aria-busy", "false");
    });

    it("keeps the items pending, enables Apply again and reports the error when the apply is rejected", async () => {
      const call = deferred<AppliedBatch>();
      mocked.applyChanges.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(applyButton());
      expect(applyButton()).toBeDisabled();
      await act(async () => call.reject({ kind: "query_failed", message: "Deadlock", code: null }));

      expect(applyButton()).toBeEnabled();
      expect(applyButton()).toHaveAttribute("aria-busy", "false");
      expect(screen.getByText("1 pending change")).toBeInTheDocument();
      expect(screen.getByRole("alert")).toHaveTextContent("Changes were not applied: Deadlock");
      expect(queryUndoButton()).not.toBeInTheDocument();
    });

    it("applies once when ⌘S is pressed twice while the call is pending", async () => {
      const call = deferred<AppliedBatch>();
      mocked.applyChanges.mockReturnValue(call.promise);
      render(<App />);

      await userEvent.keyboard("{Meta>}s{/Meta}");
      await userEvent.keyboard("{Meta>}s{/Meta}");
      await act(async () => call.resolve(batch()));

      expect(mocked.applyChanges).toHaveBeenCalledTimes(1);
      expect(screen.getAllByText("Applied 1 change")).toHaveLength(1);
    });
  });

  describe("discarding", () => {
    beforeEach(() =>
      useStore.setState({
        pending: [pendingChange({ id: 4, description: "First" }), pendingChange({ id: 5, description: "Second" })],
      }),
    );

    it("discards one change on the server by id and keeps the others", async () => {
      renderTray();

      await userEvent.click(screen.getByRole("button", { name: "Discard First" }));

      expect(mocked.discardChange).toHaveBeenCalledWith("s1", 4);
      expect(screen.getByText("1 pending change")).toBeInTheDocument();
      expect(screen.queryByText("First")).not.toBeInTheDocument();
      expect(screen.getByText("Second")).toBeInTheDocument();
    });

    it("keeps the change and reports the error when the discard is rejected", async () => {
      mocked.discardChange.mockRejectedValue({ kind: "not_found", message: "Unknown change", code: null });
      renderTray();

      await userEvent.click(screen.getByRole("button", { name: "Discard First" }));

      expect(screen.getByText("First")).toBeInTheDocument();
      expect(screen.getByText("2 pending changes")).toBeInTheDocument();
      expect(screen.getByRole("alert")).toHaveTextContent("Cannot discard change: Unknown change");
    });

    it("discards every change of the active session with Discard all", async () => {
      useStore.setState((s) => ({ pending: [...s.pending, pendingChange({ id: 6, description: "Other", sessionId: "s2" })] }));
      renderTray();

      await userEvent.click(screen.getByRole("button", { name: "Discard all" }));

      expect(mocked.discardChange).toHaveBeenCalledWith("s1", 4);
      expect(mocked.discardChange).toHaveBeenCalledWith("s1", 5);
      expect(mocked.discardChange).not.toHaveBeenCalledWith("s2", 6);
      expect(useStore.getState().pending.map((p) => p.id)).toEqual([6]);
    });
  });

  describe("undoing", () => {
    beforeEach(() => useStore.setState({ applied: [{ sessionId: "s1", batchId: 7, canUndo: true }] }));

    it("undoes the batch id returned by the apply", async () => {
      useStore.setState({ pending: [pendingChange()], applied: [] });
      mocked.applyChanges.mockResolvedValue(batch({ batchId: 42 }));
      renderTray();

      await userEvent.click(applyButton());
      await userEvent.click(await screen.findByRole("button", { name: /Undo last apply/ }));

      expect(mocked.undoBatch).toHaveBeenCalledWith("s1", 42);
    });

    it("undoes the newest batch of the active session only", async () => {
      useStore.setState({
        applied: [
          { sessionId: "s1", batchId: 7, canUndo: true },
          { sessionId: "s2", batchId: 8, canUndo: true },
        ],
      });
      renderTray();

      await userEvent.click(undoButton());

      expect(mocked.undoBatch).toHaveBeenCalledWith("s1", 7);
      expect(screen.getByRole("status")).toHaveTextContent("Undone");
    });

    it("undoes once when Undo is clicked twice while the call is pending and blocks Apply meanwhile", async () => {
      useStore.setState({ pending: [pendingChange({ id: 2, description: "Another" })] });
      const call = deferred<number>();
      mocked.undoBatch.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(undoButton());
      await userEvent.click(undoButton());
      expect(undoButton()).toBeDisabled();
      expect(undoButton()).toHaveAttribute("aria-busy", "true");
      expect(applyButton()).toBeDisabled();
      await act(async () => call.resolve(1));

      expect(mocked.undoBatch).toHaveBeenCalledTimes(1);
      expect(mocked.undoBatch).toHaveBeenCalledWith("s1", 7);
      expect(screen.getByRole("status")).toHaveTextContent("Undone");
      expect(applyButton()).toBeEnabled();
      expect(queryUndoButton()).not.toBeInTheDocument();
    });

    it("keeps the undo available and enables the button again when the undo is rejected", async () => {
      const call = deferred<number>();
      mocked.undoBatch.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(undoButton());
      await act(async () => call.reject({ kind: "query_failed", message: "Gone", code: null }));

      expect(undoButton()).toBeEnabled();
      expect(screen.getByRole("alert")).toHaveTextContent("Undo failed: Gone");
    });
  });

  describe("dismissing the undo", () => {
    const dismissButton = () => screen.getByRole("button", { name: "Dismiss undo" });

    beforeEach(() => {
      installCommands();
      useStore.setState({ applied: [{ sessionId: "s1", batchId: 7, canUndo: true }] });
    });

    it("hides the tray when the × is clicked and nothing is pending", async () => {
      renderTray();

      await userEvent.click(dismissButton());

      expect(screen.queryByRole("region", { name: "Pending changes" })).not.toBeInTheDocument();
      expect(queryUndoButton()).not.toBeInTheDocument();
    });

    it("still undoes the dismissed batch through the changes.undo command", async () => {
      renderTray();
      await userEvent.click(dismissButton());

      await act(async () => {
        runCommand("changes.undo");
      });

      expect(mocked.undoBatch).toHaveBeenCalledWith("s1", 7);
    });

    it("brings the undo back after the next apply", async () => {
      renderTray();
      await userEvent.click(dismissButton());
      act(() => useStore.setState({ pending: [pendingChange({ id: 2 })] }));

      await userEvent.click(applyButton());

      expect(await screen.findByText("No pending changes")).toBeInTheDocument();
      expect(undoButton()).toBeEnabled();
      expect(screen.getByRole("button", { name: "Dismiss undo" })).toBeInTheDocument();
    });

    it("shows no × while changes are pending", () => {
      useStore.setState({ pending: [pendingChange()] });
      renderTray();

      expect(undoButton()).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Dismiss undo" })).not.toBeInTheDocument();
    });

    it("shows no × when there is nothing to undo", () => {
      useStore.setState({ applied: [], pending: [pendingChange()] });
      renderTray();

      expect(screen.queryByRole("button", { name: "Dismiss undo" })).not.toBeInTheDocument();
    });
  });
});
