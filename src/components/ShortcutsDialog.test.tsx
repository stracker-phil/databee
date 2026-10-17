import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { installCommands } from "../commands";
import { openOrder, stubDocuments } from "../test/columnFixtures";
import { connectedStore } from "../test/fixtures";

const mocked = vi.mocked(api, true);
const questionMark = "{Shift>}?{/Shift}";

describe("ShortcutsDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    stubDocuments(mocked);
  });

  it("lists the object commands with their key caps on an object tab and closes on a second press", async () => {
    await openOrder();
    await userEvent.keyboard(questionMark);

    const dialog = await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
    const section = within(dialog).getByRole("region", { name: "Object" });
    const row = within(section).getByText("Focus the next value").parentElement!;
    expect(within(row).getByText("J")).toBeInTheDocument();

    await userEvent.keyboard(questionMark);
    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();
  });

  it("closes with the Close button", async () => {
    await openOrder();
    await userEvent.keyboard(questionMark);
    await userEvent.click(await screen.findByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();
  });
});
