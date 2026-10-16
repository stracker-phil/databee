import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { installCommands } from "../commands";
import { connectedStore } from "../test/fixtures";
import { CommandPalette } from "./CommandPalette";
import { Toolbar } from "./Toolbar";

describe("Toolbar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    installCommands();
  });

  it("opens the command palette from the Go to field", async () => {
    render(
      <>
        <Toolbar />
        <CommandPalette />
      </>,
    );
    expect(screen.queryByLabelText("Search everything")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Go to" }));
    expect(await screen.findByLabelText("Search everything")).toBeInTheDocument();
  });

  it("shows the palette shortcut as a key cap", () => {
    render(<Toolbar />);
    expect(screen.getByRole("button", { name: "Go to" })).toHaveTextContent("⌘K");
  });
});
