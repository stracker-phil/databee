import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type Address, type DecodedValue, type Provenance, type ReferenceSource, type Value as StoredValue } from "../../api";
import { address } from "../../lib/address";
import { useStore } from "../../state/store";
import { connectedStore, num, tableTab, text, value } from "../../test/fixtures";
import { NOW } from "../../test/now";
import { rulesInfoOf, showTableIcons } from "../../test/iconFixtures";
import { ColumnContext } from "./open";
import { useOpenValue } from "./useOpenValue";
import { Value, type OpenValue } from "./Value";

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

  describe("lead table", () => {
    const toTable = (...targets: Address[]) => value({ raw: num("12"), kind: { kind: "reference", targets, by: preset, exists: null } });

    it.each([
      ["a reference into one table", toTable(row("wp_users", "12")), "wp_users"],
      ["a reference to several rows of one table", toTable(row("wp_users", "12"), row("wp_users", "13")), "wp_users"],
      ["a primary key", value({ raw: num("7"), kind: { kind: "primaryKey", address: row("wp_posts", "7") } }), "wp_posts"],
    ])("names the table of %s", (_name, stored, table) => {
      const { container } = render(<Value value={stored} />);
      expect(root(container)).toHaveAttribute("data-lead-table", table);
    });

    it("names no table for a reference into several tables", () => {
      const { container } = render(<Value value={toTable(row("wp_users", "12"), row("wp_posts", "12"))} />);
      expect(root(container)).not.toHaveAttribute("data-lead-table");
    });

    it("names no table for a plain number", () => {
      const { container } = render(<Value value={value({ raw: num("7"), kind: { kind: "number" } })} />);
      expect(root(container)).not.toHaveAttribute("data-lead-table");
    });
  });

  describe("reference", () => {
    it("is a link that opens the value on click", async () => {
      const onOpen = vi.fn<OpenValue>();
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
      render(<Value value={reference({ by: { kind: "sameName", column: "ID", indexed: true } })} onOpen={vi.fn<OpenValue>()} />);
      expect(screen.getByText("inferred")).toBeInTheDocument();
      expect(screen.getByRole("link", { name: "12" })).toHaveClass("inferred");
    });

    it("shows no inferred note for a declared reference", () => {
      render(<Value value={reference()} onOpen={vi.fn<OpenValue>()} />);
      expect(screen.queryByText("inferred")).not.toBeInTheDocument();
      expect(screen.getByRole("link", { name: "12" })).not.toHaveClass("inferred");
    });

    it("is not a link when the target row does not exist", () => {
      render(<Value value={reference({ exists: false })} onOpen={vi.fn<OpenValue>()} />);
      expect(screen.queryByRole("link")).not.toBeInTheDocument();
      expect(screen.getByText("12")).toHaveClass("val-dang");
    });

    describe("target table icon", () => {
      const twoTables = value({ raw: num("12"), kind: { kind: "reference", targets: [row("wp_users", "12"), row("wp_posts", "12")], by: preset, exists: null } });
      const sameTable = value({ raw: num("12"), kind: { kind: "reference", targets: [row("wp_users", "12"), row("wp_users", "13")], by: preset, exists: null } });
      const icon = (container: HTMLElement) => container.querySelector("[data-icon]");

      it("shows the icon of the target's table before the key", () => {
        showTableIcons({ wp_users: "user" });
        const { container } = render(<Value value={reference()} />);
        const glyph = container.querySelector("[data-icon=user]")!;
        expect(glyph).toHaveAttribute("aria-hidden", "true");
        expect(screen.getByText("12")).toContainElement(glyph as HTMLElement);
      });

      it("shows the icon when every target is in the same table", () => {
        showTableIcons({ wp_users: "user" });
        const { container } = render(<Value value={sameTable} />);
        expect(icon(container)).toHaveAttribute("data-icon", "user");
      });

      it("shows no icon when the targets are in different tables", () => {
        showTableIcons({ wp_users: "user", wp_posts: "file-text" });
        const { container } = render(<Value value={twoTables} />);
        expect(icon(container)).toBeNull();
      });

      it("shows the default table icon for a table without a rule icon", () => {
        showTableIcons({ wp_posts: "file-text" });
        const { container } = render(<Value value={reference()} />);
        expect(icon(container)).toHaveAttribute("data-icon", "table");
      });

      it("loads the database's icons when they are not known yet", async () => {
        mocked.rulesInfo.mockResolvedValue(rulesInfoOf({ wp_users: "user" }));
        const { container } = render(<Value value={reference()} />);
        await waitFor(() => expect(icon(container)).toHaveAttribute("data-icon", "user"));
        expect(mocked.rulesInfo).toHaveBeenCalledWith("s1", "db");
      });
    });
  });

  describe("click indicator", () => {
    const opener = (value: StoredValue, density: "grid" | "full", owner: Address | null) => {
      function Opens() {
        const { openValue, targetMenu } = useOpenValue(owner);
        return (
          <>
            <Value value={value} density={density} onOpen={openValue} />
            {targetMenu}
          </>
        );
      }
      return <Opens />;
    };
    const inColumn = (ui: ReactNode) => <ColumnContext.Provider value={{ tabId: "t1", index: 0 }}>{ui}</ColumnContext.Provider>;
    const wholePosts = address("shop", "db", { kind: "table", table: "wp_posts", filter: [] });
    const key = (target: Address) => value({ raw: num("7"), kind: { kind: "primaryKey", address: target } });
    const serialized = value({ raw: text("a:0:{}"), kind: { kind: "serialized", format: "php", preview: "a:0:{}" } });
    const toTable = value({ raw: num("12"), kind: { kind: "reference", targets: [wholePosts], by: preset, exists: true } });
    const toRows = value({ raw: num("12"), kind: { kind: "reference", targets: [row("wp_users", "12"), row("wp_users", "13")], by: preset, exists: true } });

    it.each<[string, StoredValue, "grid" | "full", Address | null, boolean, "drill" | "go" | null]>([
      ["a reference in a column stack", reference(), "full", null, true, "drill"],
      ["a reference with several row targets in a column stack", toRows, "full", null, true, "drill"],
      ["a reference to a table in a column stack", toTable, "full", null, true, "go"],
      ["a reference outside a column stack", reference(), "grid", null, false, "go"],
      ["another row's key in a column stack", key(row("wp_posts", "7")), "full", row("wp_posts", "8"), true, "drill"],
      ["another row's key outside a column stack", key(row("wp_posts", "7")), "grid", null, false, "go"],
      ["the key of the shown row", key(row("wp_posts", "7")), "full", row("wp_posts", "7"), true, null],
      ["a table's key", key(wholePosts), "full", null, true, null],
      ["serialized data in a grid", serialized, "grid", null, false, "drill"],
      ["serialized data in a grid in a column stack", serialized, "grid", null, true, "drill"],
      ["serialized data in a document", serialized, "full", null, true, null],
      ["an enum", value({ raw: text("wc-completed"), kind: { kind: "enum" } }), "grid", null, false, null],
      ["a dangling reference", reference({ exists: false }), "full", null, true, null],
    ])("indicates the click effect for %s", (_name, stored, density, owner, stacked, expected) => {
      const ui = opener(stored, density, owner);
      const { container } = render(stacked ? inColumn(ui) : ui);
      expect(container.querySelector("[data-open]")?.getAttribute("data-open") ?? null).toBe(expected);
    });

    it("is decorative: hidden from assistive technology and not a button", () => {
      const { container } = render(opener(reference(), "grid", null));
      const hint = container.querySelector("[data-open]")!;
      expect(hint).toHaveAttribute("aria-hidden", "true");
      expect(hint.closest("button")).toBeNull();
      expect(screen.queryByRole("button")).not.toBeInTheDocument();
    });

    it("is absent without an open handler", () => {
      const { container } = render(<Value value={reference()} />);
      expect(container.querySelector("[data-open]")).toBeNull();
    });

    it("has no hover overlay beside the value", () => {
      const { container } = render(opener(reference(), "grid", null));
      expect(container.querySelector(".val-acts")).toBeNull();
    });

    describe("leading table icon", () => {
      const twoTables = value({ raw: num("12"), kind: { kind: "reference", targets: [row("wp_users", "12"), row("wp_posts", "12")], by: preset, exists: true } });
      const swapOf = (container: HTMLElement) => container.querySelector(".open-swap");

      beforeEach(() => showTableIcons({ wp_posts: "file-text", wp_users: "user" }));

      it.each<["grid" | "full", boolean]>([
        ["grid", true],
        ["grid", false],
        ["full", true],
        ["full", false],
      ])("shows the table icon of a key with a row address in %s density (open handler: %s)", (density, handler) => {
        const { container } = render(<Value value={key(row("wp_posts", "7"))} density={density} onOpen={handler ? vi.fn<OpenValue>() : undefined} />);
        expect(container.querySelector("[data-icon=file-text]")).not.toBeNull();
      });

      it("swaps the icon of another row's key for the drill indicator in a column stack", () => {
        const { container } = render(inColumn(opener(key(row("wp_posts", "7")), "full", row("wp_posts", "8"))));
        expect(container.querySelector(".open-swap[data-open=drill] [data-icon=file-text]")).not.toBeNull();
      });

      it("swaps the icon of another row's key for the go indicator outside a column stack", () => {
        const { container } = render(opener(key(row("wp_posts", "7")), "grid", null));
        expect(container.querySelector(".open-swap[data-open=go] [data-icon=file-text]")).not.toBeNull();
      });

      it("shows the icon of the shown row's own key without an indicator", () => {
        const { container } = render(inColumn(opener(key(row("wp_posts", "7")), "full", row("wp_posts", "7"))));
        expect(container.querySelector("[data-icon=file-text]")).not.toBeNull();
        expect(container.querySelector("[data-open]")).toBeNull();
      });

      it("shows no icon for a key with a table address", () => {
        const { container } = render(opener(key(wholePosts), "full", null));
        expect(container.querySelector("[data-icon]")).toBeNull();
      });

      it("swaps the icon of a reference into one table for the indicator", () => {
        const { container } = render(opener(reference(), "grid", null));
        expect(container.querySelector(".open-swap[data-open=go] [data-icon=user]")).not.toBeNull();
        expect(container.querySelector(".open-hint")).toBeNull();
      });

      it("shows an inline indicator after a reference into several tables, without an icon", () => {
        const { container } = render(opener(twoTables, "grid", null));
        expect(container.querySelector("[data-icon]")).toBeNull();
        expect(swapOf(container)).toBeNull();
        expect(container.querySelector(".open-hint[data-open]")).not.toBeNull();
      });

      it("shows no inline indicator after a reference into several tables without an open handler", () => {
        const { container } = render(<Value value={twoTables} />);
        expect(container.querySelector("[data-open]")).toBeNull();
      });
    });

    describe("serialized marker", () => {
      it("swaps the ▸ of serialized data in a grid for the drill indicator", () => {
        const { container } = render(opener(serialized, "grid", null));
        expect(container.querySelector(".open-swap[data-open=drill] .ser-mark")).toHaveTextContent("▸");
      });

      it("keeps a plain ▸ without an open handler", () => {
        const { container } = render(<Value value={serialized} density="grid" />);
        expect(container.querySelector("[data-open]")).toBeNull();
        expect(screen.getByText("▸ a:0:{}")).toBeInTheDocument();
      });
    });

    it("leaves the click to the value, which still acts", async () => {
      connectedStore({ tabs: [tableTab()] });
      render(opener(reference(), "grid", null));
      await userEvent.click(screen.getByRole("link", { name: "12" }));
      expect(useStore.getState().tabs[0]).toMatchObject({ kind: "object", address: { target: { table: "wp_users", key: "12" } } });
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
      const onOpen = vi.fn<OpenValue>();
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
      const onOpen = vi.fn<OpenValue>();
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

    describe("watching a node of the expanded tree", () => {
      const stored: Provenance = { table: "wp_options", column: "option_value", row: [{ column: "option_name", value: "cfg" }] };
      const decoded: DecodedValue = {
        format: "php",
        root: { kind: "array", children: [{ key: "mode", keyKind: "string", node: { kind: "string", text: "live" } }] },
        warning: null,
        error: null,
      };

      async function expand(source?: Provenance) {
        connectedStore({ tabs: [tableTab()] });
        mocked.decodeValue.mockResolvedValue(decoded);
        render(<Value value={value({ raw, kind: { kind: "serialized", format: "php", preview: "a:3:{…}" }, source })} />);
        await userEvent.click(screen.getByRole("button", { name: "Expand a:3:{…}" }));
        await screen.findByRole("tree", { name: "PHP value" });
      }

      it("sends the value's source and the node's path to the readout", async () => {
        await expand(stored);
        fireEvent.contextMenu(screen.getByText("mode"));
        await userEvent.click(screen.getByRole("menuitem", { name: "Watch on readout" }));
        expect(mocked.readoutWatch).toHaveBeenCalledWith("s1", "db", stored, ["mode"]);
      });

      it("offers no Watch on readout for a value without a source", async () => {
        await expand();
        fireEvent.contextMenu(screen.getByText("mode"));
        expect(screen.queryByRole("menuitem", { name: "Watch on readout" })).not.toBeInTheDocument();
      });
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
      const { container } = render(<Value value={value({ raw: text("wc-completed"), kind: { kind: "enum" }, source: stored })} onOpen={vi.fn<OpenValue>()} />);
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
