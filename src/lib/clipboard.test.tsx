import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { Toasts } from "../components/Toasts";
import { resetStore } from "../state/store";
import { copyText } from "./clipboard";

describe("copyText", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
    render(<Toasts />);
  });

  it("writes the text to the clipboard and confirms what was copied", async () => {
    await act(() => copyText("wp_options", "table name"));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("wp_options");
    expect(screen.getByRole("status")).toHaveTextContent("Copied table name");
  });

  it("reports an error toast when the clipboard rejects the write", async () => {
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error("denied"));
    await act(() => copyText("secret", "value"));
    expect(screen.getByRole("alert")).toHaveTextContent("Clipboard is not available");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
