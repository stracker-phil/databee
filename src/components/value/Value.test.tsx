import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type Address, type DecodedValue, type Provenance, type ReferenceSource, type Value as StoredValue } from "../../api";
import { address, formatAddress } from "../../lib/address";
import { connectedStore, num, tableTab, text, value } from "../../test/fixtures";
import { NOW } from "../../test/now";
import { ColumnContext } from "./open";
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

    describe("icons", () => {
      const target = formatAddress(row("wp_users", "12"));
      const goTo = () => screen.getByRole("button", { name: `Go to ${target}` });
      const sidePanel = () => screen.getByRole("button", { name: `Open ${target} in side panel` });
      const sourced = (key: { column: string; value: string }[] = [{ column: "ID", value: "7" }]): StoredValue => ({
        ...reference(),
        source: { table: "wp_posts", column: "post_author", row: key },
      });
      const inColumn = (ui: ReactNode) => (
        <ColumnContext.Provider value={{ tabId: "t1", index: 0 }}>{ui}</ColumnContext.Provider>
      );

      it("offers only Go to outside a column stack and a table tab", () => {
        render(<Value value={reference()} onOpen={vi.fn()} />);
        expect(goTo()).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: /in side panel/ })).not.toBeInTheDocument();
      });

      it("calls onOpen with mode go for Go to", async () => {
        const onOpen = vi.fn();
        const ref = reference();
        render(<Value value={ref} onOpen={onOpen} />);
        await userEvent.click(goTo());
        expect(onOpen).toHaveBeenCalledOnce();
        expect(onOpen).toHaveBeenCalledWith(ref, expect.anything(), "go");
      });

      it("offers both icons inside a column stack and calls onOpen with mode drill for the side panel", async () => {
        const onOpen = vi.fn();
        const ref = reference();
        render(inColumn(<Value value={ref} onOpen={onOpen} />));
        expect(goTo()).toBeInTheDocument();
        await userEvent.click(sidePanel());
        expect(onOpen).toHaveBeenCalledOnce();
        expect(onOpen).toHaveBeenCalledWith(ref, expect.anything(), "drill");
      });

      it("names every target of a reference with several targets", () => {
        const both = value({ raw: num("12"), kind: { kind: "reference", targets: [row("wp_users", "12"), row("wp_posts", "12")], by: preset, exists: null } });
        render(<Value value={both} onOpen={vi.fn()} />);
        expect(screen.getByRole("button", { name: `Go to ${target} or ${formatAddress(row("wp_posts", "12"))}` })).toBeInTheDocument();
      });

      it("has the icons for an inferred reference", () => {
        render(inColumn(<Value value={reference({ by: { kind: "sameName", column: "ID", indexed: true } })} onOpen={vi.fn()} />));
        expect(goTo()).toBeInTheDocument();
        expect(sidePanel()).toBeInTheDocument();
      });

      it("has no icons for a dangling reference", () => {
        render(inColumn(<Value value={reference({ exists: false })} onOpen={vi.fn()} />));
        expect(screen.queryByRole("button")).not.toBeInTheDocument();
      });

      it("has no icons without an open handler", () => {
        render(inColumn(<Value value={reference()} />));
        expect(screen.queryByRole("button")).not.toBeInTheDocument();
      });

      it("has no icons for a value that is not a reference", () => {
        const key = value({ raw: num("7"), kind: { kind: "primaryKey", address: row("wp_posts", "7") } });
        render(inColumn(<Value value={key} onOpen={vi.fn()} />));
        expect(screen.queryByRole("button")).not.toBeInTheDocument();
      });

      describe("in a table tab", () => {
        beforeEach(() => connectedStore({ tabs: [tableTab()] }));

        it("offers both icons for a value sourced from a single-column key", () => {
          render(<Value value={sourced()} onOpen={vi.fn()} />);
          expect(goTo()).toBeInTheDocument();
          expect(sidePanel()).toBeInTheDocument();
        });

        it("offers only Go to for a value without a source", () => {
          render(<Value value={reference()} onOpen={vi.fn()} />);
          expect(goTo()).toBeInTheDocument();
          expect(screen.queryByRole("button", { name: /in side panel/ })).not.toBeInTheDocument();
        });

        it("offers only Go to for a value from a multi-column key", () => {
          render(
            <Value
              value={sourced([
                { column: "post_id", value: "7" },
                { column: "meta_key", value: "k" },
              ])}
              onOpen={vi.fn()}
            />,
          );
          expect(goTo()).toBeInTheDocument();
          expect(screen.queryByRole("button", { name: /in side panel/ })).not.toBeInTheDocument();
        });
      });
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

  describe("secret", () => {
    const secret = (raw: string) => value({ raw: text(raw), kind: { kind: "secret" } });
    const hoverArea = (button: HTMLElement) => button.parentElement as HTMLElement;
    const reveal = () => screen.getByRole("button", { name: "Reveal secret" });

    it("is masked by default and explains how to reveal it", () => {
      render(<Value value={secret("sk_live_abc")} />);
      expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
      expect(reveal()).toHaveAttribute("title", "Click to reveal · ⌥ hover to peek");
    });

    it("shows the text and focuses it after a click", async () => {
      render(<Value value={secret("sk_live_abc")} />);
      await userEvent.click(reveal());
      expect(screen.getByText("sk_live_abc")).toHaveFocus();
      expect(screen.queryByRole("button", { name: "Reveal secret" })).not.toBeInTheDocument();
    });

    it("masks the text again when it loses focus", async () => {
      const user = userEvent.setup();
      render(<Value value={secret("sk_live_abc")} />);
      await user.click(reveal());
      await user.tab();
      expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
      expect(reveal()).toBeInTheDocument();
    });

    it.each([
      ["Enter", "{Enter}"],
      ["Space", " "],
    ])("reveals the text when %s is pressed on the focused button", async (_name, keys) => {
      const user = userEvent.setup();
      render(<Value value={secret("sk_live_abc")} />);
      await user.tab();
      expect(reveal()).toHaveFocus();
      await user.keyboard(keys);
      expect(screen.getByText("sk_live_abc")).toBeInTheDocument();
    });

    describe("peeking with Alt", () => {
      it("shows the hovered secret while Alt is pressed and masks it on release", async () => {
        const user = userEvent.setup();
        render(<Value value={secret("sk_live_abc")} />);
        fireEvent.mouseEnter(hoverArea(reveal()));
        await user.keyboard("{Alt>}");
        expect(screen.getByText("sk_live_abc")).toBeInTheDocument();
        await user.keyboard("{/Alt}");
        expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
      });

      it("shows the secret when the pointer arrives after Alt, and masks it when the pointer leaves", async () => {
        const user = userEvent.setup();
        render(<Value value={secret("sk_live_abc")} />);
        const area = hoverArea(reveal());
        await user.keyboard("{Alt>}");
        expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
        fireEvent.mouseEnter(area);
        expect(screen.getByText("sk_live_abc")).toBeInTheDocument();
        fireEvent.mouseLeave(area);
        expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
      });

      it.each([
        ["Meta", "{Meta>}{Alt>}"],
        ["Ctrl", "{Control>}{Alt>}"],
        ["Shift", "{Shift>}{Alt>}"],
      ])("does not reveal on hover when %s is held with Alt", async (_name, keys) => {
        const user = userEvent.setup();
        render(<Value value={secret("sk_live_abc")} />);
        fireEvent.mouseEnter(hoverArea(reveal()));
        await user.keyboard(keys);
        expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
      });

      it("masks the secret when the window loses focus", async () => {
        const user = userEvent.setup();
        render(<Value value={secret("sk_live_abc")} />);
        fireEvent.mouseEnter(hoverArea(reveal()));
        await user.keyboard("{Alt>}");
        expect(screen.getByText("sk_live_abc")).toBeInTheDocument();
        fireEvent.blur(window);
        expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
      });

      it("reveals nothing without hover, and only the hovered secret among several", async () => {
        const user = userEvent.setup();
        render(
          <>
            <Value value={secret("first_secret")} />
            <Value value={secret("second_secret")} />
          </>,
        );
        await user.keyboard("{Alt>}");
        expect(screen.queryByText("first_secret")).not.toBeInTheDocument();
        expect(screen.queryByText("second_secret")).not.toBeInTheDocument();

        fireEvent.mouseEnter(hoverArea(screen.getAllByRole("button", { name: "Reveal secret" })[1]));
        expect(screen.getByText("second_secret")).toBeInTheDocument();
        expect(screen.queryByText("first_secret")).not.toBeInTheDocument();
      });

      it("keeps a peeked secret shown after Alt is released once it has focus, until it blurs", async () => {
        const user = userEvent.setup();
        render(<Value value={secret("sk_live_abc")} />);
        fireEvent.mouseEnter(hoverArea(reveal()));
        await user.keyboard("{Alt>}");
        await user.click(screen.getByText("sk_live_abc"));
        await user.keyboard("{/Alt}");
        expect(screen.getByText("sk_live_abc")).toBeInTheDocument();
        await user.tab();
        expect(screen.queryByText("sk_live_abc")).not.toBeInTheDocument();
      });
    });
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
