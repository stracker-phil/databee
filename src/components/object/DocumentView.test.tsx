import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi, type Mock } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import type { Document, Group, Provenance } from "../../api";
import { connectedStore } from "../../test/fixtures";
import { objectDocument, orderAddress, orderGroups, paidViaSource } from "../../test/objectFixtures";
import { DocumentView } from "./DocumentView";

const { own, meta, addresses, refunds, items, events } = orderGroups;

function renderDocument(groups: Group[], focus: Provenance | null = null) {
  const document: Document = objectDocument({ groups });
  const view = (focused: Provenance | null) => (
    <DocumentView document={document} owner={orderAddress} focus={focused} onFocus={vi.fn()} onMore={vi.fn()} editable={false} />
  );
  const { container, rerender } = render(view(focus));
  return { container, refocus: (next: Provenance | null) => rerender(view(next)) };
}

const wideTables = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>(".wide-table")];
const boxOf = (label: string) => screen.getByText(label).closest<HTMLElement>(".wide-table");

describe("DocumentView value menu", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("offers Watch on readout before the rule entries on a sourced value", () => {
    renderDocument([meta]);
    fireEvent.contextMenu(screen.getByText("bacs"));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Watch on readout", "Add reference…", "Kind…", "Style…"]);
  });
});

describe("DocumentView wide sections", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  describe("pinned key column", () => {
    it("pins a rows group whose first column is a primary key", () => {
      renderDocument([refunds]);
      expect(boxOf("wc-refunded")).toHaveClass("pinned");
    });

    it.each([
      ["a rows group whose first column is no key", addresses, "Berlin"],
      ["the events group", events, "date_created_gmt"],
    ])("does not pin %s", (_name, group, cell) => {
      renderDocument([group]);
      const box = boxOf(cell);
      expect(box).not.toBeNull();
      expect(box).not.toHaveClass("pinned");
    });

    it("marks a pinned box scrolled sideways and clears the mark at 0", () => {
      const { container } = renderDocument([refunds]);
      const [box] = wideTables(container);
      fireEvent.scroll(box, { target: { scrollLeft: 30 } });
      expect(box).toHaveAttribute("data-scrolled");
      fireEvent.scroll(box, { target: { scrollLeft: 0 } });
      expect(box).not.toHaveAttribute("data-scrolled");
    });

    it("sets no scrolled mark on a box that is not pinned", () => {
      const { container } = renderDocument([addresses]);
      const [box] = wideTables(container);
      fireEvent.scroll(box, { target: { scrollLeft: 30 } });
      expect(box).not.toHaveAttribute("data-scrolled");
    });
  });

  describe("room for the leading icon", () => {
    const plainKeys: Group = {
      ...refunds,
      rows: refunds.rows.map((row) => ({
        ...row,
        fields: row.fields.map(([name, v], i): [string, typeof v] => (i === 0 ? [name, { ...v, kind: { kind: "text" } }] : [name, v])),
      })),
    };
    const firstColumn = (container: HTMLElement) => parseFloat(wideTables(container)[0].style.getPropertyValue("--first-col"));

    it("makes a primary key column wider than a plain column with the same text", () => {
      const keyed = renderDocument([refunds]);
      const withIcon = firstColumn(keyed.container);
      const plain = renderDocument([plainKeys]);
      expect(withIcon - firstColumn(plain.container)).toBe(2.5);
    });
  });

  describe("scroll box contents", () => {
    it("holds the header and the rows of a rows group, but not its band or the more button", () => {
      renderDocument([items]);
      const box = boxOf("Blue mug")!;
      expect(within(box).getByText("order_item_name")).toBeInTheDocument();
      expect(within(box).getByText("Red mug")).toBeInTheDocument();
      const band = screen.getByRole("button", { name: "Collapse wp_wc_order_items" });
      expect(box).not.toContainElement(band);
      expect(box).not.toContainElement(screen.getByRole("button", { name: "+28 rows" }));
    });

    it.each([
      ["a fields group", own, "status"],
      ["a meta group", meta, "_paid_via"],
    ])("keeps the key/value rows of %s outside a scroll box", (_name, group, label) => {
      const { container } = renderDocument([group]);
      expect(screen.getByText(label)).toBeInTheDocument();
      expect(wideTables(container)).toHaveLength(0);
    });
  });

  describe("revealing the focused value", () => {
    const reveal = { block: "nearest", inline: "nearest" };
    const giftNote = { ...paidViaSource, row: [{ column: "id", value: "92" }] };
    let scrollIntoView: Mock<(arg?: boolean | ScrollIntoViewOptions) => void>;

    beforeEach(() => {
      scrollIntoView = vi.fn<(arg?: boolean | ScrollIntoViewOptions) => void>();
      Element.prototype.scrollIntoView = scrollIntoView;
    });

    it("scrolls the focused cell into view now and once more after a frame", async () => {
      const { refocus } = renderDocument([meta]);
      expect(scrollIntoView).not.toHaveBeenCalled();
      refocus(paidViaSource);
      expect(scrollIntoView).toHaveBeenCalledTimes(1);
      expect(scrollIntoView).toHaveBeenCalledWith(reveal);
      expect(scrollIntoView.mock.contexts[0]).toHaveTextContent("bacs");
      await waitFor(() => expect(scrollIntoView).toHaveBeenCalledTimes(2));
      expect(scrollIntoView).toHaveBeenLastCalledWith(reveal);
    });

    it("scrolls to the new cell when the focus changes", async () => {
      const { refocus } = renderDocument([meta], paidViaSource);
      await waitFor(() => expect(scrollIntoView).toHaveBeenCalledTimes(2));
      scrollIntoView.mockClear();
      refocus(giftNote);
      expect(scrollIntoView).toHaveBeenCalledWith(reveal);
      expect(scrollIntoView.mock.contexts[0]).toHaveTextContent("Happy birthday");
    });
  });
});
