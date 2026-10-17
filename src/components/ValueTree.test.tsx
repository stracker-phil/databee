import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { DecodedValue, ValueChild, ValueNode } from "../api";
import { ValueTree } from "./ValueTree";

const str = (text: string): ValueNode => ({ kind: "string", text });
const decoded = (root: ValueNode): DecodedValue => ({ format: "php", root, warning: null, error: null });

describe("ValueTree editing", () => {
  it("passes the child index path of an edited leaf when display keys repeat", async () => {
    const onEdit = vi.fn();
    const children: ValueChild[] = [
      { key: "bar", keyKind: "protected", node: str("first") },
      { key: "bar", keyKind: "public", node: str("second") },
    ];
    render(<ValueTree value={decoded({ kind: "object", class: "Foo", children })} onEdit={onEdit} />);
    await userEvent.click(screen.getAllByRole("button", { name: "Edit bar" })[1]);
    await userEvent.type(screen.getByLabelText("New value"), "{Control>}a{/Control}x{Enter}");
    expect(onEdit).toHaveBeenCalledWith([1], { kind: "string", text: "x" });
  });

  it("passes the full index path of a nested leaf and labels it by display keys", async () => {
    const onEdit = vi.fn();
    const root: ValueNode = {
      kind: "array",
      children: [
        {
          key: "a",
          keyKind: "string",
          node: {
            kind: "array",
            children: [
              { key: "x", keyKind: "string", node: str("one") },
              { key: "b", keyKind: "string", node: str("two") },
            ],
          },
        },
      ],
    };
    render(<ValueTree value={decoded(root)} onEdit={onEdit} />);
    await userEvent.click(screen.getByRole("button", { name: "Edit a/b" }));
    await userEvent.type(screen.getByLabelText("New value"), "{Control>}a{/Control}y{Enter}");
    expect(onEdit).toHaveBeenCalledWith([0, 1], { kind: "string", text: "y" });
  });

  it("offers no edit buttons without an edit handler", () => {
    render(<ValueTree value={decoded({ kind: "array", children: [{ key: "a", keyKind: "string", node: str("v") }] })} />);
    expect(screen.queryByRole("button", { name: "Edit a" })).not.toBeInTheDocument();
  });
});

describe("ValueTree leaves", () => {
  it("dims the quotes around a string, so it differs from a number", () => {
    render(<ValueTree value={decoded(str("2.04"))} />);
    const quotes = screen.getAllByText('"');
    expect(quotes).toHaveLength(2);
    for (const quote of quotes) expect(quote).toHaveClass("vt-quote");
  });

  it("shows a number without quotes", () => {
    render(<ValueTree value={decoded({ kind: "float", text: "2.04" })} />);
    expect(screen.getByText("2.04")).toBeInTheDocument();
    expect(screen.queryByText('"')).not.toBeInTheDocument();
  });
});

describe("ValueTree small collections", () => {
  const leaf = (key: string, text: string): ValueChild => ({ key, keyKind: "string", node: str(text) });
  const small: ValueNode = { kind: "array", children: [leaf("a", "1"), leaf("b", "2")] };

  it("shows the leaf pairs of a small array on one line", () => {
    render(<ValueTree value={decoded(small)} />);
    expect(screen.getAllByRole("treeitem")).toHaveLength(1);
    expect(screen.getByText("array(2)")).toBeInTheDocument();
    expect(screen.getByText("a")).toBeInTheDocument();
    expect(screen.getByText("b")).toBeInTheDocument();
  });

  it("keeps one row per leaf when the tree is editable", () => {
    render(<ValueTree value={decoded(small)} onEdit={vi.fn()} />);
    expect(screen.getAllByRole("treeitem")).toHaveLength(3);
    expect(screen.getByRole("button", { name: "Edit a" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit b" })).toBeInTheDocument();
  });

  it("keeps one row per entry for an array with more than four entries", () => {
    const many: ValueNode = { kind: "array", children: ["a", "b", "c", "d", "e"].map((k) => leaf(k, "x")) };
    render(<ValueTree value={decoded(many)} />);
    expect(screen.getAllByRole("treeitem")).toHaveLength(6);
  });

  it("keeps rows for an array that holds another array", () => {
    const nested: ValueNode = { kind: "array", children: [{ key: "inner", keyKind: "string", node: small }] };
    render(<ValueTree value={decoded(nested)} />);
    expect(screen.getAllByRole("treeitem")).toHaveLength(2);
  });
});

describe("ValueTree keys", () => {
  it("shows the declaring class next to a private property", () => {
    const root: ValueNode = { kind: "object", class: "Bar", children: [{ key: "secret", keyKind: "private", declaringClass: "Foo", node: str("v") }] };
    render(<ValueTree value={decoded(root)} />);
    expect(screen.getByText("secret")).toBeInTheDocument();
    expect(screen.getByText("private Foo")).toBeInTheDocument();
  });

  it("shows plain private and protected markers without a class", () => {
    const root: ValueNode = {
      kind: "object",
      class: "Foo",
      children: [
        { key: "a", keyKind: "private", node: str("1") },
        { key: "b", keyKind: "protected", node: str("2") },
      ],
    };
    render(<ValueTree value={decoded(root)} />);
    expect(screen.getByText("private")).toBeInTheDocument();
    expect(screen.getByText("protected")).toBeInTheDocument();
  });

  it("shows the raw key bytes as a hex hint for non-UTF-8 keys", () => {
    const root: ValueNode = { kind: "array", children: [{ key: "�", keyKind: "string", rawHex: "ff", node: str("v") }] };
    render(<ValueTree value={decoded(root)} />);
    expect(screen.getByText("0xff")).toBeInTheDocument();
  });
});
