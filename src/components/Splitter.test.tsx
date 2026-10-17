import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api } from "../api";
import { resetStore } from "../state/store";
import { connectedStore, session } from "../test/fixtures";

const mocked = vi.mocked(api, true);

function drag(handle: HTMLElement, from: { x?: number; y?: number }, to: { x?: number; y?: number }) {
  fireEvent.mouseDown(handle, { button: 0, clientX: from.x ?? 0, clientY: from.y ?? 0 });
  fireEvent.mouseMove(window, { clientX: to.x ?? 0, clientY: to.y ?? 0 });
  fireEvent.mouseUp(window, { clientX: to.x ?? 0, clientY: to.y ?? 0 });
}

describe("resizable panes", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
    window.innerWidth = 1600;
    window.innerHeight = 1000;
  });

  it("resizes the SQL editor against the results, also with the keyboard", async () => {
    connectedStore();
    render(<App />);
    const handle = screen.getByRole("separator", { name: "Resize editor" });
    expect(handle).toHaveAttribute("aria-valuenow", "140");
    drag(handle, { y: 300 }, { y: 220 });
    expect(handle).toHaveAttribute("aria-valuenow", "60");
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    await vi.waitFor(() => expect(mocked.setUiState).toHaveBeenLastCalledWith(session.identity, expect.objectContaining({ sqlEditorHeight: 76 })), { timeout: 1500 });
  });
});
