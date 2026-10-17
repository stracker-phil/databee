import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LONG_LIST, LongList } from "./LongList";

const items = (count: number) => Array.from({ length: count }, (_, i) => `item ${i}`);

function renderList(count: number) {
  render(
    <LongList items={items(count)}>
      {(item, index) => (
        <div key={index}>
          row {index} {item}
        </div>
      )}
    </LongList>,
  );
}

const rendered = () => screen.queryAllByText(/^row \d+ item \d+$/);

describe("LongList", () => {
  it("renders every row of a list up to the limit", () => {
    renderList(LONG_LIST);
    expect(rendered()).toHaveLength(LONG_LIST);
    expect(screen.getByText("row 199 item 199")).toBeInTheDocument();
  });

  it("renders only the rows near the top of a longer list", () => {
    renderList(1000);
    expect(screen.getByText("row 0 item 0")).toBeInTheDocument();
    expect(screen.queryByText("row 999 item 999")).not.toBeInTheDocument();
    expect(rendered().length).toBeGreaterThan(0);
    expect(rendered().length).toBeLessThan(LONG_LIST);
  });

  it("hands each row its item and its index in the full list", () => {
    renderList(1000);
    expect(screen.getByText("row 0 item 0")).toBeInTheDocument();
    expect(screen.getByText("row 7 item 7")).toBeInTheDocument();
  });
});
