import { describe, expect, it } from "vitest";
import type { Address } from "../api";
import shared from "../test/fixtures/addresses.json";
import { AddressError,formatAddress, parseAddress, sameAddress } from "./address";

const cases = shared as unknown as { text: string; address: Address }[];

describe("addresses shared with Rust", () => {
  it("lists at least one case", () => {
    expect(cases.length).toBeGreaterThan(0);
  });

  it.each(cases.map((c) => [c.text, c] as const))("parses and formats %s like Rust", (_text, { text, address }) => {
    expect(parseAddress(text)).toEqual(address);
    expect(formatAddress(address)).toBe(text);
  });
});

describe("parseAddress", () => {
  it("reads a table with column filters", () => {
    expect(parseAddress("databee://shop/db/wp_posts?post_type=page&post_status=publish")).toEqual({
      connection: "shop",
      database: "db",
      target: { kind: "table", table: "wp_posts", filter: [{ column: "post_type", value: "page" }, { column: "post_status", value: "publish" }] },
      view: null,
      compare: null,
    });
  });

  it("reads view and compare of a row", () => {
    expect(parseAddress("databee://shop/db/wp_posts/7?view=rows&compare=9")).toMatchObject({
      target: { kind: "row", table: "wp_posts", key: "7" },
      view: "rows",
      compare: "9",
    });
  });

  it("decodes percent escapes", () => {
    expect(parseAddress("databee://shop/db/wp_posts?post_title=Hello%20world%21").target).toEqual({
      kind: "table",
      table: "wp_posts",
      filter: [{ column: "post_title", value: "Hello world!" }],
    });
  });

  it.each([
    ["a missing scheme", "shop/db/wp_posts"],
    ["a missing database", "databee://shop"],
    ["an empty connection", "databee:///db/wp_posts"],
    ["an empty database", "databee://shop//wp_posts"],
    ["an empty key", "databee://shop/db/wp_posts/"],
    ["an empty parameter name", "databee://shop/db/wp_posts?=page"],
    ["a broken percent escape", "databee://shop/db/wp_posts?post_type=%zz"],
    ["an unknown view", "databee://shop/db/wp_posts/7?view=x"],
    ["a repeated view", "databee://shop/db/wp_posts/7?view=rows&view=document"],
    ["an unknown row parameter", "databee://shop/db/wp_posts/7?foo=1"],
    ["more than two path segments", "databee://shop/db/wp_posts/7/extra"],
    ["parameters on a saved query", "databee://shop/db/q/abc?x=1"],
    ["parameters on the database home besides find or project", "databee://shop/db/?x=1"],
  ])("rejects %s", (_what, text) => {
    expect(() => parseAddress(text)).toThrow(AddressError);
  });
});

describe("formatAddress", () => {
  const base = { connection: "shop", database: "db", view: null, compare: null } as const;

  it("writes the database home with a trailing slash", () => {
    expect(formatAddress({ ...base, target: { kind: "home" } })).toBe("databee://shop/db/");
  });

  it("escapes a table literally named q so it is not a saved query", () => {
    const text = formatAddress({ ...base, target: { kind: "table", table: "q", filter: [] } });
    expect(text).toBe("databee://shop/db/%71");
    expect(parseAddress(text).target).toEqual({ kind: "table", table: "q", filter: [] });
  });

  it("escapes reserved characters in values", () => {
    const text = formatAddress({ ...base, target: { kind: "table", table: "t", filter: [{ column: "c", value: "a&b=c d" }] } });
    expect(text).toBe("databee://shop/db/t?c=a%26b%3Dc%20d");
  });

  it("encodes non-ASCII text as uppercase UTF-8 escapes", () => {
    expect(formatAddress({ ...base, target: { kind: "table", table: "t", filter: [{ column: "c", value: "ä" }] } })).toBe("databee://shop/db/t?c=%C3%A4");
  });
});

describe("sameAddress", () => {
  it("compares by canonical text", () => {
    const a = parseAddress("databee://shop/db/wp_posts/7");
    expect(sameAddress(a, { ...a })).toBe(true);
    expect(sameAddress(a, parseAddress("databee://shop/db/wp_posts/8"))).toBe(false);
  });
});
