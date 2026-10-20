import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { NO_HOVER, useStore, type ValueHover } from "../state/store";
import { TableFocus } from "./TableFocus";

const hovering = (patch: Partial<ValueHover>) => useStore.setState({ hover: { ...NO_HOVER, ...patch } });

describe("TableFocus", () => {
  beforeEach(() => useStore.setState({ hover: NO_HOVER }));

  it.each([
    ["nothing is hovered", NO_HOVER],
    ["a value is hovered", { ...NO_HOVER, src: "wp_posts.ID · ID 7", table: "wp_posts", linked: false }],
  ])("renders nothing when %s", (_name, hover) => {
    useStore.setState({ hover });
    render(<TableFocus />);
    expect(screen.queryByTestId("table-focus")).not.toBeInTheDocument();
  });

  it("lights the rows, bands and references of a linked table without dimming the rest", () => {
    hovering({ table: "wp_posts", linked: true, sidebar: false });
    render(<TableFocus />);
    const css = screen.getByTestId("table-focus").textContent;
    expect(css).toContain('[data-row-table="wp_posts"]');
    expect(css).toContain('.sh[data-table="wp_posts"]');
    expect(css).toContain('.val[data-lead-table="wp_posts"]');
    expect(css).not.toContain("var(--dimmed)");
  });

  it("dims everything else when the link is a sidebar row", () => {
    hovering({ table: "wp_posts", linked: true, sidebar: true });
    render(<TableFocus />);
    const css = screen.getByTestId("table-focus").textContent;
    expect(css).toContain('.val[data-lead-table="wp_posts"]');
    expect(css).toContain("var(--dimmed)");
  });

  it("escapes quotes and backslashes in the table name", () => {
    hovering({ table: 'we"ird\\name', linked: true });
    render(<TableFocus />);
    expect(screen.getByTestId("table-focus").textContent).toContain('[data-row-table="we\\"ird\\\\name"]');
  });
});
