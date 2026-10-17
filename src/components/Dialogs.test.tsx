import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type ColumnInfo, type Home } from "../api";
import { useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { Dialogs } from "./Dialogs";

const mocked = vi.mocked(api, true);

const column = (name: string, patch: Partial<ColumnInfo> = {}): ColumnInfo => ({
  name,
  columnType: "text",
  nullable: false,
  default: null,
  key: "",
  extra: "",
  comment: "",
  autoIncrement: false,
  generated: false,
  ...patch,
});

const columns = [
  column("ID", { columnType: "bigint unsigned", key: "PRI", extra: "auto_increment", autoIncrement: true }),
  column("post_title"),
  column("post_excerpt", { nullable: true }),
  column("post_status", { columnType: "varchar(20)", default: "draft" }),
  column("slug_hash", { columnType: "char(32)", extra: "STORED GENERATED", generated: true }),
];

const planned = (values: [string, string | null][]) => ({ type: "insertRow", database: "db", table: "wp_posts", values });

describe("InsertRowDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    useStore.getState().openDialog({ type: "insertRow", database: "db", table: "wp_posts", columns });
    render(<Dialogs />);
  });

  const field = (name: string) => screen.getByRole("textbox", { name: new RegExp(`^${name}\\b`) });
  const mode = (name: string) => screen.getByRole("combobox", { name: `${name} mode` });
  const queue = () => userEvent.click(screen.getByRole("button", { name: "Queue insert" }));

  it("does not show auto-increment or generated columns", () => {
    expect(screen.getByRole("heading", { name: "Insert into wp_posts" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: /^post_title/ })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /^ID\b/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /^slug_hash/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "ID mode" })).not.toBeInTheDocument();
  });

  it("shows a column's default as the placeholder in Default mode", () => {
    expect(field("post_status")).toHaveAttribute("placeholder", "draft");
    expect(field("post_title")).toHaveAttribute("placeholder", "default");
  });

  it("offers NULL only for nullable columns", () => {
    expect(within(mode("post_excerpt")).getByRole("option", { name: "NULL" })).toBeInTheDocument();
    expect(within(mode("post_title")).queryByRole("option", { name: "NULL" })).not.toBeInTheDocument();
  });

  it("leaves Default columns out of the INSERT and closes after queueing", async () => {
    await queue();
    expect(mocked.planChange).toHaveBeenCalledWith("s1", planned([]));
    await vi.waitFor(() => expect(useStore.getState().dialog).toBeNull());
    expect(useStore.getState().pending).toHaveLength(1);
  });

  it("sends the text NULL as a string when it is typed into a value", async () => {
    await userEvent.type(field("post_title"), "NULL");
    await queue();
    expect(mocked.planChange).toHaveBeenCalledWith("s1", planned([["post_title", "NULL"]]));
  });

  it("sends SQL NULL when the column is set to NULL mode", async () => {
    await userEvent.selectOptions(mode("post_excerpt"), "NULL");
    expect(field("post_excerpt")).toHaveValue("");
    await queue();
    expect(mocked.planChange).toHaveBeenCalledWith("s1", planned([["post_excerpt", null]]));
  });

  it("sends an empty string for Value mode with nothing typed", async () => {
    await userEvent.selectOptions(mode("post_title"), "Value");
    await queue();
    expect(mocked.planChange).toHaveBeenCalledWith("s1", planned([["post_title", ""]]));
  });

  it("sends typed values together with the other modes in column order", async () => {
    await userEvent.type(field("post_title"), "Hello");
    await userEvent.selectOptions(mode("post_excerpt"), "NULL");
    await userEvent.type(field("post_status"), "publish");
    await queue();
    expect(mocked.planChange).toHaveBeenCalledWith("s1", planned([["post_title", "Hello"], ["post_excerpt", null], ["post_status", "publish"]]));
  });

  it("keeps the dialog open and shows an error when the change cannot be planned", async () => {
    mocked.planChange.mockRejectedValueOnce({ kind: "invalid_input", message: "Column post_title cannot be NULL", code: null });
    await queue();
    expect(useStore.getState().dialog).toMatchObject({ type: "insertRow" });
    expect(useStore.getState().toasts.at(-1)?.message).toContain("Column post_title cannot be NULL");
  });
});

describe("PinDialog", () => {
  const onSubmit = vi.fn();
  const open = (pin: Home | null) => {
    connectedStore();
    useStore.getState().openDialog({ type: "pin", title: "Failed orders", pin, onSubmit });
    render(<Dialogs />);
  };
  const save = () => userEvent.click(screen.getByRole("button", { name: "Save" }));

  beforeEach(() => vi.clearAllMocks());

  it("starts on the current pin", () => {
    open({ show: "rows", limit: 12 });
    expect(screen.getByRole("heading", { name: "Pin Failed orders" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Rows" })).toBeChecked();
    expect(screen.getByLabelText("Rows to show")).toHaveValue(12);
  });

  it("starts on Value for a value pin and keeps it on Save", async () => {
    open({ show: "value" });
    expect(screen.getByRole("radio", { name: "Value" })).toBeChecked();
    await save();
    expect(onSubmit).toHaveBeenCalledWith({ show: "value" });
  });

  it("starts unpinned for a query without a pin", () => {
    open(null);
    expect(screen.getByRole("radio", { name: "Not pinned" })).toBeChecked();
    expect(screen.getByLabelText("Rows to show")).toBeDisabled();
  });

  it("submits null when the pin is removed", async () => {
    open({ show: "count" });
    await userEvent.click(screen.getByRole("radio", { name: "Not pinned" }));
    await save();
    expect(onSubmit).toHaveBeenCalledWith(null);
    expect(useStore.getState().dialog).toBeNull();
  });

  it("submits the default of five rows when Rows is chosen without a limit", async () => {
    open(null);
    await userEvent.click(screen.getByRole("radio", { name: "Rows" }));
    await save();
    expect(onSubmit).toHaveBeenCalledWith({ show: "rows", limit: 5 });
  });

  it.each(["999", "0", "-3"])("does not submit a limit of %s, outside 1 to 255", async (typed) => {
    open(null);
    await userEvent.click(screen.getByRole("radio", { name: "Rows" }));
    const limit = screen.getByLabelText("Rows to show");
    await userEvent.clear(limit);
    await userEvent.type(limit, typed);
    await save();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(useStore.getState().dialog).not.toBeNull();
  });

  it("does not submit when cancelled", async () => {
    open(null);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(useStore.getState().dialog).toBeNull();
  });
});
