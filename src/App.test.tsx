import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App", () => {
  it("shows the disconnected state on launch", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "databee" })).toBeInTheDocument();
    expect(screen.getByText("No connection")).toBeInTheDocument();
  });
});
