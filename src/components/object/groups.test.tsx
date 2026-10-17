import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef, type ReactNode } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import type { Group } from "../../api";
import { NO_HOVER, useStore } from "../../state/store";
import { text, value } from "../../test/fixtures";
import { useValueHover } from "../value/useValueHover";
import { EventsGroup, FieldsGroup, MetaGroup, RowsGroup } from "./groups";

const marks = { focus: null, editing: null };

const stored = (table: string, raw: string) => value({ raw: text(raw), source: { table, column: "c", row: [{ column: "ID", value: "1" }] } });

const group = (patch: Partial<Group>): Group => ({
  id: "g",
  table: "wp_posts",
  via: { kind: "own" },
  shape: "fields",
  columns: [],
  total: 1,
  rows: [],
  capped: false,
  rowsAddress: null,
  error: null,
  ...patch,
});

function Area({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useValueHover(ref);
  return <div ref={ref}>{children}</div>;
}

const hoverRow = async (label: string) => {
  await userEvent.hover(screen.getByText(label));
  await waitFor(() => expect(useStore.getState().hover.table).not.toBeNull());
};

describe("Document group row hover", () => {
  beforeEach(() => useStore.setState({ hover: NO_HOVER }));

  it("names the group's table when the label of a field row is hovered", async () => {
    const own = group({ rows: [{ key: null, fields: [["post_title", stored("wp_posts", "Hello")]] }] });
    render(
      <Area>
        <FieldsGroup group={own} marks={marks} />
      </Area>,
    );
    await hoverRow("post_title");
    expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false });
  });

  it("names the meta table when the key of a meta row is hovered", async () => {
    const meta = group({
      table: "wp_postmeta",
      shape: "meta",
      rows: [{ key: null, fields: [["meta_key", value({ raw: text("_edit_lock") })], ["meta_value", stored("wp_postmeta", "123")]] }],
    });
    render(
      <Area>
        <MetaGroup group={meta} marks={marks} />
      </Area>,
    );
    await hoverRow("_edit_lock");
    expect(useStore.getState().hover).toEqual({ src: null, table: "wp_postmeta", linked: false });
  });

  it("names the table of a hovered row value and none for the header row", async () => {
    const rows = group({
      table: "wp_comments",
      shape: "rows",
      columns: ["comment_ID"],
      rows: [{ key: null, fields: [["comment_ID", stored("wp_comments", "9")]] }],
    });
    render(
      <Area>
        <RowsGroup group={rows} marks={marks} />
      </Area>,
    );
    await hoverRow("9");
    expect(useStore.getState().hover).toMatchObject({ table: "wp_comments", linked: false });
    await userEvent.hover(screen.getByText("comment_ID"));
    await waitFor(() => expect(useStore.getState().hover).toEqual(NO_HOVER));
  });

  it("names the table a timestamp is stored in on an events row", async () => {
    const events = group({ table: "", rows: [{ key: null, fields: [["post_date", stored("wp_posts", "2025-01-02 10:00:00")]] }] });
    render(
      <Area>
        <EventsGroup group={events} marks={marks} />
      </Area>,
    );
    await userEvent.hover(screen.getByText("post_date"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false }));
  });
});
