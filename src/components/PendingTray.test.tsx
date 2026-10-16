import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api, type PlannedChange } from "../api";
import { useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { PendingTray } from "./PendingTray";
import { Toasts } from "./Toasts";

const mocked = vi.mocked(api, true);

const planned = (patch: Partial<PlannedChange> = {}): PlannedChange => ({
  description: "Set post_title",
  statements: ["UPDATE wp_posts SET post_title = 'x'"],
  undo: ["UPDATE wp_posts SET post_title = 'y'"],
  ...patch,
});

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

describe("PendingTray", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.applyChanges.mockReset().mockResolvedValue(1);
    connectedStore();
  });

  describe("applying", () => {
    beforeEach(() => useStore.setState({ pending: [{ ...planned(), id: 1, sessionId: "s1" }] }));

    it("applies one batch and shows one toast when Apply is clicked twice while the call is pending", async () => {
      const call = deferred<number>();
      mocked.applyChanges.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(applyButton());
      await userEvent.click(applyButton());
      await act(async () => call.resolve(1));

      expect(mocked.applyChanges).toHaveBeenCalledTimes(1);
      expect(mocked.applyChanges).toHaveBeenCalledWith("s1", ["UPDATE wp_posts SET post_title = 'x'"]);
      expect(screen.getAllByRole("status")).toHaveLength(1);
      expect(screen.getByRole("status")).toHaveTextContent("Applied 1 change");
      expect(screen.getByRole("status")).not.toHaveTextContent("×");
    });

    it("disables Apply and Undo as busy while applying and enables them after it resolves", async () => {
      useStore.setState({ applied: [{ sessionId: "s1", changes: [planned()] }] });
      const call = deferred<number>();
      mocked.applyChanges.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(applyButton());
      expect(applyButton()).toBeDisabled();
      expect(applyButton()).toHaveAttribute("aria-busy", "true");
      expect(undoButton()).toBeDisabled();
      expect(undoButton()).toHaveAttribute("aria-busy", "true");

      await act(async () => call.resolve(1));
      expect(screen.getByText("No pending changes")).toBeInTheDocument();
      expect(undoButton()).toBeEnabled();
      expect(undoButton()).toHaveAttribute("aria-busy", "false");
    });

    it("enables Apply again and reports the error when the apply is rejected", async () => {
      const call = deferred<number>();
      mocked.applyChanges.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(applyButton());
      expect(applyButton()).toBeDisabled();
      await act(async () => call.reject({ kind: "query_failed", message: "Deadlock", code: null }));

      expect(applyButton()).toBeEnabled();
      expect(applyButton()).toHaveAttribute("aria-busy", "false");
      expect(screen.getByText("1 pending change")).toBeInTheDocument();
      expect(screen.getByRole("alert")).toHaveTextContent("Changes were not applied: Deadlock");
    });

    it("applies once when ⌘S is pressed twice while the call is pending", async () => {
      const call = deferred<number>();
      mocked.applyChanges.mockReturnValue(call.promise);
      render(<App />);

      await userEvent.keyboard("{Meta>}s{/Meta}");
      await userEvent.keyboard("{Meta>}s{/Meta}");
      await act(async () => call.resolve(1));

      expect(mocked.applyChanges).toHaveBeenCalledTimes(1);
      expect(screen.getAllByText("Applied 1 change")).toHaveLength(1);
    });
  });

  describe("undoing", () => {
    beforeEach(() => useStore.setState({ applied: [{ sessionId: "s1", changes: [planned()] }] }));

    it("undoes once when Undo is clicked twice while the call is pending and blocks Apply meanwhile", async () => {
      useStore.setState({ pending: [{ ...planned({ description: "Another" }), id: 2, sessionId: "s1" }] });
      const call = deferred<number>();
      mocked.applyChanges.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(undoButton());
      await userEvent.click(undoButton());
      expect(undoButton()).toBeDisabled();
      expect(undoButton()).toHaveAttribute("aria-busy", "true");
      expect(applyButton()).toBeDisabled();
      await act(async () => call.resolve(1));

      expect(mocked.applyChanges).toHaveBeenCalledTimes(1);
      expect(mocked.applyChanges).toHaveBeenCalledWith("s1", ["UPDATE wp_posts SET post_title = 'y'"]);
      expect(screen.getByRole("status")).toHaveTextContent("Undone");
      expect(applyButton()).toBeEnabled();
      expect(screen.queryByRole("button", { name: /Undo last apply/ })).not.toBeInTheDocument();
    });

    it("keeps the undo available and enables the button again when the undo is rejected", async () => {
      const call = deferred<number>();
      mocked.applyChanges.mockReturnValue(call.promise);
      renderTray();

      await userEvent.click(undoButton());
      await act(async () => call.reject({ kind: "query_failed", message: "Gone", code: null }));

      expect(undoButton()).toBeEnabled();
      expect(screen.getByRole("alert")).toHaveTextContent("Undo failed: Gone");
    });
  });
});
