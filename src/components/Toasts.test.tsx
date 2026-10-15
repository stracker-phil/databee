import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { resetStore, useStore } from "../state/store";
import { Toasts } from "./Toasts";

describe("Toasts", () => {
  beforeEach(() => resetStore());

  it("collapses repeated messages into one toast with a counter", () => {
    render(<Toasts />);
    const { toast } = useStore.getState();
    act(() => {
      toast("error", "Connection lost");
      toast("error", "Connection lost");
      toast("error", "Connection lost");
    });
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByRole("alert")).toHaveTextContent("Something went wrong (×3)");
    expect(screen.getByRole("alert")).toHaveTextContent("Connection lost");
  });

  it("hides the raw remainder of an error behind Details", async () => {
    render(<Toasts />);
    act(() => useStore.getState().toast("error", "Unexpected error: TypeError: x is not a function\n    at Foo (app.js:1)\n    at Bar (app.js:2)"));
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("TypeError: x is not a function");
    expect(alert).not.toHaveTextContent("at Foo");
    await userEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(alert).toHaveTextContent("at Foo (app.js:1)");
  });
});
