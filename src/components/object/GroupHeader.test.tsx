import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import type { Group } from "../../api";
import { useStore } from "../../state/store";
import { connectedStore } from "../../test/fixtures";
import { showTableIcons } from "../../test/iconFixtures";
import { GroupHeader } from "./GroupHeader";

const group = (patch: Partial<Group> = {}): Group => ({
  id: "g",
  table: "wp_postmeta",
  via: { kind: "own" },
  shape: "meta",
  columns: [],
  total: 3,
  rows: [],
  capped: false,
  rowsAddress: null,
  error: null,
  ...patch,
});

const header = (g: Group) => render(<GroupHeader group={g} database="db" collapsed={false} onToggle={() => {}} />);

describe("GroupHeader", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
  });

  it("draws the table's icon and a go hint inside the button that opens its rows", () => {
    showTableIcons({ wp_postmeta: "tags" });
    header(group());
    const button = screen.getByRole("button", { name: "wp_postmeta" });
    const swap = button.querySelector(".open-swap");
    expect(swap).toHaveAttribute("data-open", "go");
    expect(swap?.querySelector("[data-icon=tags]")).not.toBeNull();
  });

  it("draws the default table icon for a table without one", () => {
    showTableIcons({});
    header(group());
    const swap = screen.getByRole("button", { name: "wp_postmeta" }).querySelector(".open-swap");
    expect(swap?.querySelector("[data-icon=table]")).not.toBeNull();
  });

  it("opens the rows in a table tab when the table name is clicked", async () => {
    showTableIcons({});
    header(group());
    await userEvent.click(screen.getByRole("button", { name: "wp_postmeta" }));
    expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "table", table: "wp_postmeta" });
  });

  it("shows the events group as plain text without icon or hint", () => {
    showTableIcons({ wp_postmeta: "tags" });
    const { container } = header(group({ via: { kind: "events" } }));
    expect(screen.getByText("events")).toBeInTheDocument();
    expect(container.querySelector("[data-icon], [data-open]")).toBeNull();
  });
});
