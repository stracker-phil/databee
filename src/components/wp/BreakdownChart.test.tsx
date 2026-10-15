import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { BreakdownChart, splitRows, type BreakdownItem } from "./BreakdownChart";

const items: BreakdownItem[] = Array.from({ length: 12 }, (_, i) => ({ id: `p${i}`, label: `Plugin ${i}`, value: 1200 - i * 100, onOpen: vi.fn() }));
const kb = (v: number) => `${(v / 1000).toFixed(1)} KB`;

describe("BreakdownChart", () => {
  it("shows at most seven named rows and folds the rest into Other", () => {
    expect(splitRows(items, 7).map((r) => r.label)).toEqual(["Plugin 0", "Plugin 1", "Plugin 2", "Plugin 3", "Plugin 4", "Plugin 5", "Plugin 6", "Other (5)"]);
    expect(splitRows(items.slice(0, 3), 7)).toHaveLength(3);
    render(<BreakdownChart items={items} format={kb} totalLabel="autoloaded" />);
    expect(screen.getAllByRole("listitem")).toHaveLength(8);
    expect(screen.getByRole("button", { name: /Other \(5\)/ })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "7.8 KB autoloaded" })).toBeInTheDocument();
  });

  it("drills into Other and climbs back with the crumb", async () => {
    render(<BreakdownChart items={items} format={kb} totalLabel="autoloaded" />);
    await userEvent.click(screen.getByRole("button", { name: /Other \(5\)/ }));
    expect(screen.getByRole("navigation", { name: "Breakdown level" })).toHaveTextContent("AllOther");
    expect(screen.getAllByRole("listitem").map((li) => li.textContent)).toEqual(expect.arrayContaining([expect.stringContaining("Plugin 11")]));
    expect(screen.queryByText("Plugin 0")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "All" }));
    expect(screen.getByText("Plugin 0")).toBeInTheDocument();
  });

  it("links hover between slice and row, and opens a row with Enter", async () => {
    render(<BreakdownChart items={items.slice(0, 3)} format={kb} totalLabel="autoloaded" />);
    const row = screen.getByRole("button", { name: "Plugin 1: 1.1 KB" });
    fireEvent.mouseEnter(row);
    const slice = document.querySelector('path[data-id="p1"]')!;
    expect(slice.classList.contains("hot")).toBe(true);
    expect(document.querySelector('path[data-id="p0"]')!.classList.contains("dim")).toBe(true);
    expect(screen.getByRole("img").textContent).toContain("1.1 KB");
    fireEvent.mouseLeave(row);
    row.focus();
    await userEvent.keyboard("{Enter}");
    expect(items[1].onOpen).toHaveBeenCalled();
  });

  it("offers a segmented mode toggle", async () => {
    const onModeChange = vi.fn();
    render(<BreakdownChart items={items} format={kb} totalLabel="rows" modes={[{ id: "autoload", label: "Autoload" }, { id: "rows", label: "Table rows" }]} mode="autoload" onModeChange={onModeChange} />);
    const group = screen.getByRole("group", { name: "Breakdown mode" });
    expect(within(group).getByRole("button", { name: "Autoload" })).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(within(group).getByRole("button", { name: "Table rows" }));
    expect(onModeChange).toHaveBeenCalledWith("rows");
  });
});
