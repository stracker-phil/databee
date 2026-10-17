import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { connectedStore, text, value } from "../../test/fixtures";
import { SummaryLine } from "./SummaryLine";

describe("SummaryLine", () => {
  beforeEach(() => connectedStore({ tabs: [] }));

  it("renders each name followed by its value, in order", () => {
    const { container } = render(
      <SummaryLine
        summary={{
          values: [
            { name: "post_title", value: value({ raw: text("Hello") }) },
            { name: "post_status", value: value({ raw: text("publish") }) },
          ],
        }}
      />,
    );
    expect(container).toHaveTextContent("post_titleHellopost_statuspublish");
  });

  it("shows the null label for a NULL value", () => {
    render(<SummaryLine summary={{ values: [{ name: "post_parent", value: value({ raw: { t: "null" }, kind: { kind: "null" } }) }] }} />);
    expect(screen.getByText("post_parent")).toBeInTheDocument();
    expect(screen.getByText("NULL")).toBeInTheDocument();
  });

  it("renders nothing for an empty summary", () => {
    const { container } = render(<SummaryLine summary={{ values: [] }} />);
    expect(container).toBeEmptyDOMElement();
  });
});
