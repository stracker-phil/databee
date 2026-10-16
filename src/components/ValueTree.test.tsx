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
