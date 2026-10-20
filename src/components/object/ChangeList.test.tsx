import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { noMarks } from "../../lib/diff";
import { connectedStore, text } from "../../test/fixtures";
import { source } from "../../test/objectFixtures";
import { ChangeList } from "./ChangeList";

describe("ChangeList", () => {
  beforeEach(() => connectedStore());

  it("draws the header and the change lines inside one scroll box that is not pinned", () => {
    const marks = {
      ...noMarks({ kind: "live" }),
      lines: [{ at: 0, kind: "changed" as const, table: "wp_wc_orders", label: "status", old: text("wc-pending"), new: text("wc-processing"), source: source("wp_wc_orders", "status", "4711") }],
    };
    render(<ChangeList marks={marks} dismissCommand="object.dismissChanges" onJump={vi.fn()} />);
    const box = screen.getByText("old → new").closest<HTMLElement>(".wide-table");
    expect(box).not.toBeNull();
    expect(box).not.toHaveClass("pinned");
    expect(box).toContainElement(screen.getByText("wc-pending"));
    expect(box).toContainElement(screen.getByText("status"));
  });

  it("renders nothing without change lines", () => {
    render(<ChangeList marks={noMarks({ kind: "live" })} dismissCommand="object.dismissChanges" onJump={vi.fn()} />);
    expect(screen.queryByRole("region", { name: "Changes" })).not.toBeInTheDocument();
  });
});
