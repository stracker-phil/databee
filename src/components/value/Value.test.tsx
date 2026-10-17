import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type Address, type DecodedValue, type Provenance, type ReferenceSource, type Value as StoredValue } from "../../api";
import { address } from "../../lib/address";
import { connectedStore, num, text, value } from "../../test/fixtures";
import { NOW } from "../../test/now";
import { Value } from "./Value";

const mocked = vi.mocked(api, true);

const row = (table: string, key: string): Address => address("shop", "db", { kind: "row", table, key });
const preset: ReferenceSource = { kind: "preset", preset: "woocommerce", id: "order-user" };
const reference = (patch: { by?: ReferenceSource; exists?: boolean | null } = {}): StoredValue =>
  value({ raw: num("12"), kind: { kind: "reference", targets: [row("wp_users", "12")], by: preset, exists: null, ...patch } });
const root = (container: HTMLElement) => container.firstElementChild as HTMLElement;

describe("Value", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("marks the value with its kind", () => {
    const { container } = render(<Value value={value({ raw: num("7"), kind: { kind: "number" } })} />);
    expect(root(container)).toHaveClass("val", "val-number");
    expect(screen.getByText("7")).toHaveClass("val-num");
  });

  it("draws a primary key in its own style", () => {
    render(<Value value={value({ raw: num("7"), kind: { kind: "primaryKey", address: row("wp_posts", "7") } })} />);
    expect(screen.getByText("7")).toHaveClass("val-pk");
  });

  describe("reference", () => {
    it("is a link that opens the value on click", async () => {
      const onOpen = vi.fn();
      const ref = reference();
      render(<Value value={ref} onOpen={onOpen} />);
      await userEvent.click(screen.getByRole("link", { name: "12" }));
      expect(onOpen).toHaveBeenCalledWith(ref, expect.anything());
    });

    it("is plain text without an open handler", () => {
      render(<Value value={reference()} />);
      expect(screen.queryByRole("link")).not.toBeInTheDocument();
      expect(screen.getByText("12")).toHaveClass("val-ref");
    });

    it("marks a reference inferred from a same-named column", () => {
      render(<Value value={reference({ by: { kind: "sameName", column: "ID", indexed: true } })} onOpen={vi.fn()} />);
      expect(screen.getByText("inferred")).toBeInTheDocument();
      expect(screen.getByRole("link", { name: "12" })).toHaveClass("inferred");
    });

    it("shows no inferred note for a declared reference", () => {
      render(<Value value={reference()} onOpen={vi.fn()} />);
      expect(screen.queryByText("inferred")).not.toBeInTheDocument();
      expect(screen.getByRole("link", { name: "12" })).not.toHaveClass("inferred");
    });

    it("is not a link when the target row does not exist", () => {
      render(<Value value={reference({ exists: false })} onOpen={vi.fn()} />);
      expect(screen.queryByRole("link")).not.toBeInTheDocument();
      expect(screen.getByText("12")).toHaveClass("val-dang");
    });
  });

  describe("no reference", () => {
    it("dims a zero", () => {
      render(<Value value={value({ raw: num("0"), kind: { kind: "noReference" } })} />);
      expect(screen.getByText("0")).toHaveClass("val-o3");
    });

    it("shows NULL in a reference column as NULL", () => {
      render(<Value value={value({ raw: { t: "null" }, kind: { kind: "noReference" } })} />);
      expect(screen.getByText("NULL")).toHaveClass("val-null");
    });
  });

  describe("enum", () => {
    const status = value({ raw: text("wc-completed"), kind: { kind: "enum" } });

    it("is a button that opens the value on click", async () => {
      const onOpen = vi.fn();
      render(<Value value={status} onOpen={onOpen} />);
      await userEvent.click(screen.getByRole("button", { name: "wc-completed" }));
      expect(onOpen).toHaveBeenCalledWith(status, expect.anything());
    });

    it("is plain text without an open handler", () => {
      render(<Value value={status} />);
      expect(screen.queryByRole("button")).not.toBeInTheDocument();
      expect(screen.getByText("wc-completed")).toHaveClass("val-enum");
    });
  });

  it("shows the currency after the amount", () => {
    const eur = value({ raw: text("EUR") });
    const { container } = render(<Value value={value({ raw: num("49.90"), kind: { kind: "money", currency: eur } })} />);
    expect(container).toHaveTextContent(/^49\.90EUR$/);
    expect(screen.getByText("EUR")).toHaveClass("val-sec");
  });

  describe("timestamp", () => {
    beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true, now: NOW }));
    afterEach(() => vi.useRealTimers());

    it("shows the stored text, then the relative time", () => {
      const utc = NOW / 1000 - 2 * 3600;
      const { container } = render(<Value value={value({ raw: { t: "temporal", v: "2026-01-15 10:00:00" }, kind: { kind: "timestamp", utc, zone: "utc" } })} />);
      expect(container).toHaveTextContent(/^2026-01-15 10:00:002 h ago$/);
    });

    it("shows only the stored text when the zone is not stored", () => {
      const { container } = render(<Value value={value({ raw: { t: "temporal", v: "2026-01-15 10:00:00" }, kind: { kind: "timestamp", utc: null, zone: "local" } })} />);
      expect(container).toHaveTextContent(/^2026-01-15 10:00:00$/);
    });
  });

  describe("null and empty", () => {
    it("shows NULL with the configured label", () => {
      connectedStore({ settings: { nullLabel: "∅" } });
      render(<Value value={value({ raw: { t: "null" }, kind: { kind: "null" } })} />);
      expect(screen.getByText("∅")).toHaveClass("val-null");
      expect(screen.queryByText("NULL")).not.toBeInTheDocument();
    });

    it("shows an empty string as two quotes, apart from NULL", () => {
      render(<Value value={value({ raw: text(""), kind: { kind: "empty" } })} />);
      expect(screen.getByText('""')).toHaveClass("val-o3");
      expect(screen.queryByText("NULL")).not.toBeInTheDocument();
    });
  });

  describe("serialized", () => {
    const raw = text('a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}');
    const serialized = value({ raw, kind: { kind: "serialized", format: "php", preview: "a:3:{…}" } });

    it("shows the preview before the text in a grid", () => {
      render(<Value value={serialized} density="grid" />);
      expect(screen.getByText("▸ a:3:{…}")).toBeInTheDocument();
      expect(screen.getByText("a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}")).toBeInTheDocument();
    });

    it("opens the value on click in a grid", async () => {
      const onOpen = vi.fn();
      render(<Value value={serialized} density="grid" onOpen={onOpen} />);
      await userEvent.click(screen.getByRole("button", { name: "▸ a:3:{…}" }));
      expect(onOpen).toHaveBeenCalledWith(serialized, expect.anything());
    });

    it("expands to a decoded tree in place at full density", async () => {
      const decoded: DecodedValue = {
        format: "php",
        root: { kind: "array", children: [{ key: "0", keyKind: "int", node: { kind: "int", text: "1" } }] },
        warning: null,
        error: null,
      };
      mocked.decodeValue.mockResolvedValue(decoded);
      render(<Value value={serialized} />);
      expect(screen.queryByRole("tree")).not.toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: "Expand a:3:{…}" }));

      expect(await screen.findByRole("tree", { name: "PHP value" })).toBeInTheDocument();
      expect(mocked.decodeValue).toHaveBeenCalledWith("a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}");
      expect(screen.getByRole("button", { name: "Collapse a:3:{…}" })).toHaveAttribute("aria-expanded", "true");
    });
  });

  it("keeps a secret masked until it is revealed", async () => {
    render(<Value value={value({ raw: text("sk_live_abc"), kind: { kind: "secret" } })} />);
    expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Reveal secret" }));
    expect(screen.getByText("sk_live_abc")).toBeInTheDocument();
  });

  it("shows the size of binary data", () => {
    render(<Value value={value({ raw: { t: "bytes", v: { len: 2, hexPreview: "cafe" } }, kind: { kind: "binary" } })} />);
    expect(screen.getByText("<2 bytes>")).toHaveClass("val-o3");
  });

  it("colors the value with its scheme style", () => {
    const { container } = render(<Value value={value({ style: "red" })} />);
    expect(root(container).getAttribute("style")).toContain("var(--style-red)");
  });

  it("shows secondary text after the value", () => {
    render(<Value value={value({ raw: num("12"), secondary: { text: "Jane Doe", source: null } })} />);
    expect(screen.getByText("Jane Doe")).toHaveClass("val-sec");
  });

  describe("status bar data", () => {
    const stored: Provenance = { table: "wp_wc_orders_meta", column: "meta_value", row: [{ column: "id", value: "9123" }] };

    it("carries the provenance text and table for the hover listener", () => {
      const { container } = render(<Value value={value({ source: stored })} />);
      expect(root(container)).toHaveAttribute("data-src", "wp_wc_orders_meta.meta_value · id 9123");
      expect(root(container)).toHaveAttribute("data-table", "wp_wc_orders_meta");
    });

    it("has no provenance for a value without a source", () => {
      const { container } = render(<Value value={value()} />);
      expect(root(container)).not.toHaveAttribute("data-src");
      expect(root(container)).not.toHaveAttribute("data-table");
    });

    it("is a Stage anchor named by table, column and row key when it acts on click", () => {
      const { container } = render(<Value value={value({ raw: text("wc-completed"), kind: { kind: "enum" }, source: stored })} onOpen={vi.fn()} />);
      expect(root(container)).toHaveAttribute("data-stage", "value:wp_wc_orders_meta.meta_value:9123");
      expect(root(container)).toHaveAttribute("data-stage-label", "wc-completed");
      expect(root(container)).toHaveAttribute("data-stage-list", "values");
    });

    it("is no Stage anchor when it does nothing on click", () => {
      const { container } = render(<Value value={value({ raw: text("wc-completed"), kind: { kind: "enum" }, source: stored })} />);
      expect(root(container)).not.toHaveAttribute("data-stage");
    });
  });
});
