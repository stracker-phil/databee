import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { ErrorBoundary } from "./ErrorBoundary";

function Broken(): never {
  throw new Error("kaboom");
}

describe("ErrorBoundary", () => {
  it("shows a recoverable fallback and logs the crash", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <Broken />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("kaboom");
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(vi.mocked(api).logError).toHaveBeenCalledWith(expect.stringContaining("kaboom"));
  });
});
